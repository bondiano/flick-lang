use std::fs;

use flick_ir::{ExprKind, ItemKind, Program};

use super::{APP, MAIN, flick_in, project};

/// The `main` of `App.Main` in `program`.
fn main_of(program: &Program) -> &ExprKind {
    let module = (program.modules.iter())
        .find(|m| m.name == "App.Main")
        .expect("App.Main");
    let body = module.items.iter().find_map(|i| match &i.kind {
        ItemKind::Value { name, body, .. } if name == "main" => Some(body),
        _ => None,
    });
    &body.expect("main").kind
}

#[test]
fn emit_ir_writes_the_program_the_backend_gets() {
    let dir = project(
        "build_emit_ir",
        &[("flick.toml", APP), ("src/App/Main.fl", MAIN)],
    );
    let out = flick_in(&dir, &["build", "--emit-ir"]);
    assert!(out.status.success(), "{out:?}");
    let text = fs::read_to_string(dir.join("dist/program.ir.json")).expect("the IR");
    let program = Program::from_json(&text).expect("a program of this version");
    assert_eq!(program.ir_version, flick_ir::VERSION);
    assert!(
        program.files.iter().any(|f| f.ends_with("Main.fl")),
        "{:?}",
        program.files
    );
    assert!(matches!(main_of(&program), ExprKind::Lambda { .. }));

    // a second build writes it again, though nothing changed
    fs::remove_file(dir.join("dist/program.ir.json")).expect("removed");
    let out = flick_in(&dir, &["build", "--emit-ir=cbor", "--profile", "release"]);
    assert!(out.status.success(), "{out:?}");
    let bytes = fs::read(dir.join("dist/program.ir.cbor")).expect("the IR");
    let program = Program::from_cbor(&bytes).expect("a program of this version");
    assert!(matches!(main_of(&program), ExprKind::Lambda { .. }));
    assert!(!dir.join("dist/program.ir.json").exists());
}
