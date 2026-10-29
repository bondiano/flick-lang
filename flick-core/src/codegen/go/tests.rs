use std::iter;

use super::{Program, go, names};
use crate::build::builtin_modules;
use crate::ir::Module;
use crate::ir::tests::lower_text;

/// The Go program of `text` with the built-in modules, by file, and the
/// messages of what could not be compiled.
fn compile(text: &str) -> (Vec<(String, String)>, Vec<String>) {
    let ir = lower_text("App", text).expect("lowers");
    let modules: Vec<&Module> = iter::once(&ir).chain(builtin_modules()).collect();
    let output = go(&Program {
        modules: &modules,
        entry: &ir.name,
        package: &|_| None,
        own: &|m| m == ir.name,
    });
    let messages = output.diagnostics.into_iter().map(|d| d.message).collect();
    (output.files, messages)
}

fn file<'a>(files: &'a [(String, String)], name: &str) -> &'a str {
    &files
        .iter()
        .find(|(path, _)| path == name)
        .unwrap_or_else(|| panic!("no file {name}"))
        .1
}

#[test]
fn module_files_are_named_by_module() {
    assert_eq!(names::file("App.Main"), "m_App_Main.go");
    assert_eq!(names::init("App.Main"), "init_App_Main");
}

#[test]
fn pure_functions_are_go_functions_of_all_their_arguments() {
    let (files, errors) = compile(
        "module App exposing (main)\n\
         add : Int -> Int -> Int\nlet add a b = a + b\n\
         inc : Int -> Int\nlet inc = add 1\n\
         main : Unit ->{} Unit\nlet main () = Platform.stdout_line (show (inc (add 2 3)))\n",
    );
    assert!(errors.is_empty(), "{errors:?}");
    insta::assert_snapshot!(file(&files, "m_App.go"));
    let main = file(&files, "main.go");
    assert!(
        main.contains("init_App()") && main.contains("F_App__main(flickrt.Unit)"),
        "{main}"
    );
}

#[test]
fn only_what_main_reaches_is_compiled() {
    let (files, errors) = compile(
        "module App exposing (main)\n\
         later : Unit ->{} Int\nlet later () = await (task { 1 })\n\
         main : Unit ->{} Unit\nlet main () = Platform.stdout_line \"hi\"\n",
    );
    assert!(errors.is_empty(), "{errors:?}");
    assert!(!file(&files, "m_App.go").contains("later"));
}

#[test]
fn what_the_target_cannot_run_yet_is_reported() {
    let (_, errors) = compile(
        "module App exposing (main)\n\
         main : Unit ->{} Unit\nlet main () = Platform.stdout_line (show (await (task { 1 })))\n",
    );
    assert!(
        errors
            .iter()
            .any(|e| e == "a `task` is not available on the go target yet"),
        "{errors:?}"
    );
    let (_, errors) = compile(
        "module App exposing (main)\n\
         now : Unit ->{} Float\nforeign sync now from global \"Date.now\"\n\
         main : Unit ->{} Unit\nlet main () = Platform.stdout_line (show (now ()))\n",
    );
    assert_eq!(
        errors,
        ["`App.now` (foreign `Date.now` global) is not available on the go target"]
    );
}
