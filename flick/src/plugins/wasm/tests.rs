use std::sync::Arc;
use std::{env, fs, process};

use super::{KEPT, load};

/// What `load` reads of a component before any call compiles it.
fn component(rest: &str) -> String {
    format!("flick:plugin/compiler@{} {rest}", flick_plugin::VERSION)
}

#[test]
fn a_plugin_is_kept_by_the_bytes_of_its_component() {
    let dir = env::temp_dir().join(format!("flick-loaded-plugin-{}", process::id()));
    fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("pairs.wasm");
    fs::write(&path, component("first")).expect("write");
    let first = load("pairs", &path).expect("a component of this version");
    let again = load("pairs", &path).expect("the same");
    assert!(Arc::ptr_eq(&first, &again), "kept with what it compiled");
    let other = load("others", &path).expect("another package");
    assert!(!Arc::ptr_eq(&first, &other), "its id names its package");
    // other bytes of the same length, whatever the time of the file says
    fs::write(&path, component("other")).expect("write");
    let rebuilt = load("pairs", &path).expect("the rebuilt one");
    assert!(!Arc::ptr_eq(&first, &rebuilt));
    // the same bytes elsewhere are the same plugin
    let moved = dir.join("moved.wasm");
    fs::write(&moved, component("first")).expect("write");
    let elsewhere = load("pairs", &moved).expect("the first again");
    assert!(Arc::ptr_eq(&first, &elsewhere));
    // only the latest are kept: the first goes after enough others
    for n in 0..KEPT {
        fs::write(&path, component(&format!("filler {n}"))).expect("write");
        load("pairs", &path).expect("a filler");
    }
    fs::write(&path, component("first")).expect("write");
    let reloaded = load("pairs", &path).expect("the first, read anew");
    assert!(!Arc::ptr_eq(&first, &reloaded));
    let _ = fs::remove_dir_all(&dir);
}
