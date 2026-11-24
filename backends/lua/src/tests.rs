//! The descriptor, the profile table against the files of `profiles/`, and
//! the rules over small programs.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use flick_backend::{
    Abi, Backend, BackendId, BackendPin, BuildRequest, Entry, Environment, InputError, Inputs,
    Mode, ModuleInput, ModuleRole, Profile, Symbol, TargetRequest, validate_foreign_metadata,
    validate_options, validate_tags, validate_value_abi,
};
use flick_ir::{
    Adapter, Arith, BinOp, Binding, Exports, Expr, ExprKind, FileId, Foreign, ForeignMode,
    Initialization, Item, ItemKind, Lit, Module, Num, Param, Pat, Program, Span, Stage,
};
use toml::{Table, Value};

use super::Lua;
use crate::profiles::PROFILES;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn the_descriptor_passes_the_handshake_checks_and_matches_the_manifest() {
    let descriptor = Lua.describe();
    validate_tags(&descriptor).expect("tags");
    validate_options(&descriptor).expect("options");
    validate_foreign_metadata(&descriptor).expect("foreign metadata");
    validate_value_abi(&descriptor).expect("value ABI");
    let manifest: Table = fs::read_to_string(root().join("Cargo.toml"))
        .expect("manifest")
        .parse()
        .expect("TOML");
    let flick = &manifest["package"]["metadata"]["flick"];
    for (key, range) in [
        ("backend-api", &descriptor.backend_api),
        ("ir", &descriptor.ir),
    ] {
        assert_eq!(
            flick[key]["minimum"].as_str(),
            Some(range.minimum.as_str()),
            "{key}"
        );
        assert_eq!(
            flick[key]["maximum-exclusive"].as_str(),
            Some(range.maximum_exclusive.as_str()),
            "{key}"
        );
    }
    assert_eq!(
        manifest["package"]["version"].as_str(),
        Some(descriptor.identity.version.as_str())
    );
}

fn profile(id: &str) -> Table {
    fs::read_to_string(root().join(format!("profiles/{id}.toml")))
        .expect("profile")
        .parse()
        .expect("TOML")
}

fn facts(table: &Table) -> BTreeMap<&str, &Value> {
    let facts = table["fact"].as_array().expect("facts");
    facts
        .iter()
        .map(|f| (f["name"].as_str().expect("a named fact"), &f["value"]))
        .collect()
}

#[test]
fn the_profile_table_is_what_the_profile_files_record() {
    for p in PROFILES {
        let table = profile(p.id);
        assert_eq!(
            table["environment"].as_str(),
            Some(format!("lua/{}", p.platform).as_str())
        );
        let facts = facts(&table);
        let dialects: Vec<&str> = (facts["dialects"].as_array().expect("dialects").iter())
            .map(|d| d.as_str().expect("a dialect"))
            .collect();
        assert_eq!(dialects, p.dialects, "{}", p.id);
        for (name, value) in [
            ("jit", p.luajit),
            ("ffi", p.luajit),
            ("require", p.require),
            ("suspension", p.suspension),
        ] {
            assert_eq!(facts[name].as_bool(), Some(value), "{} {name}", p.id);
        }
        let api = fs::read_to_string(
            root()
                .join("profiles")
                .join(table["api"].as_str().expect("api")),
        );
        assert_eq!(api.expect("snapshot"), p.api, "{}", p.id);
    }
}

#[test]
fn every_fact_carries_its_evidence() {
    let probes = root().join("probes");
    for entry in fs::read_dir(root().join("profiles"))
        .expect("profiles")
        .flatten()
    {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "toml") {
            continue;
        }
        let table: Table = fs::read_to_string(&path)
            .expect("profile")
            .parse()
            .expect("TOML");
        for fact in table["fact"].as_array().expect("facts") {
            let what = format!("{}: {}", path.display(), fact["name"]);
            assert!(fact.get("value").is_some(), "{what}: no value");
            match (
                fact.get("doc").and_then(Value::as_str),
                fact.get("exec").and_then(Value::as_str),
            ) {
                (Some(doc), None) => assert!(
                    doc.starts_with("https://")
                        || root()
                            .join("profiles")
                            .join(doc.split('#').next().unwrap_or(doc))
                            .is_file(),
                    "{what}: `{doc}` is neither a link nor a file"
                ),
                (None, Some(probe)) => {
                    assert!(
                        probe == "api" || probes.join(format!("{probe}.lua")).is_file(),
                        "{what}: no probe `{probe}`"
                    );
                    for key in ["host", "date"] {
                        assert!(
                            fact.get(key).and_then(Value::as_str).is_some(),
                            "{what}: no {key}"
                        );
                    }
                }
                _ => panic!("{what}: one of `doc` and `exec`"),
            }
        }
    }
}

fn span(start: u32, end: u32) -> Span {
    Span::new(FileId::new(0), start, end)
}

fn expr(kind: ExprKind, start: u32, end: u32) -> Expr {
    Expr::new(kind, span(start, end))
}

/// `let name () = body`.
fn function(name: &str, body: Expr) -> Item {
    let at = body.span;
    Item {
        kind: ItemKind::Value {
            name: name.into(),
            type_params: vec![],
            dicts: vec![],
            dict_constraints: vec![],
            ty: None,
            body: Expr::new(
                ExprKind::Lambda {
                    params: vec![Param {
                        pat: Pat::Lit(Lit::Unit),
                        stage: Stage::Pure,
                        ty: None,
                        binder_types: vec![],
                    }],
                    body: Box::new(body),
                },
                at,
            ),
        },
        span: at,
    }
}

fn foreign_global(name: &str, global: &str) -> Item {
    Item {
        kind: ItemKind::Foreign(Foreign {
            name: name.into(),
            source_name: None,
            mode: ForeignMode::Sync,
            binding: Binding::Global {
                name: global.into(),
            },
            stages: vec![true],
            dicts: 0,
            arity: 0,
            adapter: Adapter::default(),
            catch: None,
            type_params: vec![],
            ty: None,
        }),
        span: span(0, 1),
    }
}

fn request(platform: &str, tags: &[&str], items: Vec<Item>) -> BuildRequest {
    let module = Module {
        name: "App".into(),
        items,
        types: vec![],
        preambles: vec![],
        labels: BTreeMap::new(),
        initialization: Initialization::default(),
        records: BTreeMap::new(),
        package: None,
        exports: Exports::default(),
    };
    let program = Program::new("test", vec![module], vec![], &|_| "App.fl".into());
    BuildRequest {
        target: TargetRequest {
            backend_api: "0.1.0".into(),
            ir_version: flick_ir::VERSION.into(),
            language_version: "0.2.0".into(),
            runtime_abis: vec![Abi {
                name: "flickrt".into(),
                version: "0.2.0".into(),
            }],
            environment: Environment {
                backend: "lua".into(),
                platform: platform.into(),
            },
            tags: tags.iter().map(|&t| t.into()).collect(),
            host_package: None,
            profile: Profile::Debug,
            mode: Mode::WholeProgram,
            required_capabilities: vec![],
            options: vec![],
        },
        backend: BackendPin {
            identity: BackendId {
                id: "flick/lua".into(),
                version: "0.1.0".into(),
            },
            sha256: "0".repeat(64),
        },
        program: program.to_cbor(),
        module_root: None,
        modules: vec![ModuleInput {
            name: "App".into(),
            role: ModuleRole::Own,
            package: None,
            output: "App.lua".into(),
            interface: None,
        }],
        sources: vec![],
        entries: vec![Entry {
            name: "main".into(),
            symbol: Symbol {
                module: "App".into(),
                name: "main".into(),
            },
            runner: None,
        }],
        test_roots: vec![],
        foreign: vec![],
        inline_foreign: vec![],
        projections: vec![],
        package_targets: vec![],
        root_handlers: vec![],
        inputs: vec![],
        toolchains: vec![],
        runtime_dependencies: vec![],
        declared_dependencies: None,
    }
}

fn global(name: &str, start: u32, end: u32) -> Expr {
    expr(
        ExprKind::Global {
            module: "App".into(),
            name: name.into(),
            type_args: vec![],
        },
        start,
        end,
    )
}

fn big_product() -> Expr {
    let two = || Box::new(expr(ExprKind::Lit(Lit::BigInt("2".into())), 20, 22));
    expr(
        ExprKind::Binary(BinOp::Arith(Arith::Mul, Num::BigInt), two(), two()),
        20,
        27,
    )
}

fn messages(request: &BuildRequest) -> Vec<(String, Option<(u32, u32)>)> {
    match super::check::check(request) {
        Ok(()) => vec![],
        Err(found) => found
            .into_iter()
            .map(|d| (d.message, d.span.map(|s| (s.start, s.end))))
            .collect(),
    }
}

#[test]
fn a_reachable_big_integer_is_refused_where_it_is_and_an_unreachable_one_is_not() {
    let unit = || expr(ExprKind::Lit(Lit::Unit), 0, 2);
    let reached = request(
        "love",
        &["lua", "lua51"],
        vec![function("main", big_product())],
    );
    let found = messages(&reached);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0]
            .0
            .contains("`BigInt` is not available on the lua/love target"),
        "{found:?}"
    );
    assert_eq!(found[0].1, Some((20, 27)));
    let unreached = request(
        "love",
        &["lua", "lua51"],
        vec![function("main", unit()), function("unused", big_product())],
    );
    assert_eq!(messages(&unreached), vec![]);
}

#[test]
fn a_wide_integer_needs_the_ffi_capability() {
    let literal = || expr(ExprKind::Lit(Lit::Int64(1)), 3, 5);
    let without = messages(&request(
        "love",
        &["lua", "luajit"],
        vec![function("main", literal())],
    ));
    assert!(
        without[0].0.contains("without the `ffi` capability"),
        "{without:?}"
    );
    let with = messages(&request(
        "love",
        &["lua", "luajit", "ffi"],
        vec![function("main", literal())],
    ));
    assert_eq!(with, vec![]);
}

#[test]
fn the_environment_must_name_a_profile_and_the_hosts_dialect() {
    let unit = || vec![function("main", expr(ExprKind::Lit(Lit::Unit), 0, 2))];
    let unknown = messages(&request("love-12", &["lua", "lua51"], unit()));
    assert!(
        unknown[0]
            .0
            .contains("the profile `lua/love-12` is not available"),
        "{unknown:?}"
    );
    let jit = messages(&request("wow-retail", &["lua", "luajit"], unit()));
    assert!(
        jit[0]
            .0
            .contains("the dialect `luajit` is not available on the lua/wow-retail"),
        "{jit:?}"
    );
    let ffi = messages(&request("love", &["lua", "lua51", "ffi"], unit()));
    assert!(ffi[0].0.contains("needs the dialect `luajit`"), "{ffi:?}");
    assert_eq!(
        messages(&request("wow-forever", &["lua", "lua51"], unit())),
        vec![]
    );
}

#[test]
fn a_reachable_global_must_be_in_the_api_snapshot() {
    let call = |global_name: &str| {
        request(
            "wow-retail",
            &["lua", "lua51"],
            vec![
                function("main", global("host", 8, 12)),
                foreign_global("host", global_name),
            ],
        )
    };
    assert_eq!(messages(&call("C_Timer.After")), vec![]);
    let missing = messages(&call("io.open"));
    assert!(
        missing[0]
            .0
            .contains("not in the API snapshot of World of Warcraft Retail"),
        "{missing:?}"
    );
    assert_eq!(missing[0].1, Some((8, 12)));
    let jit = messages(&request(
        "love",
        &["lua", "lua51"],
        vec![
            function("main", global("host", 8, 12)),
            foreign_global("host", "jit.off"),
        ],
    ));
    assert!(jit[0].0.contains("needs `jit`"), "{jit:?}");
}

#[test]
fn generation_is_refused_after_a_clean_check() {
    let clean = request(
        "love",
        &["lua", "lua51"],
        vec![function("main", expr(ExprKind::Lit(Lit::Unit), 0, 2))],
    );
    let refused = Lua
        .generate(&clean, &mut NoInputs)
        .expect_err("no generation");
    assert!(
        refused[0]
            .message
            .contains("code generation is not implemented yet"),
        "{refused:?}"
    );
}

struct NoInputs;
impl Inputs for NoInputs {
    fn read(&mut self, _: &str) -> Result<Vec<u8>, InputError> {
        Err(InputError::Undeclared)
    }
}
