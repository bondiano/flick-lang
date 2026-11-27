//! Deterministic live-session generator for hot swaps and atomic
//! state-cell migrations.

use std::rc::Rc;

use super::builders::{
    Cursor, FIXTURE_MOD, HOT_MOD, add_const_fn, add_int, bool_ty, diverging_fn, dying_fn, hot,
    int_ty, mono, owner_ty, pair_ty, poly_id_scheme, string_ty, thunk_record_ty, thunk_record_val,
    unit_thunk_ty, unit_ty,
};
use super::scenario::{MigrationKind, SessionAction, SessionScenario, SwapKind};
use super::types::{
    CellMigration, GlobalDecl, MigrationBatch, ModuleDef, StateCellDecl, SwapRequest,
};
use crate::syntax::{Arm, Const, Lit, Pat, Path, Term};
use crate::types::{Caps, Kind, Row, Scheme, Stage, Type, Var};

#[derive(Clone, Copy, PartialEq, Eq)]
enum CounterRep {
    Int,
    Pair,
}

impl CounterRep {
    fn ty(self) -> Type {
        match self {
            Self::Int => int_ty(),
            Self::Pair => pair_ty(int_ty(), bool_ty()),
        }
    }
}

struct GenState {
    compute_generalized: bool,
    counter_rep: CounterRep,
}

fn driver_body() -> Term {
    Term::lam(
        "x",
        Term::Let(
            "captured".into(),
            None,
            Rc::new(Term::Global(hot("compute"))),
            Rc::new(Term::Let(
                "first".into(),
                None,
                Rc::new(Term::app(
                    Term::lam(
                        "y",
                        Term::app(
                            Term::Var("captured".into()),
                            add_int(Term::Var("y".into()), Term::Lit(Lit::Int(1))),
                        ),
                    ),
                    Term::Var("x".into()),
                )),
                Rc::new(Term::Let(
                    "second".into(),
                    None,
                    Rc::new(Term::app(
                        Term::Global(hot("compute")),
                        Term::Var("x".into()),
                    )),
                    Rc::new(Term::Tuple(Rc::from([
                        Term::Var("first".into()),
                        Term::Var("second".into()),
                    ]))),
                )),
            )),
        ),
    )
}

fn build_initial_module(init_delta: i32, init_counter: i32) -> ModuleDef {
    let int_to_int = Type::fun(int_ty(), Stage::Pure, int_ty());
    let log_row = Row::one(Path::new(FIXTURE_MOD, "Log"));
    let oops_row = Row::one(Path::new(FIXTURE_MOD, "Oops"));

    ModuleDef {
        name: HOT_MOD.into(),
        globals: vec![
            GlobalDecl {
                path: hot("compute"),
                scheme: mono(int_to_int.clone()),
                value: add_const_fn(init_delta),
            },
            GlobalDecl {
                path: hot("poly_id"),
                scheme: poly_id_scheme(),
                value: Term::lam("x", Term::Var("x".into())),
            },
            GlobalDecl {
                path: hot("effectful"),
                scheme: mono(Type::fun(
                    int_ty(),
                    Stage::Runtime(log_row, oops_row),
                    int_ty(),
                )),
                value: add_const_fn(1),
            },
            GlobalDecl {
                path: hot("pure_fn"),
                scheme: mono(int_to_int),
                value: add_const_fn(2),
            },
            GlobalDecl {
                path: hot("with_cb"),
                scheme: Scheme::new(
                    vec![("s".into(), Kind::Region)],
                    Type::fun(
                        unit_thunk_ty(Caps::atom(Var::Named("s".into()))),
                        Stage::Runtime(Row::none(), Row::none()),
                        unit_ty(),
                    ),
                ),
                value: Term::lam("f", Term::app(Term::Var("f".into()), Term::unit())),
            },
            GlobalDecl {
                path: hot("plain_cb"),
                scheme: mono(Type::fun(
                    unit_thunk_ty(Caps::default()),
                    Stage::Runtime(Row::none(), Row::none()),
                    unit_ty(),
                )),
                value: Term::lam("f", Term::app(Term::Var("f".into()), Term::unit())),
            },
            GlobalDecl {
                path: hot("mk_thunk"),
                scheme: Scheme::new(
                    vec![("s".into(), Kind::Region)],
                    Type::fun(owner_ty("s"), Stage::Pure, thunk_record_ty(Caps::default())),
                ),
                value: thunk_record_val(),
            },
            GlobalDecl {
                path: hot("driver"),
                scheme: mono(Type::fun(
                    int_ty(),
                    Stage::Pure,
                    pair_ty(int_ty(), int_ty()),
                )),
                value: driver_body(),
            },
        ],
        state_cells: vec![
            StateCellDecl {
                path: hot("counter"),
                ty: int_ty(),
                init: Term::Lit(Lit::Int(init_counter)),
            },
            StateCellDecl {
                path: hot("label"),
                ty: string_ty(),
                init: Term::Lit(Lit::String("v0".into())),
            },
        ],
    }
}

fn generate_rejected_swap(cur: &mut Cursor<'_>, choice: usize) -> SessionAction {
    let int_to_int = Type::fun(int_ty(), Stage::Pure, int_ty());
    match choice {
        4 => SessionAction::Swap {
            request: SwapRequest {
                target: hot("poly_id"),
                new_scheme: mono(int_to_int),
                new_value: add_const_fn(1),
            },
            kind: SwapKind::RejectNarrowScheme,
        },
        5 => {
            let widened_stage = if cur.pick(2) == 0 {
                Stage::Runtime(Row::one(Path::new(FIXTURE_MOD, "Log")), Row::none())
            } else {
                Stage::Runtime(Row::none(), Row::one(Path::new(FIXTURE_MOD, "Oops")))
            };
            SessionAction::Swap {
                request: SwapRequest {
                    target: hot("pure_fn"),
                    new_scheme: mono(Type::fun(int_ty(), widened_stage, int_ty())),
                    new_value: add_const_fn(5),
                },
                kind: SwapKind::RejectWidenEffect,
            }
        }
        6 => SessionAction::Swap {
            request: SwapRequest {
                target: hot("with_cb"),
                new_scheme: mono(Type::fun(
                    unit_thunk_ty(Caps::default()),
                    Stage::Runtime(Row::none(), Row::none()),
                    unit_ty(),
                )),
                new_value: Term::lam("f", Term::app(Term::Var("f".into()), Term::unit())),
            },
            kind: SwapKind::RejectNarrowParamCapture,
        },
        _ => {
            let widened_ret = Scheme::new(
                vec![("s".into(), Kind::Region)],
                Type::fun(
                    owner_ty("s"),
                    Stage::Pure,
                    thunk_record_ty(Caps::atom(Var::Named("s".into()))),
                ),
            );
            SessionAction::Swap {
                request: SwapRequest {
                    target: hot("mk_thunk"),
                    new_scheme: widened_ret,
                    new_value: thunk_record_val(),
                },
                kind: SwapKind::RejectWidenCallableCapture,
            }
        }
    }
}

fn generate_swap_action(
    cur: &mut Cursor<'_>,
    state: &mut GenState,
    choice: usize,
) -> SessionAction {
    let int_to_int = Type::fun(int_ty(), Stage::Pure, int_ty());
    match choice {
        0 => {
            let new_delta = i32::try_from(cur.pick(15)).unwrap_or(3) + 10;
            let (new_scheme, new_value) = if state.compute_generalized {
                (poly_id_scheme(), Term::lam("x", Term::Var("x".into())))
            } else {
                (mono(int_to_int), add_const_fn(new_delta))
            };
            SessionAction::Swap {
                request: SwapRequest {
                    target: hot("compute"),
                    new_scheme,
                    new_value,
                },
                kind: SwapKind::SameScheme,
            }
        }
        1 => {
            state.compute_generalized = true;
            SessionAction::Swap {
                request: SwapRequest {
                    target: hot("compute"),
                    new_scheme: poly_id_scheme(),
                    new_value: Term::lam("x", Term::Var("x".into())),
                },
                kind: SwapKind::GeneralizeScheme,
            }
        }
        2 => {
            let delta = i32::try_from(cur.pick(9)).unwrap_or(2) + 20;
            SessionAction::Swap {
                request: SwapRequest {
                    target: hot("effectful"),
                    new_scheme: mono(int_to_int),
                    new_value: add_const_fn(delta),
                },
                kind: SwapKind::NarrowEffect,
            }
        }
        3 => {
            let widened = Scheme::new(
                vec![("c".into(), Kind::Captures)],
                Type::fun(
                    unit_thunk_ty(Caps::var(Var::Named("c".into()))),
                    Stage::Runtime(Row::none(), Row::none()),
                    unit_ty(),
                ),
            );
            SessionAction::Swap {
                request: SwapRequest {
                    target: hot("plain_cb"),
                    new_scheme: widened,
                    new_value: Term::lam("f", Term::app(Term::Var("f".into()), Term::unit())),
                },
                kind: SwapKind::WidenParamCapture,
            }
        }
        _ => generate_rejected_swap(cur, choice),
    }
}

fn generate_valid_migration(cur: &mut Cursor<'_>, state: &mut GenState) -> SessionAction {
    let change_ty = cur.pick(2) == 0;
    let (old_ty, new_ty, migrate_fn, next_rep) = match (state.counter_rep, change_ty) {
        (CounterRep::Int, true) => (
            int_ty(),
            pair_ty(int_ty(), bool_ty()),
            Term::lam(
                "n",
                Term::Tuple(Rc::from([
                    add_int(Term::Var("n".into()), Term::Lit(Lit::Int(1))),
                    Term::Lit(Lit::Bool(true)),
                ])),
            ),
            CounterRep::Pair,
        ),
        (CounterRep::Int, false) => (int_ty(), int_ty(), add_const_fn(3), CounterRep::Int),
        (CounterRep::Pair, _) => (
            pair_ty(int_ty(), bool_ty()),
            int_ty(),
            Term::lam(
                "p",
                Term::Match(
                    Rc::new(Term::Var("p".into())),
                    Rc::from([Arm {
                        pat: Pat::Tuple(Rc::from([Pat::Var("n".into()), Pat::Wild])),
                        guard: None,
                        body: add_int(Term::Var("n".into()), Term::Lit(Lit::Int(5))),
                    }]),
                ),
            ),
            CounterRep::Int,
        ),
    };
    state.counter_rep = next_rep;
    SessionAction::Migrate {
        batch: MigrationBatch {
            migrations: vec![CellMigration {
                cell: hot("counter"),
                old_ty,
                new_ty,
                migrate_fn,
            }],
            fuel: 500,
        },
        kind: MigrationKind::Valid,
    }
}

fn generate_migration_action(
    cur: &mut Cursor<'_>,
    state: &mut GenState,
    choice: usize,
) -> SessionAction {
    match choice {
        8 => generate_valid_migration(cur, state),
        9 => SessionAction::Migrate {
            batch: MigrationBatch {
                migrations: vec![CellMigration {
                    cell: hot("counter"),
                    old_ty: state.counter_rep.ty(),
                    new_ty: int_ty(),
                    migrate_fn: dying_fn("migration invariant failed"),
                }],
                fuel: 32,
            },
            kind: MigrationKind::FailingDie,
        },
        10 => SessionAction::Migrate {
            batch: MigrationBatch {
                migrations: vec![CellMigration {
                    cell: hot("counter"),
                    old_ty: state.counter_rep.ty(),
                    new_ty: int_ty(),
                    migrate_fn: diverging_fn(&int_ty()),
                }],
                fuel: 32,
            },
            kind: MigrationKind::Diverging,
        },
        _ => {
            let old_ty = state.counter_rep.ty();
            let first_fn = match state.counter_rep {
                CounterRep::Int => add_const_fn(100),
                CounterRep::Pair => Term::lam("p", Term::Var("p".into())),
            };
            let second_fn = if cur.pick(2) == 0 {
                dying_fn("second cell failed")
            } else {
                diverging_fn(&string_ty())
            };
            SessionAction::Migrate {
                batch: MigrationBatch {
                    migrations: vec![
                        CellMigration {
                            cell: hot("counter"),
                            old_ty: old_ty.clone(),
                            new_ty: old_ty,
                            migrate_fn: first_fn,
                        },
                        CellMigration {
                            cell: hot("label"),
                            old_ty: string_ty(),
                            new_ty: string_ty(),
                            migrate_fn: second_fn,
                        },
                    ],
                    fuel: 32,
                },
                kind: MigrationKind::PartialBatchFailure,
            }
        }
    }
}

/// Deterministically generates a [`SessionScenario`] from `choices`, covering:
/// - suspended old frames & captured closures executing old code while new
///   calls hit swapped code;
/// - generalizing swaps, effect-narrowing swaps, and parameter-capture-widening
///   swaps;
/// - rejected narrowing swaps, effect-widening swaps, parameter-capture-narrowing
///   swaps, and callable-capture-widening swaps;
/// - valid same-type and type-changing migrations, failing (`Die`) migrations,
///   diverging (fuel-exhausted) migrations, and partial-batch rollback.
#[must_use]
pub fn generate_session(choices: &[u32]) -> SessionScenario {
    let mut cur = Cursor::new(choices);
    let init_delta = i32::try_from(cur.pick(7)).unwrap_or(1) + 1;
    let init_counter = i32::try_from(cur.pick(11)).unwrap_or(0);
    let entry_arg = i32::try_from(cur.pick(9)).unwrap_or(1);

    let initial_module = build_initial_module(init_delta, init_counter);
    let entry = Term::Let(
        "pair".into(),
        None,
        Rc::new(Term::app(
            Term::Global(hot("driver")),
            Term::Lit(Lit::Int(entry_arg)),
        )),
        Rc::new(Term::Let(
            "old_state".into(),
            None,
            Rc::new(Term::app(Term::konst(Const::RefGet), Term::Loc(0))),
            Rc::new(Term::Tuple(Rc::from([
                Term::Var("pair".into()),
                Term::Var("old_state".into()),
            ]))),
        )),
    );
    let entry_ty = pair_ty(pair_ty(int_ty(), int_ty()), int_ty());

    let mut state = GenState {
        compute_generalized: false,
        counter_rep: CounterRep::Int,
    };
    let mut actions = vec![SessionAction::Step {
        steps: 10 + cur.pick(3),
    }];

    let action_count = 4 + cur.pick(6);
    for idx in 0..action_count {
        let choice = cur.pick(13);
        let action = match choice {
            0..=7 => generate_swap_action(&mut cur, &mut state, choice),
            8..=11 => generate_migration_action(&mut cur, &mut state, choice),
            _ if idx % 2 == 0 => SessionAction::Step {
                steps: 1 + cur.pick(4),
            },
            _ if cur.pick(2) == 0 => SessionAction::Call {
                expr: Term::app(Term::Global(hot("compute")), Term::Lit(Lit::Int(42))),
                expected_ty: int_ty(),
            },
            _ => SessionAction::ReadState {
                cell: hot("counter"),
            },
        };
        actions.push(action);
    }

    actions.push(SessionAction::Call {
        expr: Term::app(Term::Global(hot("compute")), Term::Lit(Lit::Int(10))),
        expected_ty: int_ty(),
    });
    actions.push(SessionAction::ReadState {
        cell: hot("counter"),
    });

    SessionScenario {
        initial_modules: vec![initial_module],
        entry,
        entry_ty,
        actions,
    }
}
