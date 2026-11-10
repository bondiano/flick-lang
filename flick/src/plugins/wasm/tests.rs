use std::sync::Arc;
use std::{env, fs, process};

use super::load;

/// What `load` reads of a component before any call compiles it.
fn component(rest: &str) -> String {
    format!("flick:plugin/compiler@{} {rest}", flick_plugin::VERSION)
}

#[test]
fn a_plugin_is_loaded_once_while_its_file_stays() {
    let dir = env::temp_dir().join(format!("flick-loaded-plugin-{}", process::id()));
    fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("pairs.wasm");
    fs::write(&path, component("")).expect("write");
    let first = load("pairs", &path).expect("a component of this version");
    let again = load("pairs", &path).expect("the same");
    assert!(Arc::ptr_eq(&first, &again), "kept with what it compiled");
    let other = load("others", &path).expect("another package");
    assert!(!Arc::ptr_eq(&first, &other), "its id names its package");
    // a rebuilt component is read again
    fs::write(&path, component("rebuilt")).expect("write");
    let rebuilt = load("pairs", &path).expect("the rebuilt one");
    assert!(!Arc::ptr_eq(&first, &rebuilt));
    let _ = fs::remove_dir_all(&dir);
}
