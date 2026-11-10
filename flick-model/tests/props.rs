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
            Err(err) => {
                failure = Some(format!(
                    "step {steps}: {err}
{e:?}
Λ: {:?}",
                    l.regions
                ));
            }
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
///
/// The choices are the picks of the case proptest printed: each below the
/// number of forms then, so forms added later at the end keep the program.
#[test]
fn a_function_keeps_its_signature() {
    let c = [
        0, 3, 0, 3, 0, 0, 2, 1, 1, 6, 2, 4, 7, 0, 10, 0, 3, 0, 0, 3, 0, 0, 0, 4, 0, 7, 0, 0, 1, 0,
        1, 11, 0, 4, 0, 5, 0, 0, 0, 0, 5,
    ];
    let p = generate::program(&c);
    assert!(model(&p).is_ok());
    let checked = GEN.with(|(d, globals)| progress_and_preservation(globals, d, &p));
    assert!(checked.is_ok(), "{checked:?}\n{}", generate::source(&p));
}

/// A change leaves a dictionary alone, which the source does not write: a
/// cast of `instance Rank Int` printed its name. A case `checkers_agree`
/// found.
#[test]
#[allow(
    clippy::unreadable_literal,
    reason = "the choices as proptest printed them"
)]
fn a_change_leaves_dictionaries_alone() {
    let c = [
        1060304624, 1385092055, 157142940, 0, 0, 2106292236, 3731932068, 1712555527, 0, 1497749644,
        179845092, 550714479,
    ];
    let p = generate::mutate(&generate::program(&c), &[3364843738, 1681919443]);
    let source = generate::source(&p);
    let core = core(&source);
    assert_eq!(
        model(&p).is_ok(),
        core.is_empty() || generate::by_design(&core),
        "{core:?}\n{source}"
    );
}

/// A function with a class constraint is called at a type the generator
/// builds a value of: one at `Oops` took `()`. A case
/// `generated_programs_are_well_typed` found.
#[test]
#[allow(
    clippy::unreadable_literal,
    reason = "the choices as proptest printed them"
)]
fn a_constrained_call_has_its_argument() {
    let c = [
        698017122, 21051635, 4607202, 146514692, 3913711338, 0, 793500998, 1683621662, 0, 0,
        251895549, 388005714, 991649132, 1165236, 180835688, 3687733928, 1099618755, 863166628,
        2307264338, 0, 505020075, 3260581365, 0, 0, 2299046473, 0, 268533177, 2959975492,
        2121448646, 1497048870, 62919524, 23206904, 418612195, 2862996111, 511838154, 208771041,
        3968671193, 0, 394628986,
    ];
    let p = generate::program(&c);
    assert!(model(&p).is_ok(), "{}", generate::source(&p));
}

/// The work a region keeps is typed with the names of its `scope`s: a
/// task spawned in `scope s1` whose local signature writes `^{s1}` was
/// typed in `Λ` without `s1`, and its use of the region did not fit. A case
/// `progress_and_preservation_hold` found.
#[test]
#[allow(
    clippy::unreadable_literal,
    reason = "the choices as proptest printed them"
)]
fn work_of_a_region_names_its_scope() {
    let c = [
        353888456, 969789605, 1953347358, 892319260, 59954573, 61239403, 328474308, 3050587714,
        317413424, 0, 3925569377, 286594538, 331073892, 1239522750, 2644842610, 0, 0, 0, 613638093,
        2117780986, 0, 103271129, 3875599509, 810550817, 0, 1179730476, 0, 3142153284, 21161750,
        605014265, 0, 64989447, 0, 0,
    ];
    let p = generate::program(&c);
    assert!(model(&p).is_ok(), "{}", generate::source(&p));
    let checked = GEN.with(|(d, globals)| progress_and_preservation(globals, d, &p));
    assert!(checked.is_ok(), "{checked:?}\n{}", generate::source(&p));
}

/// A change leaves the annotation of a method's argument, which fixes the
/// dictionary the model's term names and the source does not write: `rank`
/// of `Bool` got an `Int`, and flick-core took `Rank Int`. A case
/// `checkers_agree` found.
#[test]
#[allow(
    clippy::unreadable_literal,
    reason = "the choices as proptest printed them"
)]
fn a_change_keeps_a_methods_annotation() {
    let c = [
        2177478260, 286759169, 812953880, 3858219642, 839833810, 221447766, 0, 0, 3741861477,
        3440751098, 0, 0, 1162552687, 196817947, 1879677901, 1709693216,
    ];
    let p = generate::mutate(&generate::program(&c), &[181709788, 1865841925]);
    let source = generate::source(&p);
    let core = core(&source);
    assert_eq!(
        model(&p).is_ok(),
        core.is_empty() || generate::by_design(&core),
        "{core:?}\n{source}"
    );
}

/// A local function bound with its signature keeps the qualifier inferred
/// for it once its `let` steps to a `fix` (`core.md` §3.6): `v4 : Unit ->{}
/// Unit` uses a region, and the `fix` in a child read its signature bare. A
/// case `progress_and_preservation_hold` found.
#[test]
#[allow(
    clippy::unreadable_literal,
    reason = "the choices as proptest printed them"
)]
fn a_stepped_signature_keeps_its_qualifier() {
    let c = [
        1425097824, 506836982, 1915265, 0, 885644193, 1741814068, 577031872, 1905967045, 0,
        701402791, 3155179838, 3121792442, 3210598022, 0, 3391211448, 0, 687554087, 3160364061, 0,
        2702157866, 2194849492, 0, 3391091691, 1362750893, 0, 5597883, 0, 0,
    ];
    let p = generate::program(&c);
    assert!(model(&p).is_ok(), "{}", generate::source(&p));
    let checked = GEN.with(|(d, globals)| progress_and_preservation(globals, d, &p));
    assert!(checked.is_ok(), "{checked:?}\n{}", generate::source(&p));
}

/// Every instance a program names is one of `Gen.fl`.
#[test]
fn the_instances_of_the_programs_are_declared() {
    GEN.with(|(d, _)| {
        for i in generate::instances() {
            assert!(d.instances.contains_key(&i), "no `{i}` in Gen.fl");
        }
    });
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
        // a change can make a local function of a `let` without its
        // signature, or leave a type of a method open
        let by_design = generate::by_design(&core);
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
