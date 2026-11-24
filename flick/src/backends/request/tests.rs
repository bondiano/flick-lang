use std::{env, fs, process};

use flick_backend::{
    Abi, Backend, BackendId, BackendPin, DeclaredInputs, Descriptor, Environment,
    EnvironmentServices, Limits, Mode, ModuleRole, NamedRange, Profile, ValueAbi, VersionRange,
    sha256, validate_request,
};
use flick_backend_js::Javascript;
use flick_core::build::{self, Extras};
use flick_core::diagnostics::Severity;
use flick_core::iface::{self, Target};
use flick_core::project;
use flick_core::source::SourceMap;
use std::path::{Path, PathBuf};

use super::Prepared;
use crate::backends::Selected;
use crate::backends::options::ResolvedOptions;
use crate::commands::build::{BuildPlan, Purpose};
use crate::sources::FsHost;

fn go_test_descriptor() -> Descriptor {
    let range = || VersionRange {
        minimum: "0.1.0".into(),
        maximum_exclusive: "0.2.0".into(),
    };
    Descriptor {
        identity: BackendId {
            id: "test/go".into(),
            version: "1.0.0".into(),
        },
        backend_api: range(),
        ir: range(),
        language: range(),
        runtime_abis: vec![NamedRange {
            name: "flickrt".into(),
            versions: range(),
        }],
        environments: vec![Environment {
            backend: "go".into(),
            platform: "native".into(),
        }],
        tags: vec!["go".into()],
        axes: vec![],
        profiles: vec![Profile::Debug, Profile::Release],
        modes: vec![Mode::WholeProgram],
        capabilities: vec![],
        options: vec![],
        launch_runtimes: vec![],
        host_tools: vec!["go".into()],
        projections: vec![Abi {
            name: "flick.go".into(),
            version: "0.1.0".into(),
        }],
        stages_by_class_option: None,
        foreign_extension: Some("go".into()),
        foreign_suffixes: vec![".go".into()],
        handled_services: vec![EnvironmentServices {
            environment: Environment {
                backend: "go".into(),
                platform: "native".into(),
            },
            services: vec!["Database".into(), "Db".into(), "Server".into()],
        }],
        foreign_adapters: vec![],
        type_form_attributes: vec![],
        value_abi: None,
    }
}

#[test]
fn go_dependency_fingerprints_describe_the_published_go_interfaces() {
    let root = env::temp_dir().join(format!("flick-go-request-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("directory");
    fs::write(
        root.join("Helper.fl"),
        "module Helper exposing (n)\nlet n = 1\n",
    )
    .expect("helper");
    fs::write(
        root.join("Main.fl"),
        "module Main exposing (answer)\nimport Helper\nlet answer = Helper.n\n",
    )
    .expect("main");
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let (diagnostics, compilation) =
        project::compile(&host, &mut sources, None, &[root.join("Main.fl")]);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let pin = BackendPin {
        identity: BackendId {
            id: "test/go".into(),
            version: "1.0.0".into(),
        },
        sha256: sha256(b"component"),
    };
    let output = BuildPlan {
        purpose: Purpose::Build,
        backend: Selected::Native,
        target: "go".into(),
        target_overridden: false,
        options: ResolvedOptions::default(),
        inspect_module: None,
        out_dir: root.join("output"),
        entries: vec![],
        typescript: false,
        stages_by_class: false,
        suites: vec![],
        extras: Extras::default(),
        release: false,
    };
    let go_desc = go_test_descriptor();
    let js_desc = Javascript.describe();
    let prepared = Prepared::new(
        &host,
        &mut sources,
        None,
        &compilation,
        &output,
        pin,
        &go_desc,
    );
    assert_interfaces(&prepared, Target::Go);
    for (environment, desc, expected) in [
        ("js/node", &js_desc, Target::Js),
        ("go/native", &go_desc, Target::Go),
    ] {
        let config_file = sources.add("config", "");
        let mut config = project::config::parse(
            "schema-version = 1\n[project]\nkind = \"lib\"\n",
            config_file,
        )
        .expect("config");
        config.build.environment = Some(environment.parse().expect("environment"));
        let project = project::Project {
            root: root.clone(),
            config,
            config_file,
            workspace: None,
        };
        let prepared = Prepared::new(
            &host,
            &mut sources,
            Some(&project),
            &compilation,
            &output,
            prepared.request.backend.clone(),
            desc,
        );
        assert_interfaces(&prepared, expected);
        assert_root_handlers(environment, &prepared);
    }
    fs::remove_dir_all(root).expect("cleanup");
}

fn assert_root_handlers(environment: &str, prepared: &Prepared) {
    if environment == "js/node" {
        assert!(
            prepared
                .request
                .root_handlers
                .iter()
                .any(|h| h.service.module == "Database" && h.handler.name == "pg"),
            "{:?}",
            prepared.request.root_handlers
        );
    }
    if environment == "go/native" {
        assert!(
            prepared
                .request
                .root_handlers
                .iter()
                .any(|h| h.service.name == "Server"),
            "{:?}",
            prepared.request.root_handlers
        );
    }
}

fn assert_interfaces(prepared: &Prepared, expected: Target) {
    let interface = |name: &str| {
        let file = prepared
            .shared
            .iter()
            .find(|artifact| artifact.path == format!("{name}.iface"))
            .expect("published interface");
        iface::read(&file.contents, name).expect("interface")
    };
    let helper = interface("Helper");
    let main = interface("Main");
    assert_eq!(helper.targets, [expected]);
    assert_eq!(main.targets, [expected]);
    assert_eq!(
        main.dependencies.get("Helper"),
        Some(&iface::fingerprint(&helper).expect("publishable fingerprint"))
    );
}

#[test]
fn a_backend_receives_a_closed_graph_with_real_source_provenance() {
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run/codegen/gadt_rows.fl");
    let (diagnostics, compilation) = project::compile(&host, &mut sources, None, &[root]);
    assert!(
        !diagnostics.iter().any(|d| d.severity == Severity::Error),
        "{diagnostics:?}"
    );
    let output = BuildPlan {
        purpose: Purpose::Build,
        backend: Selected::Native,
        target: "js".into(),
        target_overridden: false,
        options: ResolvedOptions::js(false, false, false, false, false),
        inspect_module: None,
        out_dir: "unused-provenance-output".into(),
        entries: vec![("main".into(), "Main".into())],
        typescript: false,
        stages_by_class: false,
        suites: vec![],
        extras: build::Extras::default(),
        release: false,
    };
    let js_desc = Javascript.describe();
    let prepared = Prepared::new(
        &host,
        &mut sources,
        None,
        &compilation,
        &output,
        BackendPin {
            identity: BackendId {
                id: "test/provenance".into(),
                version: "1.0.0".into(),
            },
            sha256: flick_backend::sha256(b"test component"),
        },
        &js_desc,
    );
    let mut inputs = DeclaredInputs::new(&prepared.request.inputs, |id| {
        prepared.bytes.get(id).cloned()
    })
    .expect("all side inputs have their declared bytes");
    let program = validate_request(&prepared.request, &mut inputs, Limits::default())
        .expect("the combined IR, source table and module metadata agree");
    assert!(program.modules.len() > 100);
    let count = sources.files().count();
    let again = Prepared::new(
        &host,
        &mut sources,
        None,
        &compilation,
        &output,
        prepared.request.backend.clone(),
        &js_desc,
    );
    assert_eq!(
        sources.files().count(),
        count,
        "preparing again reuses source identities"
    );
    assert_eq!(again.request.program, prepared.request.program);
    assert_eq!(again.request.sources, prepared.request.sources);
    assert_eq!(again.bytes, prepared.bytes);
    for module in &program.modules {
        let role = prepared
            .request
            .modules
            .iter()
            .find(|m| m.name == module.name)
            .expect("metadata")
            .role;
        module.clone().each_span_mut(&mut |span| {
            let index = span.file.index() as usize;
            let source = &prepared.request.sources[index];
            let actual = sources.get(prepared.files[index]);
            assert_eq!(source.name, actual.path);
            assert_eq!(prepared.bytes[&source.input], actual.text.as_bytes());
            assert!(actual.text.get(span.range()).is_some());
            if role == ModuleRole::Builtin {
                assert_eq!(source.name, format!("<builtin>/{}.fl", module.name));
            } else {
                assert_eq!(role, ModuleRole::Own);
                assert!(!source.name.starts_with("<builtin>"));
            }
        });
    }
}

fn foreign_fixture_root(tag: &str) -> (PathBuf, project::Compilation, SourceMap) {
    let root = env::temp_dir().join(format!("flick-abi-request-{tag}-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("directory");
    fs::write(
        root.join("native.mjs"),
        "export const add = (a, b) => a + b;\n",
    )
    .expect("native.mjs");
    fs::write(
        root.join("native.go"),
        "package main\nimport \"flick.app/flickrt\"\nfunc add(fb *flickrt.Fiber, a, b flickrt.Value) flickrt.Value { return a }\n",
    )
    .expect("native.go");
    fs::write(
        root.join("Main.fl"),
        "module Main exposing (add, main)\n\nadd : Int -> Int -> Int\nforeign pure add from \"./native.mjs\" \"add\"\n  arity 2\n\nmain : Unit -> Unit\nlet main () = ()\n",
    )
    .expect("Main.fl");
    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let (diagnostics, compilation) =
        project::compile(&host, &mut sources, None, &[root.join("Main.fl")]);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    (root, compilation, sources)
}

fn plan_for(target: &str, options: ResolvedOptions) -> BuildPlan {
    BuildPlan {
        purpose: Purpose::Build,
        backend: Selected::Native,
        target: target.into(),
        target_overridden: false,
        options,
        inspect_module: None,
        out_dir: "unused-abi-out".into(),
        entries: vec![("main".into(), "Main".into())],
        typescript: false,
        stages_by_class: false,
        suites: vec![],
        extras: Extras::default(),
        release: false,
    }
}

fn sample_custom_value_abi(name: &str, version: &str) -> ValueAbi {
    ValueAbi {
        abi: Abi {
            name: name.into(),
            version: version.into(),
        },
        opaque_type: "void *".into(),
        catch_error_type: "const char *".into(),
        catch_lang_item: Some("string".into()),
        entries: vec![],
        unsupported: vec![],
        fallback_reason: "outside table".into(),
        admits_copied_data: false,
    }
}

fn native_binding_abi(prepared: &Prepared) -> Abi {
    prepared
        .request
        .foreign
        .iter()
        .find(|b| b.module == "Main" && b.specifier == "./native.mjs")
        .expect("request contains ./native.mjs binding")
        .abi
        .clone()
}

fn assert_backend_rejects_abi_mismatch(
    backend: &dyn Backend,
    mut prepared: Prepared,
    expected_abi: &str,
    mismatched_abi: &str,
) {
    for binding in &mut prepared.request.foreign {
        if binding.module == "Main" {
            binding.abi.version = "9.9.9".into();
        }
    }
    let mut inputs = DeclaredInputs::new(&prepared.request.inputs, |id| {
        prepared.bytes.get(id).cloned()
    })
    .expect("inputs");
    for (stage, err) in [
        (
            "check_target",
            backend
                .check_target(&prepared.request, &mut inputs)
                .expect_err("check_target must reject mismatched foreign-binding.abi"),
        ),
        (
            "generate",
            backend
                .generate(&prepared.request, &mut inputs)
                .expect_err("generate must reject mismatched foreign-binding.abi"),
        ),
    ] {
        assert!(
            err.iter().any(|d| d.code == "F014"
                && d.message.contains(expected_abi)
                && d.message.contains(mismatched_abi)),
            "expected F014 from {stage}, got {err:?}"
        );
    }
}

#[test]
fn prepared_populates_foreign_binding_abi_for_js_go_and_generic_backends() {
    let (root, compilation, mut sources) = foreign_fixture_root("populate");
    let host = FsHost::new();
    let pin = |id: &str| BackendPin {
        identity: BackendId {
            id: id.into(),
            version: "0.1.0".into(),
        },
        sha256: sha256(b"component"),
    };

    let js_desc = Javascript.describe();
    let js_prepared = Prepared::new(
        &host,
        &mut sources,
        None,
        &compilation,
        &plan_for("js", ResolvedOptions::js(false, false, false, false, false)),
        pin("flick/js"),
        &js_desc,
    );
    assert_eq!(
        native_binding_abi(&js_prepared),
        js_desc.value_abi.as_ref().expect("js value_abi").abi
    );

    let go_default = go_test_descriptor();
    let go_prepared = Prepared::new(
        &host,
        &mut sources,
        None,
        &compilation,
        &plan_for("go", ResolvedOptions::default()),
        pin("flick/go"),
        &go_default,
    );
    assert_eq!(
        native_binding_abi(&go_prepared),
        Abi {
            name: "flick.go-ffi".into(),
            version: "0.1.0".into(),
        }
    );

    let mut go_custom = go_default;
    go_custom.value_abi = Some(sample_custom_value_abi("custom.go-ffi", "0.2.0"));
    let go_custom_prepared = Prepared::new(
        &host,
        &mut sources,
        None,
        &compilation,
        &plan_for("go", ResolvedOptions::default()),
        pin("flick/go"),
        &go_custom,
    );
    assert_eq!(
        native_binding_abi(&go_custom_prepared),
        Abi {
            name: "custom.go-ffi".into(),
            version: "0.2.0".into(),
        }
    );

    let mut generic_desc = go_test_descriptor();
    generic_desc.projections.clear();
    generic_desc.value_abi = Some(sample_custom_value_abi("test.literal-ffi", "0.1.0"));
    let generic_prepared = Prepared::new(
        &host,
        &mut sources,
        None,
        &compilation,
        &plan_for("literal", ResolvedOptions::default()),
        pin("test/literal"),
        &generic_desc,
    );
    assert_eq!(
        native_binding_abi(&generic_prepared),
        Abi {
            name: "test.literal-ffi".into(),
            version: "0.1.0".into(),
        }
    );

    fs::remove_dir_all(root).expect("cleanup");
}

#[test]
fn js_and_go_backends_reject_mismatched_foreign_binding_abi_with_f014() {
    let (root, compilation, mut sources) = foreign_fixture_root("mismatch");
    let host = FsHost::new();

    let js_desc = Javascript.describe();
    let js_prepared = Prepared::new(
        &host,
        &mut sources,
        None,
        &compilation,
        &plan_for("js", ResolvedOptions::js(false, false, false, false, false)),
        BackendPin {
            identity: js_desc.identity.clone(),
            sha256: sha256(b"js"),
        },
        &js_desc,
    );
    assert_backend_rejects_abi_mismatch(
        &Javascript,
        js_prepared,
        "flick.javascript-ffi@0.1.0",
        "flick.javascript-ffi@9.9.9",
    );

    #[cfg(feature = "wasm-backends")]
    {
        use flick_backend::host::{Budget, WasmBackend};

        use crate::component;

        let go_pkg = Path::new(env!("CARGO_MANIFEST_DIR")).join("../backends/go/package");
        let go_pin: BackendPin =
            serde_json::from_slice(&fs::read(go_pkg.join("backend.json")).expect("go pin"))
                .expect("go pin json");
        let go_bytes = fs::read(go_pkg.join("component.wasm")).expect("go wasm");
        let engine = component::engine().expect("engine");
        let cancel = component::cancellation().expect("cancellation").clone();
        let go_backend = WasmBackend::load_with_cancellation(
            engine,
            go_pin.clone(),
            &go_bytes,
            Budget::default(),
            cancel,
        )
        .expect("go component loads");
        let go_desc = go_backend.describe();
        let go_prepared = Prepared::new(
            &host,
            &mut sources,
            None,
            &compilation,
            &plan_for("go", ResolvedOptions::default()),
            go_pin,
            &go_desc,
        );
        assert_backend_rejects_abi_mismatch(
            &go_backend,
            go_prepared,
            "flick.go-ffi@0.1.0",
            "flick.go-ffi@9.9.9",
        );
    }

    fs::remove_dir_all(root).expect("cleanup");
}
