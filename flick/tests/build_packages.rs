mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::flick_in;
use flick_core::iface;

fn built_package_consumer(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    let lib = dir.join("lib");
    fs::create_dir_all(lib.join("src/Text")).expect("dir");
    fs::write(
        lib.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
    )
    .expect("write");
    fs::write(lib.join("package.json"), "{ \"name\": \"@acme/text\" }\n").expect("write");
    fs::write(
        lib.join("src/Text/Core.fl"),
        "module Text.Core exposing (suffix)\nsuffix : String\nlet suffix = \"!\"\n",
    )
    .expect("write");
    fs::write(
        lib.join("src/Text/Shout.fl"),
        "module Text.Shout exposing (shout)\n\nimport Text.Core exposing (suffix)\n\nshout : String -> String\nlet shout s = s ++ suffix\n",
    )
    .expect("write");
    fs::write(
        lib.join("src/Text/Api.fl"),
        "module Text.Api exposing (shout)\nimport Text.Shout exposing (shout)\n",
    )
    .expect("write");
    let out = flick_in(&lib, &["build"]);
    assert!(out.status.success(), "{out:?}");
    // the library as installed: its build output is its `iface-dir`
    let installed = dir.join("app/node_modules/@acme/text");
    fs::create_dir_all(installed.join("dist/Text")).expect("dir");
    for module in ["Api", "Core", "Shout"] {
        for ext in ["js", "iface"] {
            let file = format!("{module}.{ext}");
            fs::copy(
                lib.join("dist/Text").join(&file),
                installed.join("dist/Text").join(file),
            )
            .expect("copy");
        }
    }
    fs::write(
        installed.join("package.json"),
        "{ \"name\": \"@acme/text\", \"type\": \"module\", \"flick\": { \"iface-dir\": \"./dist/\" } }\n",
    )
    .expect("write");
    let app = dir.join("app");
    fs::create_dir_all(app.join("src/App")).expect("dir");
    fs::write(
        app.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
    )
    .expect("write");
    fs::write(
        app.join("package.json"),
        "{ \"name\": \"@acme/app\", \"dependencies\": { \"@acme/text\": \"1.0.0\" } }\n",
    )
    .expect("write");
    fs::write(
        app.join("src/App/Main.fl"),
        "module App.Main exposing (greeting)\n\nimport Text.Api exposing (shout)\n\ngreeting : String\nlet greeting = shout \"hi\"\n",
    )
    .expect("write");
    let out = flick_in(&app, &["build"]);
    assert!(out.status.success(), "{out:?}");
    dir
}

#[test]
fn installed_package_graph_uses_published_fingerprints_and_reexport_origins() {
    let dir = built_package_consumer("build_package_graph");
    for (module, dependency) in [("Text.Api", "Text.Shout"), ("Text.Shout", "Text.Core")] {
        let published = |name: &str| {
            iface::read(
                &fs::read(
                    dir.join("lib/dist")
                        .join(format!("{}.iface", name.replace('.', "/"))),
                )
                .expect("interface"),
                name,
            )
            .expect("valid interface")
        };
        assert_eq!(
            published(module).dependencies[dependency],
            iface::fingerprint(&published(dependency)),
        );
    }
    let app = dir.join("app");
    // Older interfaces may record only the export origin, not its dependency hash.
    let api_path = app.join("node_modules/@acme/text/dist/Text/Api.iface");
    let mut api = iface::read(&fs::read(&api_path).expect("interface"), "Text.Api").expect("valid");
    api.dependencies.clear();
    fs::write(&api_path, iface::write(&api)).expect("write");
    let origins = flick_in(&app, &["build", "--out-dir", "origins"]);
    assert!(origins.status.success(), "{origins:?}");
    let core_path = app.join("node_modules/@acme/text/dist/Text/Core.iface");
    let mut core =
        iface::read(&fs::read(&core_path).expect("interface"), "Text.Core").expect("valid");
    let summary = &mut core.abi.as_mut().expect("ABI").values[0];
    summary.partial = !summary.partial;
    fs::write(&core_path, iface::write(&core)).expect("write");
    let mismatch = flick_in(&app, &["build", "--out-dir", "incompatible"]);
    assert_eq!(mismatch.status.code(), Some(1), "{mismatch:?}");
    let stderr = String::from_utf8_lossy(&mismatch.stderr);
    assert!(
        stderr.contains("FLICK-P014") && stderr.contains("Text.Core"),
        "{stderr}"
    );
    fs::remove_file(core_path).expect("remove interface");
    let missing = flick_in(&app, &["build", "--out-dir", "missing"]);
    assert_eq!(missing.status.code(), Some(1), "{missing:?}");
    let stderr = String::from_utf8_lossy(&missing.stderr);
    assert!(
        stderr.contains("FLICK-P014") && stderr.contains("Text.Core"),
        "{stderr}"
    );
    api.dependencies
        .insert("../Escaped".into(), "invalid".into());
    fs::write(api_path, iface::write(&api)).expect("write");
    let invalid = flick_in(&app, &["build", "--out-dir", "invalid"]);
    assert_eq!(invalid.status.code(), Some(2), "{invalid:?}");
    let stderr = String::from_utf8_lossy(&invalid.stderr);
    assert!(
        stderr.contains("FLICK-P010") && stderr.contains("../Escaped"),
        "{stderr}"
    );
}

#[test]
#[ignore = "needs Node"]
fn an_external_node_consumer_runs_an_installed_library_reexport() {
    let dir = built_package_consumer("build_package_node");
    let app = dir.join("app");
    let api_path = app.join("node_modules/@acme/text/dist/Text/Api.iface");
    let mut api = iface::read(&fs::read(&api_path).expect("interface"), "Text.Api").expect("valid");
    api.dependencies.clear();
    fs::write(api_path, iface::write(&api)).expect("write");
    let origins = flick_in(&app, &["build", "--out-dir", "origins"]);
    assert!(origins.status.success(), "{origins:?}");
    for output in ["dist", "origins"] {
        let node = Command::new("node")
            .args(["--input-type=module", "--eval"])
            .arg(format!(
                "const {{ greeting }} = await import('./{output}/App/Main.js'); console.log(greeting);"
            ))
            .current_dir(&app)
            .output()
            .expect("node");
        assert!(node.status.success(), "{node:?}");
        assert_eq!(String::from_utf8_lossy(&node.stdout), "hi!\n");
        assert!(node.stderr.is_empty(), "{node:?}");
    }
}
