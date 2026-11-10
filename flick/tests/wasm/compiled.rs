use std::fs;

use super::{app, flick};
use crate::MAIN;

/// What compiling the component gave is kept in the cache directory:
/// the next process, which the plugin runs in again, reads it.
#[test]
fn a_compiled_component_is_kept_for_the_next_process() {
    let root = app("plugins-wasm-compiled", MAIN);
    let checked = flick(&root, &["check"]);
    assert!(checked.status.success(), "{checked:?}");
    let kept = root.join("cache/flick/wasmtime");
    let entries = fs::read_dir(&kept).map_or(0, Iterator::count);
    assert!(entries > 0, "nothing in {}", kept.display());
    // a changed block calls the plugin again, from the kept code
    let main = MAIN.replace("x = 1.0", "x = 2.0");
    fs::write(root.join("app/src/App/Main.fl"), main).expect("write");
    let checked = flick(&root, &["check"]);
    assert!(checked.status.success(), "{checked:?}");
}
