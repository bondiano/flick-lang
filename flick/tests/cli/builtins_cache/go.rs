use super::*;

/// A project that emits Go is checked as its configuration says, on Flick's
/// own runtime unless it names another: a build reads the built-in modules
/// checked that way from their cache, as a check does, and checks them once.
#[test]
fn a_build_for_go_reads_the_built_in_modules_a_check_saved() {
    let app = project(
        "builtins_cache_go",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[project]\nkind = \"lib\"\n\n[build]\ntarget = \"go\"\n",
            ),
            (
                "src/App/Main.fl",
                "module App.Main exposing (x)\nx : Int\nlet x = 1\n",
            ),
        ],
    );
    let cache = app.join("cache");
    let std = Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages/std");
    // a library has no entry for the go target to build, which the build
    // says after checking: no Go toolchain is asked for
    let checked = || {
        let _ = fs::remove_dir_all(app.join(".flick"));
        let out = Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(["build", "--timings", "--quiet"])
            .current_dir(&app)
            .env("FLICK_STD_PATH", &std)
            .env("XDG_CACHE_HOME", &cache)
            .output()
            .expect("flick runs");
        let stderr = String::from_utf8(out.stderr).expect("utf-8 stderr");
        assert!(stderr.contains("FLICK-P012"), "{stderr}");
        stderr.contains("built-in modules checked")
    };
    assert!(checked(), "the first build checks them");
    assert!(!checked(), "the second reads them");
}
