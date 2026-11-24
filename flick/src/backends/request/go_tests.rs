//! Unit tests for Go backend `BuildRequest` preparation: direct Go package
//! foreign bindings, `[target.go.dependencies]`, and project `go.sum` inputs.

use std::path::{Path, PathBuf};
use std::{env, fs, process};

use flick_backend::{
    Abi, BackendId, BackendPin, DeclaredInputs, Descriptor, Environment, InputKind, Limits, Mode,
    NamedRange, Profile, ValueAbi, VersionRange, sha256, validate_request,
};
use flick_core::build::Extras;
use flick_core::project;
use flick_core::source::SourceMap;

use super::Prepared;
use crate::backends::Selected;
use crate::backends::options::ResolvedOptions;
use crate::commands::build::{BuildPlan, Purpose};
use crate::sources::FsHost;

fn temp_dir(name: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("flick-go-req-{name}-{}", process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create dir");
    dir
}

fn go_descriptor() -> Descriptor {
    let range = || VersionRange {
        minimum: "0.1.0".into(),
        maximum_exclusive: "0.2.0".into(),
    };
    Descriptor {
        identity: BackendId {
            id: "flick/go".into(),
            version: "0.1.0".into(),
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
        handled_services: vec![],
        foreign_adapters: vec![],
        type_form_attributes: vec![],
        value_abi: Some(ValueAbi {
            abi: Abi {
                name: "flick.go-ffi".into(),
                version: "0.1.0".into(),
            },
            opaque_type: "flickrt.Value".into(),
            catch_error_type: "error".into(),
            catch_lang_item: Some("string".into()),
            entries: vec![],
            unsupported: vec![],
            fallback_reason: "outside table".into(),
            admits_copied_data: false,
        }),
    }
}

fn go_plan(out_dir: PathBuf) -> BuildPlan {
    BuildPlan {
        purpose: Purpose::Build,
        backend: Selected::Native,
        target: "go".into(),
        target_overridden: false,
        options: ResolvedOptions::default(),
        inspect_module: None,
        out_dir,
        entries: vec![("main".into(), "App.Main".into())],
        typescript: false,
        stages_by_class: false,
        suites: vec![],
        extras: Extras::default(),
        release: false,
    }
}

fn write_go_request_fixture(root: &Path) -> &'static str {
    fs::create_dir_all(root.join("src/App")).expect("src dir");
    let toml_text = "schema-version = 1\n\
         [tools]\n\
         \"flick/go\" = [\"go\"]\n\
         [project]\n\
         kind = \"app\"\n\
         entry = \"App.Main\"\n\
         [build]\n\
         target = \"go\"\n\
         [target.go.dependencies]\n\
         \"example.com/acme/math\" = \"v1.2.3\"\n";
    fs::write(root.join("flick.toml"), toml_text).expect("toml");
    fs::write(
        root.join("go.sum"),
        "example.com/acme/math v1.2.3 h1:abc=\n",
    )
    .expect("go.sum");
    fs::write(
        root.join("src/App/Main.fl"),
        "module App.Main exposing (main)\n\
         hypot : Float -> Float -> Float\n\
         foreign pure hypot from \"math\" \"Hypot\"\n\
         scale : Float -> Float\n\
         foreign pure scale from \"example.com/acme/math\" \"Scale\"\n\
         main : Unit -> Unit\n\
         let main () =\n\
         \x20\x20let _ = scale (hypot 3.0 4.0)\n\
         \x20\x20()\n",
    )
    .expect("main");
    toml_text
}

#[test]
fn prepared_go_attaches_direct_package_bindings_dependencies_and_go_sum() {
    let root = temp_dir("deps_and_sum");
    let toml_text = write_go_request_fixture(&root);

    let host = FsHost::new();
    let mut sources = SourceMap::default();
    let config_file = sources.add(root.join("flick.toml").display().to_string(), toml_text);
    let config = project::config::parse(toml_text, config_file).expect("config parses");
    let project = project::Project {
        root: root.clone(),
        config,
        config_file,
        workspace: None,
    };
    let (diags, compilation) = project::compile(
        &host,
        &mut sources,
        Some(&project),
        &[root.join("src/App/Main.fl")],
    );
    assert!(diags.is_empty(), "{diags:?}");

    let desc = go_descriptor();
    let pin = BackendPin {
        identity: BackendId {
            id: desc.identity.id.clone(),
            version: desc.identity.version.clone(),
        },
        sha256: sha256(b"go-component"),
    };
    let prepared = Prepared::new(
        &host,
        &mut sources,
        Some(&project),
        &compilation,
        &go_plan(root.join("dist")),
        pin,
        &desc,
    );
    assert!(
        prepared.diagnostics.is_empty(),
        "{:?}",
        prepared.diagnostics
    );
    let mut inputs = DeclaredInputs::new(&prepared.request.inputs, |id| {
        prepared.bytes.get(id).cloned()
    })
    .expect("declared inputs");
    validate_request(&prepared.request, &mut inputs, Limits::default())
        .expect("valid BuildRequest");

    let math_binding = prepared
        .request
        .foreign
        .iter()
        .find(|b| b.module == "App.Main" && b.specifier == "math")
        .expect("math direct package binding");
    assert!(math_binding.inputs.is_empty(), "{math_binding:?}");
    assert_eq!(math_binding.abi.name, "flick.go-ffi");
    assert_eq!(math_binding.abi.version, "0.1.0");

    let third_party = prepared
        .request
        .foreign
        .iter()
        .find(|b| b.module == "App.Main" && b.specifier == "example.com/acme/math")
        .expect("third-party direct package binding");
    assert!(third_party.inputs.is_empty(), "{third_party:?}");
    assert_eq!(third_party.abi.name, "flick.go-ffi");

    let deps_input = prepared
        .request
        .inputs
        .iter()
        .find(|i| i.id == "go/dependencies")
        .expect("go/dependencies input");
    assert_eq!(deps_input.kind, InputKind::TargetFacts);
    assert_eq!(
        prepared.bytes["go/dependencies"],
        b"example.com/acme/math v1.2.3\n"
    );

    let sum_input = prepared
        .request
        .inputs
        .iter()
        .find(|i| i.id == "go/go.sum")
        .expect("go/go.sum input");
    assert_eq!(sum_input.kind, InputKind::Lockfile);
    assert_eq!(
        prepared.bytes["go/go.sum"],
        b"example.com/acme/math v1.2.3 h1:abc=\n"
    );

    let _ = fs::remove_dir_all(&root);
}
