use std::iter;

use super::{Program, go};
use crate::build::builtin_modules;
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::ir::Module;
use crate::ir::tests::lower_text;

/// The target diagnostics of `text` with the built-in modules.
fn compile(text: &str) -> Vec<Diagnostic> {
    let ir = lower_text("App", text).expect("lowers");
    let modules: Vec<&Module> = iter::once(&ir).chain(builtin_modules()).collect();
    let output = go(&Program {
        suites: &[],
        modules: &modules,
        entry: &ir.name,
        package: &|_| None,
        own: &|m| m == ir.name,
        location: &|_, _| None,
        foreign_file: &|_, _| None,
    });
    output.diagnostics
}

#[test]
fn what_the_target_cannot_run_yet_is_reported() {
    let errors = compile(
        "module App exposing (main)\n\
         now : Unit ->{} Float\nforeign sync now from global \"Date.now\"\n\
         main : Unit ->{} Unit\nlet main () = Platform.stdout_line (show (now ()))\n",
    );
    assert_eq!(
        errors.iter().map(|error| error.code).collect::<Vec<_>>(),
        [codes::P012]
    );
    assert_eq!(errors[0].severity, Severity::Error);
    assert!(errors[0].span.is_some());
}

#[test]
fn unreachable_foreigns_do_not_reject_a_supported_entry() {
    let errors = compile(
        "module App exposing (main)\n\
         now : Unit ->{} Float\nforeign sync now from global \"Date.now\"\n\
         main : Unit ->{} Unit\nlet main () = Platform.stdout_line \"hi\"\n",
    );
    assert_eq!(errors, []);
}

#[test]
fn a_missing_local_go_foreign_reports_a_target_diagnostic() {
    let errors = compile(
        "module App exposing (main)\n\
         wait : Float ->{} Unit\nforeign async wait from \"./gone.mjs\" \"wait\"\n\
         main : Unit ->{} Unit\nlet main () = wait 1\n",
    );
    assert_eq!(
        errors.iter().map(|error| error.code).collect::<Vec<_>>(),
        [codes::P012]
    );
    assert_eq!(errors[0].severity, Severity::Error);
    assert!(errors[0].span.is_some());
}
