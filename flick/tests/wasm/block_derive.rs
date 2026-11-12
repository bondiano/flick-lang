//! A class of a type a component's code declares: the component's derive is
//! called for it, and what the type wraps crosses the plugin interface.

use std::fs;

use super::{app, flick};
use crate::PAIRS;

#[test]
fn a_wasm_plugin_derives_for_a_type_its_block_declares() {
    let main = "module App.Main exposing (main, metres)\n\nimport Pairs exposing (plugin wrapped)\n\nwrapped Meters = Float deriving Pairs.Arbitrary\n\nmetres : Float\nlet metres = Meters.value Pairs.arbitrary\n\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line (show metres)\n";
    let root = app("plugins-block-derive", main);
    let class = "\n@derive\nclass Arbitrary a =\n  arbitrary : a\n\ninstance Arbitrary Float =\n  let arbitrary = 0.0\n";
    let pairs = PAIRS.replace("exposing (version, tally, boom)", "exposing (..)") + class;
    fs::write(root.join("lib/src/Pairs.fl"), pairs).expect("write");
    let expanded = flick(&root, &["expand"]);
    let stdout = String::from_utf8_lossy(&expanded.stdout);
    assert!(expanded.status.success(), "{expanded:?}");
    assert!(
        stdout.contains("let arbitrary = (Meters.make Pairs.arbitrary)"),
        "{stdout}"
    );
}
