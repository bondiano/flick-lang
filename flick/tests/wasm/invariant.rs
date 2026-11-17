//! A type a component declares with an invariant: the declaration crosses
//! the plugin interface, and the compiler proves the check where the
//! constructor is applied, in the component's own code as in the program.

use super::{app, flick};

#[test]
fn a_wasm_plugin_declares_a_type_with_an_invariant() {
    let main = "module App.Main exposing (main)\n\nimport Pairs exposing (plugin kept)\n\nkept Positive { Checks.positive }\n\nmade : Float -> Option Positive\nlet made x = if Positive.check x then Some (Positive.prove x) else None\n\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line (show (Option.map Positive.value (made 2.0)))\n";
    let root = app("plugins-invariant", main);
    let checked = flick(&root, &["check"]);
    assert!(checked.status.success(), "{checked:?}");
    let expanded = flick(&root, &["expand"]);
    let stdout = String::from_utf8_lossy(&expanded.stdout);
    assert!(
        stdout.contains("@invariant$of check value prove"),
        "{stdout}"
    );
    // without the fact, and in the component's own code, the check is unproved
    let unproved = main.replace(
        "if Positive.check x then Some (Positive.prove x) else None",
        "Some (Positive.prove x)",
    );
    let root = app("plugins-invariant-unproved", &unproved);
    let checked = flick(&root, &["check"]);
    let stderr = String::from_utf8_lossy(&checked.stderr);
    assert!(stderr.contains("FLICK-T076"), "{stderr}");
    let leaky = main
        .replace("plugin kept", "plugin leaky")
        .replace("kept Positive", "leaky Positive");
    let root = app("plugins-invariant-leaky", &leaky);
    let checked = flick(&root, &["check"]);
    let stderr = String::from_utf8_lossy(&checked.stderr);
    assert!(
        stderr.contains("FLICK-T078") && stderr.contains("FLICK-T076"),
        "{stderr}"
    );
}
