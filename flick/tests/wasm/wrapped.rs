//! A type a component declares over another: the mark of its
//! representation crosses the plugin interface with the declaration.

use super::{app, flick};

#[test]
fn a_wasm_plugin_declares_an_unboxed_type() {
    let main = "module App.Main exposing (main, metres)\n\nimport Pairs exposing (plugin wrapped)\n\nwrapped Meters\n\nmetres : Float\nlet metres = Meters.value (Meters.make 3.0)\n\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line (show metres)\n";
    let root = app("plugins-wrapped", main);
    let checked = flick(&root, &["check"]);
    assert!(checked.status.success(), "{checked:?}");
    let expanded = flick(&root, &["expand"]);
    let stdout = String::from_utf8_lossy(&expanded.stdout);
    assert!(
        stdout.contains("unboxed type Meters = Meters$Make Float"),
        "{stdout}"
    );
}
