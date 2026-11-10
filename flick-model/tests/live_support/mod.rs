//! Shared helpers of the live-discipline tests: checking a generated
//! program under either discipline, running it, and building probes.

#![allow(dead_code)]

use std::path::Path;
use std::rc::Rc;

use flick_core::diagnostics::Severity;
use flick_core::source::SourceMap;
use flick_model::check::Checker;
use flick_model::decls::Decls;
use flick_model::eval::{self, Machine, Outcome};
use flick_model::generate::{self, Program};
use flick_model::run;
use flick_model::syntax::{Path as Global, Term};
use flick_model::types::{Row, Scheme, Stage, Type};

thread_local! {
    static GEN: (Decls, eval::Program) = {
        let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/generate/Gen.fl");
        let modules = run::compile(&file).expect("Gen.fl compiles");
        (run::decls(&modules), run::program(&modules))
    };
}

const FUEL: u64 = 20_000;

pub fn verdict(p: &Program, live: bool) -> Result<(), String> {
    GEN.with(|(d, _)| {
        let mut c = if live {
            Checker::live(d)
        } else {
            Checker::new(d)
        };
        c.check_top(&p.term, &p.scheme).map_err(|e| e.to_string())
    })
}

/// `let program : σ = e in` its call, the services provided.
pub fn runnable(p: &Program) -> Term {
    let program = || Term::Var("program".into());
    let call = match &p.scheme.body {
        Type::Fun(u, _, _) if **u == Type::con("Prelude", "Unit") => {
            Term::app(program(), Term::unit())
        }
        Type::Eff(..) => Term::Await(Rc::new(program())),
        _ => program(),
    };
    Term::Let(
        "program".into(),
        Some(Rc::new(p.scheme.clone())),
        Rc::new(p.term.clone()),
        Rc::new(generate::runnable(call)),
    )
}

pub fn run(p: &Program) -> Outcome {
    GEN.with(|(_, globals)| Machine::new(globals, FUEL).run(runnable(p)))
}

pub fn g(name: &str) -> Term {
    Term::Global(Global::new("Gen", name))
}

pub fn v(x: &str) -> Term {
    Term::Var(x.into())
}

pub fn open_in(s: &str, body: Term) -> Term {
    let open = Term::app(g("open_scoped"), Term::string("x"));
    Term::Scope(
        s.into(),
        Rc::new(Term::Let("h".into(), None, Rc::new(open), Rc::new(body))),
    )
}

/// `fun u -> e` of `Unit ->{} Unit`.
pub fn thunk_program(e: Term) -> Program {
    let unit = Type::con("Prelude", "Unit");
    let empty = Stage::Runtime(Row::none(), Row::none());
    Program {
        scheme: Scheme::new(Vec::new(), Type::fun(unit.clone(), empty, unit)),
        term: Term::lam("u", e),
    }
}

/// flick-core's errors on the program printed as Flick.
pub fn core(p: &Program) -> Vec<String> {
    let mut sources = SourceMap::default();
    let file = sources.add("Gen.fl", generate::source(p));
    flick_core::check(&sources, file)
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

pub fn probe(name: &str, p: &Program) -> (Result<(), String>, Result<(), String>, Outcome) {
    let (captures, live) = (verdict(p, false), verdict(p, true));
    let outcome = run(p);
    println!("probe {name}: captures {captures:?}; live {live:?}; run {outcome:?}");
    println!(
        "probe {name}: flick-core {:?}\n{}",
        core(p),
        generate::source(p)
            .rsplit("shape_name from")
            .next()
            .unwrap_or("")
    );
    (captures, live, outcome)
}

/// flick-core's errors on the program printed as Flick, under
/// `[build] regions = "live"`.
pub fn core_live(p: &Program) -> Vec<String> {
    use flick_core::types::RegionMode;
    let mut sources = SourceMap::default();
    let file = sources.add("Gen.fl", generate::source(p));
    flick_core::check_regions(&sources, file, RegionMode::Live)
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}
