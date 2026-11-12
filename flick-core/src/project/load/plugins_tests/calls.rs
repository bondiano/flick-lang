use super::{blocks, check, host, library, roots, show};
use crate::source::SourceMap;

#[test]
fn watched_calls_receive_the_state_of_both_blocks_once() {
    let main = format!(
        "{}\ncounted : Float\nlet counted = Pairs.tally 0.0\n",
        blocks(&[("x", "1.0"), ("y", "2.0")])
    );
    let host = host(&main);
    host.write(
        "/pairs/flick.toml",
        "schema-version = 1\n[project]\nkind = \"lib\"\n[package]\nname = \"pairs\"\nversion = \"1.0.0\"\nplugin = \"pairs.wasm\"\nwatch = [\"Pairs.tally\"]\n",
    );
    host.write(
        "/pairs/src/Pairs.fl",
        "module Pairs exposing (tally)\ntally : Float -> Float\nlet tally n = n\n",
    );
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let diagnostics = check(&host, &mut sources, Some(&project), &roots());
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].code.to_string(), "pairs/X005");
    assert_eq!(host.calls(), (1, 2));
    let states = host.plugin.call_states.lock().expect("call states");
    assert_eq!(states.len(), 1, "one dispatch, not one per block");
    assert_eq!(&states[0][..4], &2u32.to_le_bytes());
}

/// A plugin is told where a value it watches is named, applied or not: the
/// declaration around it, whether inside a function, and the arguments given.
#[test]
fn a_watched_name_comes_with_the_declaration_it_stands_in() {
    let main = "module App.Main exposing (..)\n\nimport Pairs\n\none : Float\nlet one = Pairs.keyed 1.0 2.0\n\nlater : Float -> Float\nlet later n = Pairs.keyed n 2.0\n\nbare : Float -> Float -> Float\nlet bare = Pairs.keyed\n";
    let host = host(main);
    host.write(
        "/pairs/flick.toml",
        "schema-version = 1\n[project]\nkind = \"lib\"\n[package]\nname = \"pairs\"\nversion = \"1.0.0\"\nplugin = \"pairs.wasm\"\nwatch = [\"Pairs.keyed\"]\n",
    );
    host.write(
        "/pairs/src/Pairs.fl",
        "module Pairs exposing (keyed)\nkeyed : Float -> Float -> Float\nlet keyed a b = a + b\n",
    );
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let found = check(&host, &mut sources, Some(&project), &roots());
    let found: Vec<String> = found.iter().map(|d| show(&sources, d)).collect();
    assert_eq!(
        found,
        [
            "pairs/X008 `Pairs.keyed`: `keyed` in `one`, given 2",
            // a function is no binding of one value: it has no name to key by
            "pairs/X008 `Pairs.keyed`: `keyed` in `_`, given 2, in a function",
            "pairs/X008 `Pairs.keyed`: `keyed` in `bare`, given 0",
        ]
    );
}
