//! What startup builds after reading the built-in interfaces from their
//! cache, and when it builds it at all.

use std::path::Path;
use std::process::Command;

use crate::common::project;

/// What startup builds after reading the cache is timed when a module is
/// checked, and not built when the store gives every module.
#[test]
fn timings_show_the_built_in_environment_only_when_a_module_is_checked() {
    let app = project(
        "builtins_cache_environment",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
            ),
            (
                "src/App/Main.fl",
                "module App.Main exposing (x)\nx : Int\nlet x = 1\n",
            ),
        ],
    );
    let cache = app.join("cache");
    let std = Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages/std");
    let phases = || {
        let out = Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(["check", "--timings"])
            .current_dir(&app)
            .env("FLICK_STD_PATH", &std)
            .env("XDG_CACHE_HOME", &cache)
            .output()
            .expect("flick runs");
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stderr).expect("utf-8 stderr")
    };
    let first = phases();
    for phase in ["built-in environment learned", "lang items found"] {
        assert!(first.contains(phase), "{phase}: {first}");
    }
    let stored = phases();
    assert!(!stored.contains("built-in environment learned"), "{stored}");
}
