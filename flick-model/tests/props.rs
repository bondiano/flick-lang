//! The model properties on the programs of `flick_model::generate`: the
//! model accepts what the generator builds; Progress and Preservation
//! (`docs/formal/core.md` §4) hold on every state the machine passes
//! through; flick-core accepts a program, changed or not, exactly when the
//! model does, but for the local function without a signature flick-core
//! rejects by design. The programs of regions (`docs/formal/regions.md`)
//! are run too: each state has `⊢ Λ`, and no run uses a capability after
//! its region closed (T4.1), which the leaks reach. A change may leak a
//! region by each path of its §4 but the fourth. The generator makes none
//! of the forms of the recorded incompletenesses of `docs/formal/regions.md`: an empty cell and
//! the region of a task argument. The forms of
//! the holes of TypeScript (`docs/formal/ts-holes.md` §4) are checked
//! by both and not run: the machine has no natives for their foreign
//! values. Not part of `just test`: `just model` runs them, and
//! `PROPTEST_CASES` sets how many programs each sees.

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
    /// What the checker knows of `Gen.fl` and the built-in modules, and
    /// their values the machine runs.
    static GEN: (Decls, eval::Program) = {
        let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/generate/Gen.fl");
        let modules = run::compile(&file).expect("Gen.fl compiles");
        (run::decls(&modules), run::program(&modules))
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
    GEN.with(|(d, _)| {
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
    let phi0 =
        check_config(d, &[], &Lambda::default(), &e0, &t0).map_err(|e| format!("e₀: {e}"))?;
    let mut machine = Machine::new(decls, FUEL);
    let mut failure = None;
    let mut mem = Vec::new();
    let mut regions = Lambda::default();
    let mut steps = 0;
    let outcome = machine.run_watched(e0, &mut |m: &[Term], l: &Lambda, e: &Term| {
        steps += 1;
        if failure.is_some() {
            return;
        }
        mem = m.to_vec();
        regions = l.clone();
        match check_config(d, m, l, e, &t0) {
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
        Outcome::Raise(v) => {
            match check_config(d, &mem, &regions, &Term::Raise(Rc::new(v.clone())), &t0) {
                Ok(phi) if phi.within(&phi0) => Ok(()),
                other => Err(format!("raised {v:?}, not an error of `{phi0}`: {other:?}")),
            }
        }
        Outcome::Value(_) | Outcome::Die(_) | Outcome::OutOfFuel => Ok(()),
    }
}

/// T4 on the machine (`docs/formal/regions.md` §5.3): each leak of the
/// generator, with a use of what leaked after its region closed, is
/// rejected by the model, and run, it reaches the use after close that
/// no accepted program reaches (`progress_and_preservation_hold`).
#[test]
fn leaks_reach_a_use_after_close() {
    let mut seen = 0;
    for path in 0..13 {
        let Some(p) = generate::leak_used(path) else {
            continue;
        };
        let source = generate::source(&p);
        assert!(
            model(&p).is_err(),
            "path {path}: the model accepts\n{source}"
        );
        let (e, _) = runnable(&p);
        let outcome = GEN.with(|(_, globals)| Machine::new(globals, FUEL).run(e));
        assert!(
            matches!(&outcome, Outcome::Stuck(w) if w.starts_with("use after")),
            "path {path}: {outcome:?}\n{source}"
        );
        seen += 1;
    }
    assert_eq!(seen, 11);
}

/// A signed `let` keeps its signature when it steps: `(fun g x -> g x) (try … with _ -> as4)`, the λ of `v1` after
/// its `let` with a signature stepped, whose argument needs the runtime
/// arrow of the signature. A case the model properties found.
#[test]
#[allow(
    clippy::unreadable_literal,
    reason = "the choices as proptest printed them"
)]
fn a_function_keeps_its_signature() {
    let c = [
        53872532, 614429763, 316647345, 3323234475, 11786860, 1371144852, 2703845322, 1737490265,
        255759160, 2157138854, 3717280138, 1624885393, 2433392247, 2010853328, 2566700010,
        409181594, 736146643, 0, 17231705, 3249833829, 0, 0, 0, 3387466705, 0, 3191903211, 0, 0,
        1714366921, 0, 2364491101, 3175111223, 0, 95212, 8248188, 12185093, 0, 0, 0, 0, 2690176445,
        0, 0, 0, 0, 0, 0, 0, 0,
    ];
    let p = generate::program(&c);
    assert!(model(&p).is_ok());
    let checked = GEN.with(|(d, globals)| progress_and_preservation(globals, d, &p));
    assert!(checked.is_ok(), "{checked:?}\n{}", generate::source(&p));
}

proptest! {
    // a failure becomes a case of `tests/check`, not a file of seeds
    // (the programs with a hole of TypeScript are not run, below)
    #![proptest_config(ProptestConfig { failure_persistence: None, max_global_rejects: 1 << 20, ..ProptestConfig::default() })]

    #[test]
    #[ignore = "a model property; run `just model`"]
    fn generated_programs_are_well_typed(c in choices()) {
        let p = generate::program(&c);
        if let Err(e) = model(&p) {
            return Err(TestCaseError::fail(format!("{e}\n{}", generate::source(&p))));
        }
    }

    #[test]
    #[ignore = "a model property; run `just model`"]
    fn progress_and_preservation_hold(c in choices()) {
        let p = generate::program(&c);
        prop_assume!(model(&p).is_ok());
        prop_assume!(generate::runs(&p));
        let checked = GEN.with(|(d, globals)| progress_and_preservation(globals, d, &p));
        if let Err(e) = checked {
            return Err(TestCaseError::fail(format!("{e}\n{}", generate::source(&p))));
        }
    }

    #[test]
    #[ignore = "a model property; run `just model`"]
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
