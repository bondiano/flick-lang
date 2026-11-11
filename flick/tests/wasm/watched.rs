//! What a package may watch besides calls: a tag and a service.

use std::fs;

use super::{LIB, app, flick};

/// A module of the library with a maker and a service.
const MARKS: &str = "module Marks exposing (fresh, Tick)\n\n@maker\nfresh : Float -> Float\nlet fresh n = n\n\nservice Tick =\n  now : Unit ->{} Float\n";

#[test]
fn a_wasm_plugin_watches_a_tag_and_a_service() {
    let main = "module App.Main exposing (main, made, timed)\n\nimport Marks\n\nmade : Float -> Float\nlet made n = Marks.fresh n\n\ntimed : Unit ->{Marks.Tick} Float\nlet timed () = Marks.Tick.now ()\n\nmain : Unit ->{; AppError} Unit\nlet main () = ()\n";
    let root = app("plugins-marks", main);
    let watch = "watch = [\"@maker\", \"Marks.Tick\"]\n";
    let lib = LIB.replace("watch = [\"Pairs.tally\", \"*.boom\"]\n", watch);
    assert!(lib.contains(watch));
    fs::write(root.join("lib/flick.toml"), lib).expect("write");
    fs::write(root.join("lib/src/Marks.fl"), MARKS).expect("write");
    let checked = flick(&root, &["check"]);
    let stderr = String::from_utf8_lossy(&checked.stderr);
    assert!(!checked.status.success(), "{stderr}");
    // the name of a maker inside a function, with the row of the function
    assert!(stderr.contains("pairs/X006"), "{stderr}");
    let named = "`Marks.fresh` reaches `Marks.fresh` of `@maker` inside {}";
    assert!(stderr.contains(named), "{stderr}");
    // the function whose signature names the service, with what it gives
    assert!(stderr.contains("pairs/X007"), "{stderr}");
    let asks = "`timed` asks for `Marks.Tick` to give `Float`";
    assert!(stderr.contains(asks), "{stderr}");
}
