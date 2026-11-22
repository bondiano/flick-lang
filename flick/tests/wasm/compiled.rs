use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use super::app;
use crate::MAIN;

fn flick_with_default_cache(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(root.join("app"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("FLICK_PLUGIN_TRACE", root.join("trace"))
        .env("NO_COLOR", "1")
        .env_remove("FLICK_WASMTIME_CACHE_DIR")
        .env_remove("CI")
        .output()
        .expect("flick runs")
}

/// What compiling the component gave is kept in the cache directory:
/// the next process, which the plugin runs in again, reads it.
#[test]
fn a_compiled_component_is_kept_for_the_next_process() {
    let root = app("plugins-wasm-compiled", MAIN);
    let checked = flick_with_default_cache(&root, &["check"]);
    assert!(checked.status.success(), "{checked:?}");
    let kept = root.join("cache/flick/wasmtime");
    let entries = fs::read_dir(&kept).map_or(0, Iterator::count);
    assert!(entries > 0, "nothing in {}", kept.display());
    // a changed block calls the plugin again, from the kept code
    let main = MAIN.replace("x = 1.0", "x = 2.0");
    fs::write(root.join("app/src/App/Main.fl"), main).expect("write");
    let checked = flick_with_default_cache(&root, &["check"]);
    assert!(checked.status.success(), "{checked:?}");
}
