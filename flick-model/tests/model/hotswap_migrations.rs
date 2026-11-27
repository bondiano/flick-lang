//! Unit tests and proptest model properties for atomic state-cell
//! migrations and generated hot-swap sessions (`docs/formal/hot-swap.md`).

use std::rc::Rc;

use flick_core::numeric::{Arith, Num};
use flick_model::hotswap::{
    ActionObservation, CellMigration, GlobalDecl, LiveSession, MigrationBatch, MigrationKind,
    MigrationOutcome, MigrationRollbackReason, ModuleDef, SessionAction, SessionOutcomeSummary,
    StateCellDecl, SwapError, SwapKind, SwapRequest, generate_session, render_term,
    run_session_oracle, summarize_outcome,
};
use flick_model::syntax::{Arm, Const, Lit, Pat, Path, Prim, Term};
use flick_model::types::{Kind, Scheme, Stage, Type, Var};
use proptest::collection::vec;
use proptest::prelude::*;

use crate::props::GEN;

fn hot(name: &str) -> Path {
    Path::new("Hot", name)
}

fn int_ty() -> Type {
    Type::con("Prelude", "Int")
}

fn bool_ty() -> Type {
    Type::con("Prelude", "Bool")
}

fn string_ty() -> Type {
    Type::con("Prelude", "String")
}

fn unit_ty() -> Type {
    Type::con("Prelude", "Unit")
}

fn pair_ty(a: Type, b: Type) -> Type {
    Type::Tuple(Rc::from([a, b]))
}

fn mono(body: Type) -> Scheme {
    Scheme::new(Vec::new(), body)
}

fn add_int(a: Term, b: Term) -> Term {
    Term::app(
        Term::app(
            Term::konst(Const::Prim(Prim::Arith(Arith::Add, Num::Int))),
            a,
        ),
        b,
    )
}

fn setup_counter_session(session: &mut LiveSession) {
    session
        .load_module(ModuleDef {
            name: "Hot".into(),
            globals: Vec::new(),
            state_cells: vec![
                StateCellDecl {
                    path: hot("counter"),
                    ty: int_ty(),
                    init: Term::Lit(Lit::Int(7)),
                },
                StateCellDecl {
                    path: hot("tag"),
                    ty: string_ty(),
                    init: Term::Lit(Lit::String("init".into())),
                },
            ],
        })
        .expect("module loads");

    let entry = Term::Let(
        "pad".into(),
        None,
        Rc::new(add_int(Term::Lit(Lit::Int(1)), Term::Lit(Lit::Int(2)))),
        Rc::new(add_int(
            Term::Var("pad".into()),
            Term::app(Term::konst(Const::RefGet), Term::Loc(0)),
        )),
    );
    session.start_entry(entry, int_ty()).expect("entry types");
    session.step_n(2, true).expect("pauses mid-execution");
}

#[test]
fn failing_and_diverging_migrations_roll_back_atomically() {
    GEN.with(|(decls, program)| {
        let mut session = LiveSession::new(decls.clone(), program.clone(), 10_000);
        setup_counter_session(&mut session);

        let snap_before_panic = session.snapshot();
        let panic_out = session.migrate(&MigrationBatch {
            migrations: vec![
                CellMigration {
                    cell: hot("counter"),
                    old_ty: int_ty(),
                    new_ty: int_ty(),
                    migrate_fn: Term::lam(
                        "n",
                        add_int(Term::Var("n".into()), Term::Lit(Lit::Int(1))),
                    ),
                },
                CellMigration {
                    cell: hot("tag"),
                    old_ty: string_ty(),
                    new_ty: string_ty(),
                    migrate_fn: Term::lam("_s", Term::Die("boom".into())),
                },
            ],
            fuel: 100,
        });
        assert_eq!(
            panic_out,
            MigrationOutcome::RolledBack {
                reason: MigrationRollbackReason::RuntimeDie("boom".into())
            }
        );
        assert_eq!(session.snapshot(), snap_before_panic);

        let snap_before_spin = session.snapshot();
        let spin_scheme = Scheme::new(Vec::new(), Type::fun(unit_ty(), Stage::Pure, bool_ty()));
        let spin_fn = Term::lam(
            "_n",
            Term::LetRec(
                "spin".into(),
                Some(Rc::new(spin_scheme)),
                Rc::new(Term::lam(
                    "u",
                    Term::app(Term::Var("spin".into()), Term::Var("u".into())),
                )),
                Rc::new(Term::app(Term::Var("spin".into()), Term::unit())),
            ),
        );
        let spin_out = session.migrate(&MigrationBatch {
            migrations: vec![CellMigration {
                cell: hot("counter"),
                old_ty: int_ty(),
                new_ty: bool_ty(),
                migrate_fn: spin_fn,
            }],
            fuel: 50,
        });
        assert_eq!(
            spin_out,
            MigrationOutcome::RolledBack {
                reason: MigrationRollbackReason::Diverged
            }
        );
        assert_eq!(session.snapshot(), snap_before_spin);
    });
}

#[test]
fn type_changing_migration_preserves_old_frames_and_exposes_new_state() {
    GEN.with(|(decls, program)| {
        let mut session = LiveSession::new(decls.clone(), program.clone(), 10_000);
        setup_counter_session(&mut session);

        let ok_out = session.migrate(&MigrationBatch {
            migrations: vec![CellMigration {
                cell: hot("counter"),
                old_ty: int_ty(),
                new_ty: pair_ty(int_ty(), bool_ty()),
                migrate_fn: Term::lam(
                    "n",
                    Term::Tuple(Rc::from([
                        add_int(Term::Var("n".into()), Term::Lit(Lit::Int(10))),
                        Term::Lit(Lit::Bool(true)),
                    ])),
                ),
            }],
            fuel: 200,
        });
        let MigrationOutcome::Committed { migrated } = ok_out else {
            panic!("expected committed migration, got {ok_out:?}");
        };
        assert_eq!(migrated.len(), 1);
        assert_eq!(migrated[0].old_loc, 0);
        assert_eq!(migrated[0].new_loc, 2);
        assert_eq!(render_term(&session.mem()[0]), "7");
        assert_eq!(render_term(&session.mem()[2]), "(17, true)");

        session
            .check_configuration()
            .expect("old frame stays typed after type-changing migration");
        let outcome = session
            .run_to_end(true)
            .expect("old frame finishes with preservation")
            .expect("entry returns value");
        assert_eq!(
            summarize_outcome(&outcome),
            SessionOutcomeSummary::Value("10".into())
        );

        let state_read = session
            .read_state(&hot("counter"), 500)
            .expect("state reads");
        assert_eq!(
            summarize_outcome(&state_read),
            SessionOutcomeSummary::Value("(17, true)".into())
        );
    });
}

#[test]
fn rejected_swap_leaves_session_unchanged() {
    GEN.with(|(decls, program)| {
        let mut session = LiveSession::new(decls.clone(), program.clone(), 5_000);
        let poly_id = Scheme::new(
            vec![("a".into(), Kind::Type)],
            Type::fun(
                Type::Var(Var::Named("a".into())),
                Stage::Pure,
                Type::Var(Var::Named("a".into())),
            ),
        );
        session
            .load_module(ModuleDef {
                name: "Hot".into(),
                globals: vec![GlobalDecl {
                    path: hot("id"),
                    scheme: poly_id,
                    value: Term::lam("x", Term::Var("x".into())),
                }],
                state_cells: Vec::new(),
            })
            .expect("loads");
        let before = session.snapshot();
        let err = session
            .swap(SwapRequest {
                target: hot("id"),
                new_scheme: mono(Type::fun(int_ty(), Stage::Pure, int_ty())),
                new_value: Term::lam("x", add_int(Term::Var("x".into()), Term::Lit(Lit::Int(1)))),
            })
            .expect_err("narrowing swap must fail");
        assert!(matches!(err, SwapError::Incompatible(_)));
        assert_eq!(session.snapshot(), before);
    });
}

#[test]
fn migration_rejects_impure_function_at_type_check() {
    GEN.with(|(decls, program)| {
        let mut session = LiveSession::new(decls.clone(), program.clone(), 5_000);
        session
            .load_module(ModuleDef {
                name: "Hot".into(),
                globals: Vec::new(),
                state_cells: vec![StateCellDecl {
                    path: hot("counter"),
                    ty: int_ty(),
                    init: Term::Lit(Lit::Int(1)),
                }],
            })
            .expect("loads");
        let before = session.snapshot();
        // Calling `Ref.make` inside a migration function is runtime, not pure!
        let impure = Term::lam(
            "n",
            Term::Let(
                "_r".into(),
                None,
                Rc::new(Term::app(
                    Term::konst(Const::RefMake),
                    Term::Var("n".into()),
                )),
                Rc::new(Term::Var("n".into())),
            ),
        );
        let out = session.migrate(&MigrationBatch {
            migrations: vec![CellMigration {
                cell: hot("counter"),
                old_ty: int_ty(),
                new_ty: int_ty(),
                migrate_fn: impure,
            }],
            fuel: 100,
        });
        assert!(matches!(
            out,
            MigrationOutcome::RolledBack {
                reason: MigrationRollbackReason::TypeError(_)
            }
        ));
        assert_eq!(session.snapshot(), before);
    });
}

#[test]
fn deterministic_oracle_covers_all_swap_and_migration_kinds() {
    GEN.with(|(decls, program)| {
        // Explicit choice vector hitting all 8 SwapKinds and all 4 MigrationKinds.
        let mut saw_generalize = false;
        let mut saw_narrow_reject = false;
        let mut saw_widen_effect_reject = false;
        let mut saw_narrow_cap_reject = false;
        let mut saw_widen_ret_cap_reject = false;
        let mut saw_valid_mig = false;
        let mut saw_panic_mig = false;
        let mut saw_diverging_mig = false;
        let mut saw_partial_mig = false;

        for seed in 0u32..48 {
            let choices: Vec<u32> = (0u32..40)
                .map(|i| seed.wrapping_mul(31).wrapping_add(i.wrapping_mul(7)))
                .collect();
            let scenario = generate_session(&choices);
            let report = run_session_oracle(decls, program, &scenario)
                .unwrap_or_else(|e| panic!("seed {seed} failed oracle: {e}"));
            assert!(
                matches!(report.final_outcome, SessionOutcomeSummary::Value(_)),
                "seed {seed}: unexpected outcome {:?}",
                report.final_outcome
            );
            for obs in &report.observations {
                match obs {
                    ActionObservation::SwapAccepted {
                        kind: SwapKind::GeneralizeScheme,
                        ..
                    } => saw_generalize = true,
                    ActionObservation::SwapRejected {
                        kind: SwapKind::RejectNarrowScheme,
                        ..
                    } => saw_narrow_reject = true,
                    ActionObservation::SwapRejected {
                        kind: SwapKind::RejectWidenEffect,
                        ..
                    } => saw_widen_effect_reject = true,
                    ActionObservation::SwapRejected {
                        kind: SwapKind::RejectNarrowParamCapture,
                        ..
                    } => saw_narrow_cap_reject = true,
                    ActionObservation::SwapRejected {
                        kind: SwapKind::RejectWidenCallableCapture,
                        ..
                    } => saw_widen_ret_cap_reject = true,
                    ActionObservation::MigrationCommitted {
                        kind: MigrationKind::Valid,
                        ..
                    } => saw_valid_mig = true,
                    ActionObservation::MigrationRolledBack {
                        kind: MigrationKind::FailingDie,
                        ..
                    } => saw_panic_mig = true,
                    ActionObservation::MigrationRolledBack {
                        kind: MigrationKind::Diverging,
                        ..
                    } => saw_diverging_mig = true,
                    ActionObservation::MigrationRolledBack {
                        kind: MigrationKind::PartialBatchFailure,
                        ..
                    } => saw_partial_mig = true,
                    _ => {}
                }
            }
        }
        assert!(saw_generalize, "must cover GeneralizeScheme");
        assert!(saw_narrow_reject, "must cover RejectNarrowScheme");
        assert!(saw_widen_effect_reject, "must cover RejectWidenEffect");
        assert!(saw_narrow_cap_reject, "must cover RejectNarrowParamCapture");
        assert!(
            saw_widen_ret_cap_reject,
            "must cover RejectWidenCallableCapture"
        );
        assert!(saw_valid_mig, "must cover Valid migration");
        assert!(saw_panic_mig, "must cover FailingDie migration");
        assert!(saw_diverging_mig, "must cover Diverging migration");
        assert!(saw_partial_mig, "must cover PartialBatchFailure migration");
    });
}

#[test]
fn pair_pattern_migration_works_in_session() {
    GEN.with(|(decls, program)| {
        let mut session = LiveSession::new(decls.clone(), program.clone(), 5_000);
        session
            .load_module(ModuleDef {
                name: "Hot".into(),
                globals: Vec::new(),
                state_cells: vec![StateCellDecl {
                    path: hot("p"),
                    ty: pair_ty(int_ty(), bool_ty()),
                    init: Term::Tuple(Rc::from([
                        Term::Lit(Lit::Int(9)),
                        Term::Lit(Lit::Bool(false)),
                    ])),
                }],
            })
            .expect("loads");
        let out = session.migrate(&MigrationBatch {
            migrations: vec![CellMigration {
                cell: hot("p"),
                old_ty: pair_ty(int_ty(), bool_ty()),
                new_ty: int_ty(),
                migrate_fn: Term::lam(
                    "p",
                    Term::Match(
                        Rc::new(Term::Var("p".into())),
                        Rc::from([Arm {
                            pat: Pat::Tuple(Rc::from([Pat::Var("n".into()), Pat::Wild])),
                            guard: None,
                            body: add_int(Term::Var("n".into()), Term::Lit(Lit::Int(1))),
                        }]),
                    ),
                ),
            }],
            fuel: 200,
        });
        assert!(matches!(out, MigrationOutcome::Committed { .. }));
        let read = session.read_state(&hot("p"), 500).expect("reads");
        assert_eq!(
            summarize_outcome(&read),
            SessionOutcomeSummary::Value("10".into())
        );
    });
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    #[ignore = "a model property; run `just model`"]
    fn hot_swap_sessions_preserve_typing_progress_and_atomicity(
        choices in vec(any::<u32>(), 0..96)
    ) {
        let scenario = generate_session(&choices);
        let result = GEN.with(|(decls, program)| run_session_oracle(decls, program, &scenario));
        if let Err(err) = result {
            return Err(TestCaseError::fail(format!(
                "hot-swap session property failed: {err}\nactions: {:?}",
                scenario.actions.iter().map(|a| match a {
                    SessionAction::Step { steps } => format!("Step({steps})"),
                    SessionAction::LoadModule(m) => format!("LoadModule({})", m.name),
                    SessionAction::Swap { request, kind } => format!("Swap({}, {kind:?})", request.target),
                    SessionAction::Migrate { kind, .. } => format!("Migrate({kind:?})"),
                    SessionAction::Call { .. } => "Call".into(),
                    SessionAction::ReadState { cell } => format!("ReadState({cell})"),
                }).collect::<Vec<_>>()
            )));
        }
        let report = result.expect("checked above");
        prop_assert!(
            matches!(report.final_outcome, SessionOutcomeSummary::Value(_)),
            "expected value outcome, got {:?}",
            report.final_outcome
        );
    }
}
