use std::{env, fs, process};

use flick_backend::{BackendId, BackendPin, sha256};
use flick_core::build::Extras;
use flick_core::iface::{self, Target};
use flick_core::project;
use flick_core::source::SourceMap;

use super::Prepared;
use crate::backends::Selected;
use crate::commands::build::{BuildPlan, Purpose};
use crate::sources::FsHost;

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
    let mut output = BuildPlan {
        purpose: Purpose::Build,
        backend: Selected::Native,
        out_dir: root.join("output"),
        entries: vec![],
        typescript: false,
        go: true,
        suites: vec![],
        extras: Extras::default(),
        release: false,
    };
    let prepared = Prepared::new(&host, &sources, None, &compilation, &output, pin);
    assert_interfaces(&prepared, Target::Go);
    for (environment, go, expected) in [
        ("js/node", true, Target::Js),
        ("go/native", false, Target::Go),
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
        output.go = go;
        let prepared = Prepared::new(
            &host,
            &sources,
            Some(&project),
            &compilation,
            &output,
            prepared.request.backend.clone(),
        );
        assert_interfaces(&prepared, expected);
    }
    fs::remove_dir_all(root).expect("cleanup");
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
