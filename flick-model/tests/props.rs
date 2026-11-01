//! The properties of X2 on the programs of `flick_model::generate`: the
//! model accepts what the generator builds; Progress and Preservation
//! (`docs/formal/core.md` §4) hold on every state the machine passes
//! through; flick-core accepts a program, changed or not, exactly when the
//! model does, but for the local function without a signature flick-core
//! rejects by design. The programs of regions (X3, `docs/formal/regions.md`)
//! are checked by both and not run; a change may leak a region by each path
//! of its §4 but the fourth. The generator makes none of the forms of the
//! recorded incompletenesses Р23 and Р25. The forms of the holes of
//! TypeScript (X6, `docs/formal/ts-holes.md` §4) are checked by both and
//! not run either: the machine has no natives of `Gen.fl`. Not part of `just test`: `just
//! model` runs them, and `PROPTEST_CASES` sets how many programs each sees.

use std::path::Path;
use std::rc::Rc;

use flick_core::diagnostics::Severity;
use flick_core::source::SourceMap;
use flick_model::check::{Checker, check_config};
use flick_model::decls::Decls;
use flick_model::eval::{self, Lambda, Machine, Outcome};
use flick_model::generate::{self, Program};
use flick_model::run;
use flick_model::syntax::Term;
use flick_model::types::Type;
use proptest::collection::vec;
use proptest::option;
use proptest::prelude::*;

thread_local! {
    /// What the checker knows of `Gen.fl` and the built-in modules.
    static DECLS: Decls = {
        let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/generate/Gen.fl");
        let modules = run::compile(&file).expect("Gen.fl compiles");
        run::decls(&modules)
    };
}

/// Steps a run may take: the loops of the generator count down from
/// small numbers.
const FUEL: u64 = 20_000;

fn choices() -> impl Strategy<Value = Vec<u32>> {
    vec(any::<u32>(), 0..512)
}

/// The model's verdict on `p`: (Let-Sig) of `program : σ` at the top.
fn model(p: &Program) -> Result<(), String> {
    DECLS.with(|d| {
        Checker::new(d)
            .check_top(&p.term, &p.scheme)
            .map_err(|e| e.to_string())
    })
}

/// flick-core's errors on `source`; one of syntax is a bug of the printer.
fn core(source: &str) -> Vec<String> {
    let mut sources = SourceMap::default();
    let file = sources.add("Gen.fl", source);
    let errors: Vec<String> = flick_core::check(&sources, file)
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(
        !errors.iter().any(|e| e.starts_with("FLICK-S")),
        "the printer wrote what does not parse:\n{source}\n{}",
        errors.join("\n")
    );
    errors
}

/// `let program : σ = e in` what a runner does with it, the services
/// provided, and the type of that.
fn runnable(p: &Program) -> (Term, Type) {
    let program = || Term::Var("program".into());
    let (call, t) = match &p.scheme.body {
        Type::Fun(u, _, t) if **u == Type::con("Prelude", "Unit") => {
            (Term::app(program(), Term::unit()), (**t).clone())
        }
        Type::Eff(_, _, t) => (Term::Await(Rc::new(program())), (**t).clone()),
        t => (program(), t.clone()),
    };
    let e = Term::Let(
        "program".into(),
        Some(Rc::new(p.scheme.clone())),
        Rc::new(p.term.clone()),
        Rc::new(generate::runnable(call)),
    );
    (e, t)
}

/// Runs `p`; each state must have the program's type at a stage within
/// the first one's (Preservation), and the run must not get stuck
/// (Progress); an error it ends with must be one of `ε₀` (T1).
fn progress_and_preservation(decls: &eval::Program, d: &Decls, p: &Program) -> Result<(), String> {
    let (e0, t0) = runnable(p);
    let phi0 = check_config(d, &[], &e0, &t0).map_err(|e| format!("e₀: {e}"))?;
    let mut machine = Machine::new(decls, FUEL);
    let mut failure = None;
    let mut mem = Vec::new();
    let mut steps = 0;
    let outcome = machine.run_watched(e0, &mut |m: &[Term], _: &Lambda, e: &Term| {
        steps += 1;
        if failure.is_some() {
            return;
        }
        mem = m.to_vec();
        match check_config(d, m, e, &t0) {
            Ok(phi) if phi.within(&phi0) => {}
            Ok(phi) => {
                failure = Some(format!(
                    "step {steps}: `{phi}` is not within `{phi0}`:\n{e:?}"
                ));
            }
            Err(err) => failure = Some(format!("step {steps}: {err}\n{e:?}")),
        }
    });
    if let Some(f) = failure {
        return Err(f);
    }
    match outcome {
        Outcome::Stuck(why) => Err(format!("stuck after {steps} steps: {why}")),
        Outcome::Raise(v) => match check_config(d, &mem, &Term::Raise(Rc::new(v.clone())), &t0) {
            Ok(phi) if phi.within(&phi0) => Ok(()),
            other => Err(format!("raised {v:?}, not an error of `{phi0}`: {other:?}")),
        },
        Outcome::Value(_) | Outcome::Die(_) | Outcome::OutOfFuel => Ok(()),
    }
}

proptest! {
    // a failure becomes a case of `tests/check`, not a file of seeds
    // (most programs with a `scope` are not run, below)
    #![proptest_config(ProptestConfig { failure_persistence: None, max_global_rejects: 1 << 20, ..ProptestConfig::default() })]

    #[test]
    #[ignore = "a property of X2; run `just model`"]
    fn generated_programs_are_well_typed(c in choices()) {
        let p = generate::program(&c);
        if let Err(e) = model(&p) {
            return Err(TestCaseError::fail(format!("{e}\n{}", generate::source(&p))));
        }
    }

    #[test]
    #[ignore = "a property of X2; run `just model`"]
    fn progress_and_preservation_hold(c in choices()) {
        let p = generate::program(&c);
        prop_assume!(model(&p).is_ok());
        // shortcut: a program with a `scope` or a foreign value is not run,
        // the machine has no dynamics of regions (`regions.md` §3) and no
        // natives of `Gen.fl`; run the first once P2 adds them
        prop_assume!(generate::runs(&p));
        let globals = eval::Program::default();
        let checked = DECLS.with(|d| progress_and_preservation(&globals, d, &p));
        if let Err(e) = checked {
            return Err(TestCaseError::fail(format!("{e}\n{}", generate::source(&p))));
        }
    }

    #[test]
    #[ignore = "a property of X2; run `just model`"]
    fn checkers_agree(c in choices(), m in option::of(vec(any::<u32>(), 0..8))) {
        let mut p = generate::program(&c);
        if let Some(m) = &m {
            p = generate::mutate(&p, m);
        }
        let source = generate::source(&p);
        let model = model(&p);
        let core = core(&source);
        // a local function needs a signature in flick-core, not in the
        // rules (`core.md` §2.5, 6): a change can make one of a `let`
        let by_design = !core.is_empty() && core.iter().all(|e| e.starts_with("FLICK-T004"));
        prop_assert_eq!(
            model.is_ok(),
            core.is_empty() || by_design,
            "model: {:?}\nflick-core: {:?}\n{}",
            model,
            core,
            source
        );
    }
}
