//! A program of one file: compiled by flick-core, translated to λFlick
//! and run from its `main`, as a runner does (§2.6): `main ()`, and the
//! task it gives awaited.

use std::collections::HashSet;
use std::rc::Rc;
use std::{fs, io, path};

use flick_core::diagnostics::Severity;
use flick_core::project::{self, Compiled, Host};
use flick_core::source::SourceMap;
use flick_core::{build, ir};

use crate::check::check_program;
use crate::decls::Decls;
use crate::eval::{Machine, Outcome, Program};
use crate::syntax::{Name, Path, Term};
use crate::translate;

struct Files;

impl Host for Files {
    fn read(&self, p: &path::Path) -> io::Result<Vec<u8>> {
        fs::read(p)
    }

    fn display(&self, p: &path::Path) -> String {
        p.display().to_string()
    }
}

/// The modules of the program in `file`, or its errors.
pub fn compile(file: &path::Path) -> Result<Vec<Compiled>, String> {
    let file = fs::canonicalize(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let mut sources = SourceMap::default();
    let (diagnostics, compilation) =
        project::compile(&Files, &mut sources, None, &[project::normalize(&file)]);
    let errors: Vec<String> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    if errors.is_empty() {
        Ok(compilation.modules)
    } else {
        Err(errors.join("\n"))
    }
}

/// The program of `modules` and the built-in modules.
pub fn program(modules: &[Compiled]) -> Program {
    let builtins: &[ir::Module] = build::builtin_modules();
    translate::program(modules.iter().map(|m| &m.ir).chain(builtins))
}

/// What the checker knows of `modules` and the built-in modules.
pub fn decls(modules: &[Compiled]) -> Decls {
    let mut decls = Decls::default();
    let builtins: &[ir::Module] = build::builtin_modules();
    for m in modules.iter().map(|m| &m.ir).chain(builtins) {
        decls.add(&m.declarations);
    }
    decls.finish();
    decls
}

/// The type errors of the top-level values of `modules`.
pub fn check(modules: &[Compiled], program: &Program) -> Vec<String> {
    let checked: HashSet<Name> = modules
        .iter()
        .map(|m| Name::from(m.name.as_str()))
        .collect();
    check_program(program, &decls(modules), &checked)
        .into_iter()
        .map(|(p, e)| format!("{p}: {e}"))
        .collect()
}

/// The `main` of the module that exports one.
pub fn main_of(modules: &[Compiled]) -> Result<Path, String> {
    modules
        .iter()
        .find(|m| m.interface.exports_value("main"))
        .map(|m| Path::new(&m.name, "main"))
        .ok_or_else(|| "no module exports `main`".to_owned())
}

/// What a run printed and how it ended.
pub struct Ran {
    pub outcome: Outcome,
    pub stdout: String,
}

/// Runs `main` of `program` within `fuel` steps.
pub fn run_main(program: &Program, main: &Path, fuel: u64) -> Ran {
    let mut machine = Machine::new(program, fuel);
    let mut outcome = machine.run(Term::Global(main.clone()));
    // `main` is a function of `()`, or a task; a task is awaited
    if let Outcome::Value(v @ (Term::Lam(..) | Term::Fix(..) | Term::Apply(..))) = outcome {
        outcome = machine.run(Term::app(v, Term::unit()));
    }
    if let Outcome::Value(t @ Term::Task(_)) = outcome {
        outcome = machine.run(Term::Await(Rc::new(t)));
    }
    Ran {
        outcome,
        stdout: machine.stdout,
    }
}

/// Compiles and runs the program in `file`.
pub fn run_file(file: &path::Path, fuel: u64) -> Result<Ran, String> {
    let modules = compile(file)?;
    let main = main_of(&modules)?;
    Ok(run_main(&program(&modules), &main, fuel))
}
