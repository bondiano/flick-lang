//! Operational owner context: C transports evidence independently of D.
use flick_model::eval::{Machine, Outcome, Program};
use flick_model::syntax::{Arm, Pat, Term};
use std::rc::Rc;
fn scope(name: &str, body: Term) -> Term {
    Term::Scope(name.into(), Rc::new(body))
}
fn evidence(owner: Term, body: Term) -> Term {
    Term::WithOwnerEvidence(Rc::new(owner), Rc::new(body))
}
fn activate(owner: Term, body: Term) -> Term {
    Term::ActivateOwner(Rc::new(owner), Rc::new(body))
}
fn tuple(xs: impl IntoIterator<Item = Term>) -> Term {
    Term::Tuple(xs.into_iter().collect())
}
fn run(term: Term) -> Term {
    let p = Program::default();
    match Machine::new(&p, 10000).run(term) {
        Outcome::Value(v) => v,
        other => panic!("{other:?}"),
    }
}
#[test]
fn absence_and_nested_brackets_restore_without_changing_default_owner() {
    let result = run(scope(
        "outer",
        tuple([
            Term::ReadOwnerEvidence,
            scope(
                "inner",
                evidence(
                    Term::Own(0),
                    tuple([Term::ReadOwnerEvidence, Term::DefaultOwner]),
                ),
            ),
            evidence(
                Term::unit(),
                tuple([Term::ReadOwnerEvidence, Term::DefaultOwner]),
            ),
            Term::ReadOwnerEvidence,
        ]),
    ));
    assert_eq!(
        format!("{result:?}"),
        "Tuple([OwnerCandidate(Some(0)), Tuple([OwnerCandidate(Some(0)), Own(1)]), Tuple([OwnerCandidate(None), Own(0)]), OwnerCandidate(Some(0))])"
    );
    assert!(matches!(
        run(Term::ReadOwnerEvidence),
        Term::OwnerCandidate(None)
    ));
}
#[test]
fn fresh_scope_overrides_evidence_and_raw_activation_changes_only_default() {
    let result = run(scope(
        "outer",
        scope(
            "inner",
            activate(
                Term::Own(0),
                tuple([
                    Term::ReadOwnerEvidence,
                    Term::DefaultOwner,
                    scope(
                        "fresh",
                        tuple([Term::ReadOwnerEvidence, Term::DefaultOwner]),
                    ),
                    Term::ReadOwnerEvidence,
                ]),
            ),
        ),
    ));
    assert_eq!(
        format!("{result:?}"),
        "Tuple([OwnerCandidate(Some(1)), Own(0), Tuple([OwnerCandidate(Some(2)), Own(2)]), OwnerCandidate(Some(1))])"
    );
}
#[test]
fn error_unwind_restores_evidence() {
    let caught = Term::Try(
        Rc::new(evidence(Term::unit(), Term::Raise(Rc::new(Term::unit())))),
        Rc::from([Arm {
            pat: Pat::Wild,
            guard: None,
            body: Term::ReadOwnerEvidence,
        }]),
    );
    assert!(matches!(
        run(scope("outer", caught)),
        Term::OwnerCandidate(Some(0))
    ));
}
#[test]
fn closed_evidence_is_inert_but_raw_activation_rejects_it() {
    let closed = scope("closed", Term::ReadOwnerEvidence);
    let term = Term::Let(
        "owner".into(),
        None,
        Rc::new(closed.clone()),
        Rc::new(evidence(Term::Var("owner".into()), Term::ReadOwnerEvidence)),
    );
    assert!(matches!(run(term), Term::OwnerCandidate(Some(0))));
    let p = Program::default();
    let term = Term::Let(
        "owner".into(),
        None,
        Rc::new(closed),
        Rc::new(activate(Term::Var("owner".into()), Term::unit())),
    );
    assert!(
        matches!(Machine::new(&p, 10000).run(term), Outcome::Stuck(s) if s.contains("closed region"))
    );
}

#[test]
fn handlers_use_performer_evidence_across_prompt_transfer() {
    use flick_model::handler::{Handled, Handler, Op, OpClass};
    use flick_model::syntax::{Const, Path};
    for class in [OpClass::Tail, OpClass::Abort, OpClass::Control] {
        let effect = Path::new("Probe", "Probe");
        let perform = Term::app(
            Term::konst(Const::Perform {
                id: 1,
                effect: effect.clone(),
                op: "read".into(),
                class,
                arity: 1,
            }),
            Term::unit(),
        );
        let result = if class == OpClass::Control {
            Term::lam("resume", Term::ReadOwnerEvidence)
        } else {
            Term::ReadOwnerEvidence
        };
        let handler = Term::Handler(Rc::new(Handler {
            effects: vec![Handled {
                effect,
                ops: vec![Op {
                    name: "read".into(),
                    class,
                    dicts: 0,
                    arity: 1,
                    covers: true,
                    function: Term::lam("_", result),
                }],
            }],
            body: evidence(Term::unit(), perform),
            ret: None,
            errors: Rc::from([]),
        }));
        assert!(
            matches!(run(scope("outer", handler)), Term::OwnerCandidate(None)),
            "{class:?}"
        );
    }
}

#[test]
fn provider_context_preserves_candidate_and_task_data_is_not_run() {
    use flick_model::syntax::{Const, Path};
    let layer = Term::app(
        Term::konst(Const::LayerSucceed(Path::new("Probe", "Probe"))),
        Term::unit(),
    );
    let term = scope(
        "outer",
        evidence(
            Term::unit(),
            Term::Provide(Rc::new(layer), Rc::new(Term::ReadOwnerEvidence)),
        ),
    );
    assert!(matches!(run(term), Term::OwnerCandidate(None)));
    let lazy = evidence(
        Term::Own(999),
        Term::Task(Rc::new(Term::Die("must stay data".into()))),
    );
    assert!(matches!(run(lazy), Term::Task(_)));
}

#[test]
fn source_list_stored_callback_preserves_lexical_owner() {
    use flick_model::run;
    let modules = run::compile_source(
        r#"
module App.Main exposing (main)
service Read =
  read : Unit ->{} String
layer : Layer none none {Read}
let layer = Layer.succeed Read { read = fun () -> "unused" }
acquire : Unit ->{Scope s} Arena.Value s String
let acquire () = Arena.alloc (Arena.make ()) "owned"
callbacks : List (Unit ->{Read, Scope s} Arena.Value s String)
let callbacks = [acquire]
under_list : List (Unit ->{Read | r; e} a) ->{r; e} Option a
let under_list callbacks =
  match callbacks with
  | [] -> None
  | [body, ..._] -> Some (Layer.run layer body)
main : Unit ->{Host.Console} Unit
let main () = scope {
  match under_list callbacks with
  | None -> Platform.stdout_line "missing"
  | Some value -> Platform.stdout_line (Arena.get value)
}
"#,
    )
    .expect("compile source List callback");
    let program = run::program(&modules);
    let main = run::main_of(&modules).expect("main");
    let result = run::run_main(&program, &main, 1_000_000);
    assert!(
        matches!(result.outcome, Outcome::Value(_)),
        "{:?}",
        result.outcome
    );
    assert_eq!(result.stdout, "owned\n");
}

#[test]
fn checker_validates_context_operand_and_preserves_body_type() {
    use flick_model::{check::Checker, decls::Decls, types::Type};
    let decls = Decls::default();
    let valid = evidence(Term::ReadOwnerEvidence, Term::string("result"));
    let (ty, _) = Checker::new(&decls)
        .infer_top(&valid)
        .expect("internal evidence carrier");
    assert_eq!(ty, Type::con("Prelude", "String"));
    assert!(
        Checker::new(&decls)
            .infer_top(&evidence(Term::string("not-owner"), Term::unit()))
            .is_err()
    );
    assert!(
        Checker::new(&decls)
            .infer_top(&activate(Term::unit(), Term::unit()))
            .is_err()
    );
}

#[test]
fn finalizers_keep_registration_evidence() {
    use flick_model::syntax::{Const, Path, RegionOp};
    // Capture a finalizer's C in a reference, then inspect it after scope close.
    let set = Term::apps(
        Term::konst(Const::RefSet),
        [Term::Var("saved".into()), Term::ReadOwnerEvidence],
    );
    let add = Term::app(
        Term::konst(Const::Region(
            Path::new("Scope", "add_finalizer"),
            RegionOp::AddFinalizer,
        )),
        Term::Task(Rc::new(set)),
    );
    let get = Term::app(Term::konst(Const::RefGet), Term::Var("saved".into()));
    let term = Term::Let(
        "saved".into(),
        None,
        Rc::new(Term::app(
            Term::konst(Const::RefMake),
            Term::string("unset"),
        )),
        Rc::new(Term::Let(
            "_".into(),
            None,
            Rc::new(scope("owner", evidence(Term::unit(), add))),
            Rc::new(get),
        )),
    );
    assert!(matches!(run(term), Term::OwnerCandidate(None)));
}

#[test]
fn operational_read_preserves_configuration_typing() {
    use flick_model::{check::check_config, decls::Decls, types::Type};
    let snapshot = Term::Let(
        "candidate".into(),
        None,
        Rc::new(Term::ReadOwnerEvidence),
        Rc::new(evidence(
            Term::Var("candidate".into()),
            Term::Let(
                "_".into(),
                None,
                Rc::new(Term::ReadOwnerEvidence),
                Rc::new(Term::unit()),
            ),
        )),
    );
    let program = Program::default();
    let decls = Decls::default();
    let mut steps = 0;
    let result = Machine::new(&program, 10000).run_watched(
        scope("owner", snapshot),
        &mut |mem, lambda, term| {
            steps += 1;
            check_config(&decls, mem, lambda, term, &Type::con("Prelude", "Unit"))
                .unwrap_or_else(|error| panic!("step {steps}: {error}: {term:?}"));
        },
    );
    assert!(matches!(result, Outcome::Value(_)));
    assert!(steps > 5);
}

#[test]
fn source_foreign_alias_keeps_declared_scheme_and_raw_activation() {
    use flick_model::run;
    let modules = run::compile_source(
        r#"
module App.Main exposing (main)
main : Unit ->{Host.Console} Unit
let main () = scope {
  let make = Arena.make
  let arena = make ()
  let value = Arena.alloc arena "named"
  Platform.stdout_line (Arena.get value)
}
"#,
    )
    .expect("compile raw foreign alias");
    let program = run::program(&modules);
    let errors = run::check(&modules, &program);
    assert!(errors.is_empty(), "{errors:?}");
    let result = run::run_main(&program, &run::main_of(&modules).expect("main"), 1_000_000);
    assert!(
        matches!(result.outcome, Outcome::Value(_)),
        "{:?}",
        result.outcome
    );
    assert_eq!(result.stdout, "named\n");
}

#[test]
fn terminal_defects_and_fuel_exhaustion_do_not_leave_context_installed() {
    let program = Program::default();
    let mut machine = Machine::new(&program, 100);
    assert!(matches!(
        machine.run(evidence(Term::Own(77), Term::Die("defect".into()))),
        Outcome::Die(_)
    ));
    assert!(matches!(
        machine.run(Term::ReadOwnerEvidence),
        Outcome::Value(Term::OwnerCandidate(None))
    ));
    let mut machine = Machine::new(&program, 2);
    assert!(matches!(
        machine.run(evidence(Term::Own(77), Term::ReadOwnerEvidence)),
        Outcome::OutOfFuel
    ));
    // Fuel remains exhausted, but the observable configuration of a new run
    // must start outside the abandoned bracket.
    let mut first = None;
    machine.run_watched(Term::ReadOwnerEvidence, &mut |_, _, term| {
        first = Some(format!("{term:?}"));
    });
    assert_eq!(first.as_deref(), Some("ReadOwnerEvidence"));
}
