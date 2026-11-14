#![cfg(unix)]

use super::*;
use std::os::unix::fs::symlink;

#[test]
fn test_refuses_internal_redirects_before_removing_files() {
    internal_redirects("test");
}

#[test]
fn clean_refuses_internal_redirects_before_removing_files() {
    internal_redirects("clean");
}

fn internal_redirects(command: &str) {
    for redirect in [".flick", ".flick/test", ".flick/run", ".flick/cache"] {
        if command == "test" && !matches!(redirect, ".flick" | ".flick/test") {
            continue;
        }
        let dir = project(
            &format!(
                "cli_internal_redirect_{command}_{}",
                redirect.replace('/', "_")
            ),
            &[
                ("flick.toml", APP),
                ("src/App/Main.fl", MAIN),
                ("outside/test/kept", "owned elsewhere"),
                ("outside/kept", "owned elsewhere"),
                ("outside/1/kept", "owned elsewhere"),
            ],
        );
        let link = dir.join(redirect);
        fs::create_dir_all(link.parent().expect("link parent")).expect("parent");
        symlink(dir.join("outside"), &link).expect("redirect");
        let args: &[&str] = if command == "clean" {
            &["clean", "--cache"]
        } else {
            &["test"]
        };
        let result = flick_in(&dir, args);
        assert_eq!(code(&result), Some(1), "{command} {redirect}: {result:?}");
        assert!(stderr(&result).contains("FLICK-P013"), "{result:?}");
        for file in ["test/kept", "kept", "1/kept"] {
            assert_eq!(
                fs::read(dir.join("outside").join(file)).expect("outside remains"),
                b"owned elsewhere"
            );
        }
    }
}

#[test]
fn a_directory_in_place_of_a_stale_file_preserves_the_previous_publication() {
    use flick_core::project::cache::{MANIFEST, Manifest};

    for command in ["build", "clean"] {
        let dir = project(
            &format!("cli_stale_directory_{command}"),
            &[
                ("flick.toml", LIB),
                (
                    "src/Main.fl",
                    "module Main exposing (answer)\nlet answer = 1\n",
                ),
                ("dist/old.js", "previous build"),
                ("dist/stale.js/kept", "owned elsewhere"),
            ],
        );
        let mut manifest = Manifest::default();
        manifest.record("dist", vec!["old.js".into(), "stale.js".into()]);
        let original = serde_json::to_vec(&manifest).expect("manifest JSON");
        let path = dir.join(MANIFEST);
        fs::create_dir_all(path.parent().expect("parent")).expect("manifest directory");
        fs::write(&path, &original).expect("manifest");
        let result = flick_in(&dir, &[command]);
        assert_eq!(code(&result), Some(1), "{command}: {result:?}");
        assert!(stderr(&result).contains("FLICK-P013"), "{result:?}");
        assert_eq!(
            fs::read(dir.join("dist/old.js")).expect("old remains"),
            b"previous build"
        );
        assert_eq!(
            fs::read(dir.join("dist/stale.js/kept")).expect("directory remains"),
            b"owned elsewhere"
        );
        assert_eq!(fs::read(path).expect("manifest remains"), original);
        assert!(!dir.join("dist/Main.js").exists(), "no earlier publication");
    }
}
