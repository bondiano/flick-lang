//! A block whose code opens a module, from a component: the form crosses
//! the plugin interface as the rest of an arena does.

use std::fs;

use super::{app, flick};

#[test]
fn a_wasm_plugin_opens_a_module_around_the_users_code() {
    let main = "module App.Main exposing (main, next)\n\nimport Pairs exposing (plugin opens)\n\nnext : Float\nlet next = opens { version + 1.0 }\n\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line (show next)\n";
    let root = app("plugins-opens", main);
    let checked = flick(&root, &["check"]);
    assert!(checked.status.success(), "{checked:?}");
    // outside the block the module's name is not in sight
    let outside = main.replace("opens { version + 1.0 }", "opens { 1.0 } + version");
    fs::write(root.join("app/src/App/Main.fl"), outside).expect("write");
    let checked = flick(&root, &["check"]);
    let stderr = String::from_utf8_lossy(&checked.stderr);
    assert!(!checked.status.success(), "{stderr}");
    assert!(stderr.contains("cannot find value `version`"), "{stderr}");
}
