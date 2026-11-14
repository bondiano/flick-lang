//! A partial handler forwards unmentioned operations, not its whole effect.
//! Expectations follow the operation rule of `docs/formal/core.md` §3.7.

use std::path::Path;
use std::rc::Rc;

use flick_model::eval::{Machine, Outcome, Program};
use flick_model::handler::{Handled, Handler, Op, OpClass};
use flick_model::run;
use flick_model::syntax::{Const, Env, Lit, Path as Global, Term};

fn effect() -> Global {
    Global::new("Fixture", "PairOps")
}

fn operation(name: &str) -> Term {
    Term::app(
        Term::Field(
            Rc::new(Term::app(Term::konst(Const::Ask(effect())), Term::unit())),
            name.into(),
        ),
        Term::unit(),
    )
}

fn handled(ops: &[(&str, i32)], body: Term) -> Term {
    Term::Handler(Rc::new(Handler {
        effects: vec![Handled {
            effect: effect(),
            ops: ops
                .iter()
                .map(|(name, n)| Op {
                    name: (*name).into(),
                    class: OpClass::Tail,
                    dicts: 0,
                    arity: 1,
                    covers: true,
                    function: Term::lam("_", Term::Lit(Lit::Int(*n))),
                })
                .collect(),
        }],
        body,
        ret: None,
        errors: Rc::from([]),
    }))
}

fn outcome(term: Term) -> Outcome {
    Machine::new(&Program::default(), 10_000).run(term)
}

#[test]
fn an_unmentioned_operation_reaches_the_outer_handler() {
    let term = handled(
        &[("left", 10), ("right", 20)],
        handled(&[("left", 1)], operation("right")),
    );
    assert!(matches!(
        outcome(term),
        Outcome::Value(Term::Lit(Lit::Int(20)))
    ));
}

#[test]
fn a_complete_provider_still_shadows_the_outer_handler() {
    let provider = Rc::new(Env::from([(
        effect(),
        Term::Record(Rc::from([(
            "right".into(),
            Term::lam("_", Term::Lit(Lit::Int(30))),
        )])),
    )]));
    let term = handled(
        &[("left", 10), ("right", 20)],
        Term::With(
            provider,
            Rc::new(handled(&[("left", 1)], operation("right"))),
        ),
    );
    assert!(matches!(
        outcome(term),
        Outcome::Value(Term::Lit(Lit::Int(30)))
    ));
}

#[test]
fn a_malformed_complete_provider_is_not_a_partial_handler() {
    let provider = Rc::new(Env::from([(effect(), Term::Record(Rc::from([])))]));
    let term = handled(
        &[("left", 10), ("right", 20)],
        Term::With(
            provider,
            Rc::new(handled(&[("left", 1)], operation("right"))),
        ),
    );
    assert!(matches!(outcome(term), Outcome::Stuck(why) if why == "no field `right`"));
}

#[test]
fn forwarding_checks_only_the_selected_providers_liveness() {
    let provider = Rc::new(Env::from([(
        effect(),
        Term::Record(Rc::from([(
            "right".into(),
            Term::lam("_", Term::Lit(Lit::Int(30))),
        )])),
    )]));
    let globals = Program::default();
    let mut machine = Machine::new(&globals, 10_000);
    assert!(matches!(
        machine.run(Term::With(provider.clone(), Rc::new(Term::unit()))),
        Outcome::Value(_)
    ));
    // Reinstall an ended environment to probe the runtime check. A nearer
    // matching arm must not inspect it; a forwarded operation must check it.
    let inner = handled(&[("left", 1)], operation("left"));
    assert!(matches!(
        machine.run(Term::With(provider.clone(), Rc::new(inner))),
        Outcome::Value(Term::Lit(Lit::Int(1)))
    ));
    let inner = handled(&[("left", 1)], operation("right"));
    assert!(matches!(
        machine.run(Term::With(provider, Rc::new(inner))),
        Outcome::Stuck(why) if why == "use after release of service `Fixture.PairOps`"
    ));
}

#[test]
fn a_bare_internal_ask_is_not_a_merged_service_record() {
    let ask = Term::app(Term::konst(Const::Ask(effect())), Term::unit());
    let term = handled(&[("right", 20)], handled(&[("left", 1)], ask));
    assert!(matches!(
        outcome(term), Outcome::Value(Term::Record(fields))
            if fields.len() == 1 && &*fields[0].0 == "left"
    ));
}

#[test]
fn checked_partial_handlers_forward_at_the_first_runtime_stage() {
    let file =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run/codegen/partial_handlers.fl");
    let modules = run::compile(&file).expect("source checks");
    let program = run::program(&modules);
    assert_eq!(run::check(&modules, &program), Vec::<String>::new());
    let main = run::main_of(&modules).expect("main");
    let ran = run::run_main(&program, &main, 100_000);
    assert_eq!(ran.stdout, "21\n12\n21\n121\n25\n", "{:?}", ran.outcome);
    assert!(
        matches!(ran.outcome, Outcome::Value(_)),
        "{:?}",
        ran.outcome
    );
}
