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
        location: &|_, _| None,
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
        main.contains("init_App()") && main.contains("F_App__main(fb, flickrt.Unit)"),
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

#[test]
fn tail_calls_jump_instead_of_calling() {
    let (files, errors) = compile(
        "module App exposing (main)\n\
         sum : Int -> Int -> Int\nlet rec sum acc n = if n == 0 then acc else sum (acc + n) (n - 1)\n\
         rec {\n\
         even : Int -> Bool\nlet even n = n == 0 || odd (n - 1)\n\
         odd : Int -> Bool\nlet odd n = n != 0 && even (n - 1)\n\
         }\n\
         main : Unit ->{} Unit\nlet main () = Platform.stdout_line (show (sum 0 3) ++ show (even 4))\n",
    );
    assert!(errors.is_empty(), "{errors:?}");
    let code = file(&files, "m_App.go");
    let sum = &code[code.find("func F_App__sum(").expect("sum")..];
    let sum = &sum[..sum.find("\n}\n").expect("end of sum")];
    assert!(
        sum.contains("for {") && sum.contains("continue") && !sum.contains("F_App__sum(f"),
        "{sum}"
    );
    assert!(
        code.contains("func G_App__even(state int, v0 flickrt.Value)"),
        "{code}"
    );
    assert!(code.contains("return G_App__even(1, "), "{code}");
}

#[test]
fn effectful_tail_calls_jump_but_not_out_of_a_try() {
    let (files, errors) = compile(
        "module App exposing (main)\n\
         down : Ref Int -> Int ->{} Unit\n\
         let rec down cell n = if n == 0 then () else\n  Ref.set cell n\n  down cell (n - 1)\n\
         main : Unit ->{} Unit\nlet main () = down (Ref.make 0) 3\n",
    );
    assert!(errors.is_empty(), "{errors:?}");
    let code = file(&files, "m_App.go");
    assert!(
        code.contains("for {") && code.contains("continue"),
        "{code}"
    );
    let (files, _) = compile(
        "module App exposing (main)\n\
         type error Stop = Stop\n\
         again : Int ->{; Stop} Int\n\
         let rec again n = try again (n - 1) with | Stop -> n\n\
         main : Unit ->{; Stop} Unit\nlet main () = Platform.stdout_line (show (again 3))\n",
    );
    let code = file(&files, "m_App.go");
    assert!(!code.contains("continue"), "{code}");
}
