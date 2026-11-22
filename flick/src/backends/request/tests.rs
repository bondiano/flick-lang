use std::{env, fs, process};

use flick_backend::{
    Abi, Backend, BackendId, BackendPin, DeclaredInputs, Descriptor, Environment,
    EnvironmentServices, Limits, Mode, ModuleRole, NamedRange, Profile, VersionRange, sha256,
    validate_request,
};
use flick_backend_js::Javascript;
use flick_core::build::{self, Extras};
use flick_core::diagnostics::Severity;
use flick_core::iface::{self, Target};
use flick_core::project;
use flick_core::source::SourceMap;
use std::path::Path;

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
        Some(&iface::fingerprint(&helper))
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
