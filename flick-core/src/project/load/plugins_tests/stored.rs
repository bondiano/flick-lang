use std::sync::{Arc, Mutex};

use super::super::{Mode, ModuleCache, ModuleStore, run};
use super::{Memory, blocks, host, library, roots};
use crate::source::SourceMap;

/// The keys of the modules a clean check of `host` gave a store.
fn saved_by(host: &Memory) -> Vec<String> {
    let saved: Arc<Mutex<Vec<String>>> = Arc::default();
    let keys = Arc::clone(&saved);
    let mut cache = ModuleCache::stored(ModuleStore {
        salt: "one".to_owned(),
        load: Box::new(|_| None),
        save: Box::new(move |key, _| keys.lock().expect("the keys").push(key.to_owned())),
    });
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let roots = roots();
    let output = run(
        host,
        &mut sources,
        Some(&project),
        &roots,
        Mode::Check,
        Some(&mut cache),
        None,
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    saved.lock().expect("the keys").clone()
}

/// What a plugin expands or watches depends on the plugin, which no key of
/// a module covers: such a module is checked each time.
#[test]
fn a_module_a_plugin_expands_or_watches_is_not_saved() {
    let plain = "module App.Main exposing (point)\n\npoint : Float\nlet point = 1.0\n";
    assert_eq!(saved_by(&host(plain)).len(), 1, "a module of no plugin");
    let expanded = host(&blocks(&[("x", "1.0")]));
    assert_eq!(
        saved_by(&expanded),
        Vec::<String>::new(),
        "a module with a block"
    );
    let watched = host(plain);
    watched.write(
        "/pairs/flick.toml",
        "schema-version = 1\n[project]\nkind = \"lib\"\n[package]\nname = \"pairs\"\nversion = \"1.0.0\"\nplugin = \"pairs.wasm\"\nwatch = [\"Pairs.tally\"]\n",
    );
    assert_eq!(
        saved_by(&watched),
        Vec::<String>::new(),
        "a module a package watches"
    );
}
