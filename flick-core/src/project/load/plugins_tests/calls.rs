use super::{blocks, check, host, library, roots};
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
