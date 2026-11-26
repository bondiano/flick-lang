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
use crate::emit::kernel_paths;
use crate::profiles::{DIALECTS, PROFILES};

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
fn the_dialect_table_is_what_the_dialect_files_record() {
    for d in DIALECTS {
        let table = profile(d.id);
        assert_eq!(table["kind"].as_str(), Some("dialect"), "{}", d.id);
        let facts = facts(&table);
        let dialects = facts["dialects"].as_array().expect("dialects");
        assert!(
            dialects.iter().any(|t| t.as_str() == Some(d.tag)),
            "{}",
            d.id
        );
        for (name, value) in [
            ("max-locals", d.max_locals),
            ("max-upvalues", d.max_upvalues),
            ("max-constants", d.max_constants),
            ("max-registers", d.max_registers),
        ] {
            let found = facts[name]
                .as_integer()
                .and_then(|n| usize::try_from(n).ok());
            assert_eq!(found, Some(value), "{} {name}", d.id);
        }
        // a call after one local fails at this many arguments
        let observed = facts["limits-observed"].as_str().expect("observed limits");
        let arguments = d.max_registers - 1 - d.call_slots;
        assert!(
            observed.contains(&format!("a call of {arguments} arguments")),
            "{}: {observed}",
            d.id
        );
        assert_eq!(
            observed.contains("of a kind"),
            d.constants_by_kind,
            "{}",
            d.id
        );
        let zero = facts["negative-zero-literal"].as_str().expect("-0.0");
        assert_eq!(
            zero.contains("stays -0"),
            d.negative_zero,
            "{}: {zero}",
            d.id
        );
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

pub(crate) fn span(start: u32, end: u32) -> Span {
    Span::new(FileId::new(0), start, end)
}

pub(crate) fn expr(kind: ExprKind, start: u32, end: u32) -> Expr {
    Expr::new(kind, span(start, end))
}

/// `let name () = body`.
pub(crate) fn function(name: &str, body: Expr) -> Item {
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

pub(crate) fn request(platform: &str, tags: &[&str], items: Vec<Item>) -> BuildRequest {
    let module = module("App", items);
    let program = Program::new("test", vec![module], vec![], &|_| "App.fl".into());
    let own = vec![ModuleInput {
        name: "App".into(),
        role: ModuleRole::Own,
        package: None,
        output: "App.lua".into(),
        interface: None,
    }];
    build_request(platform, tags, &program, own)
}

/// A module of `items`.
pub(crate) fn module(name: &str, items: Vec<Item>) -> Module {
    Module {
        name: name.into(),
        items,
        types: vec![],
        preambles: vec![],
        labels: BTreeMap::new(),
        initialization: Initialization::default(),
        records: BTreeMap::new(),
        package: None,
        exports: Exports::default(),
    }
}

/// The request to build `program`, of `modules`, whose entry is `App.main`.
pub(crate) fn build_request(
    platform: &str,
    tags: &[&str],
    program: &Program,
    modules: Vec<ModuleInput>,
) -> BuildRequest {
    BuildRequest {
        target: TargetRequest {
            backend_api: "0.1.0".into(),
            ir_version: flick_ir::VERSION.into(),
            language_version: "0.2.0".into(),
            runtime_abis: vec![
                Abi {
                    name: "flickrt".into(),
                    version: "0.2.0".into(),
                },
                Abi {
                    name: "flick.kernel".into(),
                    version: "0.1.0".into(),
                },
            ],
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
        modules,
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
        exports: vec![],
        headers: None,
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
        Ok(_) => vec![],
        Err(found) => found
            .into_iter()
            .map(|d| (d.message, d.span.map(|s| (s.start, s.end))))
            .collect(),
    }
}

/// The Lua kernel holds `BigInt`, `Int64` and `UInt64` exactly on every
/// profile and dialect, without the `ffi` capability.
#[test]
fn a_wide_integer_is_the_kernels_on_every_profile() {
    let literal = || expr(ExprKind::Lit(Lit::Int64(1)), 3, 5);
    for platform in ["love", "wow-retail"] {
        let big = request(
            platform,
            &["lua", "lua51"],
            vec![function("main", big_product())],
        );
        assert_eq!(messages(&big), vec![], "{platform}");
        let built = Lua.generate(&big, &mut NoInputs).expect("generated");
        let app = (built.artifacts.iter())
            .find(|a| a.path == "App.lua")
            .expect("App.lua");
        let text = String::from_utf8(app.bytes.clone()).expect("UTF-8");
        assert!(
            text.contains("K.big_mul(K.big(1, 2), K.big(1, 2))"),
            "{text}"
        );
        let wide = request(
            platform,
            &["lua", "lua51"],
            vec![function("main", literal())],
        );
        assert_eq!(messages(&wide), vec![], "{platform}");
    }
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

fn foreign_kernel(name: &str, from: &str, export: &str) -> Item {
    let mut item = foreign_global(name, "unused");
    if let ItemKind::Foreign(foreign) = &mut item.kind {
        foreign.binding = Binding::Module {
            from: from.into(),
            name: export.into(),
        };
    }
    item
}

#[test]
fn whether_a_kernel_binding_suspends_is_the_kernel_tables() {
    let both = expr(
        ExprKind::Tuple(vec![global("pause", 8, 13), global("now", 20, 23)]),
        8,
        23,
    );
    let program = |platform| {
        request(
            platform,
            &["lua", "lua51"],
            vec![
                function("main", both.clone()),
                foreign_kernel("pause", "flick:Effect", "sleep"),
                foreign_kernel("now", "flick:Clock", "now"),
            ],
        )
    };
    // a binding that suspends cannot run on WoW; one that returns is the kernel's
    let wow = messages(&program("wow-retail"));
    let suspension = "asynchronous suspension (`App.pause`) is not available";
    let kernel = "the Lua kernel does not implement it";
    assert_eq!(wow.len(), 2, "{wow:?}");
    assert!(wow[0].0.contains(suspension), "{wow:?}");
    assert_eq!(wow[0].1, Some((8, 13)));
    assert!(wow[1].0.contains(kernel), "{wow:?}");
    assert_eq!(wow[1].1, Some((20, 23)));
    // LÖVE can suspend: both are the kernel's
    let love = messages(&program("love"));
    assert_eq!(love.len(), 2, "{love:?}");
    assert!(love.iter().all(|(m, _)| m.contains(kernel)), "{love:?}");
}

#[test]
fn generation_writes_the_chunks_of_a_clean_check() {
    let clean = |platform| {
        request(
            platform,
            &["lua", "lua51"],
            vec![function("main", expr(ExprKind::Lit(Lit::Unit), 0, 2))],
        )
    };
    let built = Lua
        .generate(&clean("love"), &mut NoInputs)
        .expect("generated");
    let paths: Vec<&str> = built.artifacts.iter().map(|a| a.path.as_str()).collect();
    let kernel = kernel_paths();
    let k = kernel.len();
    assert_eq!(paths[..k], kernel);
    assert_eq!(
        paths[k..],
        ["App.lua", "App.lua.map", "chunks.txt", "main.lua"]
    );
    let text = |i: usize| String::from_utf8(built.artifacts[i].bytes.clone()).expect("UTF-8");
    assert_eq!(
        text(k),
        "local _, ns = ...\nlocal K = ns.k\nlocal M = {}\nns.App = M\nM.main = function(_)\n  \
         return true\nend\n"
    );
    let mut main = String::from("local ns = {}\n");
    for p in kernel.iter().map(String::as_str).chain(["App.lua"]) {
        main.push_str("assert(love.filesystem.load(\"");
        main.push_str(p);
        main.push_str("\"))(nil, ns)\n");
    }
    assert_eq!(text(k + 3), main);
    // WoW loads the files its `.toc` lists: no `main.lua`
    let wow = Lua
        .generate(&clean("wow-retail"), &mut NoInputs)
        .expect("generated");
    let paths: Vec<&str> = wow.artifacts.iter().map(|a| a.path.as_str()).collect();
    assert_eq!(paths[k..], ["App.lua", "App.lua.map", "chunks.txt"]);
    let again = Lua
        .generate(&clean("wow-retail"), &mut NoInputs)
        .expect("generated");
    assert_eq!(
        wow.artifacts, again.artifacts,
        "a rebuild writes the same bytes"
    );
}

#[test]
fn malformed_ir_is_refused_and_generates_nothing() {
    let mut bad = request(
        "love",
        &["lua", "lua51"],
        vec![function("main", expr(ExprKind::Lit(Lit::Unit), 0, 2))],
    );
    bad.program = b"not the CBOR of a program".to_vec();
    let refused = Lua.generate(&bad, &mut NoInputs).expect_err("refused");
    assert_eq!(refused.len(), 1, "{refused:?}");
    assert_eq!(refused[0].code, "P012");
}

pub(crate) struct NoInputs;
impl Inputs for NoInputs {
    fn read(&mut self, _: &str) -> Result<Vec<u8>, InputError> {
        Err(InputError::Undeclared)
    }
}

mod launch;
