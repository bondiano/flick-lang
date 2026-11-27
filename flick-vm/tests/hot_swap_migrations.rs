//! Integration tests for atomic typed state-cell migrations (#990):
//! - migrating `Ref` state cells (`Int -> (Int, Bool)`) with two-version store
//!   safety (suspended frame holding pre-migration `Ref` keeps reading old `Int`
//!   while fresh reads see migrated `(Int, Bool)`);
//! - atomic rollback on failing (`Die`), typed-error (`Raise`), ill-typed,
//!   effectful/mutating, fuel-exhausted, and partial-batch-failing migrations
//!   with exact heap-slot/byte preservation and zero leaks;
//! - combined `swap_with_migration` atomicity.

#[path = "hot_swap_support/mod.rs"]
mod support;

use std::collections::BTreeMap;

use flick_core::ir::memory::{
    Block, Exit, Function, Global, Instruction, Intrinsic, Name, Operation, Program,
};
use flick_core::ir::{BinOp, Lit};
use flick_core::numeric::{Arith, Num};
use flick_core::types::Type;

use flick_vm::session::{
    EvalOutcome, ExecutionTier, HotSwapError, MigrationBatchRequest, StateCellSpec, StateMigration,
    StepEvalOutcome, SwapBinding, SwapDefinitionsRequest, VmSession,
};
use flick_vm::tier0::HostMode;
use support::{
    add_const_program, bool_ty, int_global_program, int_ty, migrate_die_program,
    migrate_int_to_pair_program, migrate_raise_program, migrate_side_effect_program, named_row,
    pair_ty, pure_fun, rt_fun, string_ty,
};

#[allow(clippy::too_many_lines)]
fn state_cells_program(init_counter: i32) -> Program {
    let mut functions = BTreeMap::new();
    functions.insert(
        0,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![
                    Instruction::Let {
                        binding: 0,
                        value: Operation::Literal(Lit::Int(init_counter)),
                    },
                    Instruction::Let {
                        binding: 1,
                        value: Operation::Intrinsic {
                            kind: Intrinsic::RefMake,
                            args: vec![0],
                        },
                    },
                ],
                exit: Exit::Return(1),
            },
        },
    );
    functions.insert(
        1,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![
                    Instruction::Let {
                        binding: 0,
                        value: Operation::Literal(Lit::String("v0".to_owned())),
                    },
                    Instruction::Let {
                        binding: 1,
                        value: Operation::Intrinsic {
                            kind: Intrinsic::RefMake,
                            args: vec![0],
                        },
                    },
                ],
                exit: Exit::Return(1),
            },
        },
    );
    functions.insert(
        2,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![
                    Instruction::Let {
                        binding: 0,
                        value: Operation::Global(Name::new("App", "counter")),
                    },
                    Instruction::Let {
                        binding: 1,
                        value: Operation::Literal(Lit::Int(1)),
                    },
                    Instruction::Let {
                        binding: 2,
                        value: Operation::Literal(Lit::Int(2)),
                    },
                    Instruction::Let {
                        binding: 3,
                        value: Operation::Binary {
                            op: BinOp::Arith(Arith::Add, Num::Int),
                            left: 1,
                            right: 2,
                        },
                    },
                    Instruction::Drop(3),
                    Instruction::Let {
                        binding: 4,
                        value: Operation::Intrinsic {
                            kind: Intrinsic::RefGet,
                            args: vec![0],
                        },
                    },
                ],
                exit: Exit::Return(4),
            },
        },
    );
    functions.insert(
        3,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![Instruction::Let {
                    binding: 0,
                    value: Operation::Literal(Lit::Int(1)),
                }],
                exit: Exit::Return(0),
            },
        },
    );
    Program {
        functions,
        globals: vec![
            Global {
                name: Name::new("App", "counter"),
                initializer: 0,
            },
            Global {
                name: Name::new("App", "label"),
                initializer: 1,
            },
            Global {
                name: Name::new("App", "old_reader"),
                initializer: 2,
            },
            Global {
                name: Name::new("App", "version_tag"),
                initializer: 3,
            },
        ],
        op_spans: BTreeMap::new(),
    }
}

fn migrate_diverging_program() -> Program {
    let mut functions = BTreeMap::new();
    functions.insert(
        0,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![Instruction::Let {
                    binding: 0,
                    value: Operation::Closure {
                        code: 1,
                        captures: vec![],
                    },
                }],
                exit: Exit::Return(0),
            },
        },
    );
    functions.insert(
        1,
        Function {
            params: vec![0, 1],
            body: Block {
                instructions: vec![
                    Instruction::Drop(0),
                    Instruction::Drop(1),
                    Instruction::Let {
                        binding: 2,
                        value: Operation::Loop {
                            id: "spin".to_owned(),
                            code: 2,
                            args: vec![],
                        },
                    },
                ],
                exit: Exit::Return(2),
            },
        },
    );
    functions.insert(
        2,
        Function {
            params: vec![],
            body: Block {
                instructions: vec![Instruction::Let {
                    binding: 0,
                    value: Operation::Literal(Lit::Unit),
                }],
                exit: Exit::Return(0),
            },
        },
    );
    Program {
        functions,
        globals: vec![Global {
            name: Name::new("Mig", "spin"),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    }
}

#[test]
fn typed_state_migration_updates_live_cell_and_preserves_two_version_ref_safety() {
    let mut session = VmSession::with_host_mode(HostMode::Deterministic);
    let prog = state_cells_program(41);
    let _ = session.eval_declarations(
        &prog,
        "App",
        &[
            Name::new("App", "counter"),
            Name::new("App", "label"),
            Name::new("App", "version_tag"),
        ],
        None,
        None,
    );
    session
        .declare_state_cell(StateCellSpec {
            name: Name::new("App", "counter"),
            ty: int_ty(),
            is_ref: true,
        })
        .expect("declare counter");

    let task = session
        .start_step_existing(&Name::new("App", "old_reader"), ExecutionTier::Tier0)
        .expect("start old_reader");
    assert!(matches!(
        session.step_eval(task, 3).expect("step old_reader"),
        StepEvalOutcome::Running { .. }
    ));

    let report = session
        .swap_with_migration(
            Some(SwapDefinitionsRequest::new(
                int_global_program("App", "version_tag", 2),
                vec![SwapBinding {
                    name: Name::new("App", "version_tag"),
                    new_scheme: Some(int_ty()),
                }],
            )),
            Some(&MigrationBatchRequest::with_fuel(
                vec![StateMigration {
                    cell: Name::new("App", "counter"),
                    old_ty: int_ty(),
                    new_ty: pair_ty(int_ty(), bool_ty()),
                    migrate_scheme: Some(pure_fun(int_ty(), pair_ty(int_ty(), bool_ty()))),
                    migrate_entry: Name::new("Mig", "int_to_pair"),
                    migrate_program: migrate_int_to_pair_program(),
                }],
                500,
            )),
        )
        .expect("atomic swap + migration commits");

    assert_eq!(report.migrated_cells.len(), 1);
    assert_eq!(report.migrated_cells[0].version, 1);
    assert_ne!(
        report.migrated_cells[0].old_cell_id,
        report.migrated_cells[0].new_cell_id
    );
    assert_eq!(
        session.state_cell_type(&Name::new("App", "counter")),
        Some(&pair_ty(int_ty(), bool_ty()))
    );
    assert_eq!(
        session.read_state_cell(&Name::new("App", "counter")),
        Ok(EvalOutcome::Value {
            rendered: "(42, true)".to_owned(),
            stdout: String::new(),
        })
    );
    assert_eq!(
        session.eval_existing_on_tier(ExecutionTier::Tier0, &Name::new("App", "version_tag")),
        EvalOutcome::Value {
            rendered: "2".to_owned(),
            stdout: String::new(),
        }
    );
    assert_eq!(
        session.finish_step_eval(task),
        Ok(EvalOutcome::Value {
            rendered: "41".to_owned(),
            stdout: String::new(),
        })
    );
    assert_eq!(session.shutdown(), Ok(()));
}

#[test]
#[allow(clippy::too_many_lines)]
fn failing_diverging_and_partial_batch_migrations_roll_back_atomically_with_zero_leaks() {
    let mut session = VmSession::with_host_mode(HostMode::Deterministic);
    let prog = state_cells_program(10);
    let _ = session.eval_declarations(
        &prog,
        "App",
        &[
            Name::new("App", "counter"),
            Name::new("App", "label"),
            Name::new("App", "version_tag"),
        ],
        None,
        None,
    );
    session
        .declare_state_cell(StateCellSpec {
            name: Name::new("App", "counter"),
            ty: int_ty(),
            is_ref: true,
        })
        .expect("declare counter");
    session
        .declare_state_cell(StateCellSpec {
            name: Name::new("App", "label"),
            ty: string_ty(),
            is_ref: true,
        })
        .expect("declare label");

    let slots_before = session.heap_inventory().total;
    let bytes_before = session.heap_live_bytes();

    // 1. Effectful migration scheme rejected.
    assert!(matches!(
        session.migrate_state_cells(&MigrationBatchRequest::with_fuel(
            vec![StateMigration {
                cell: Name::new("App", "counter"),
                old_ty: int_ty(),
                new_ty: int_ty(),
                migrate_scheme: Some(rt_fun(
                    int_ty(),
                    named_row("Demo", "Log"),
                    Type::empty_row(),
                    int_ty(),
                )),
                migrate_entry: Name::new("Mig", "add"),
                migrate_program: add_const_program("Mig", "add", 5),
            }],
            500,
        )),
        Err(HotSwapError::MigrationTypeError { .. })
    ));

    // 2. Disallowed side effect (`PrintLine`) rejected.
    assert!(matches!(
        session.migrate_state_cells(&MigrationBatchRequest::with_fuel(
            vec![StateMigration {
                cell: Name::new("App", "counter"),
                old_ty: int_ty(),
                new_ty: int_ty(),
                migrate_scheme: Some(pure_fun(int_ty(), int_ty())),
                migrate_entry: Name::new("Mig", "io"),
                migrate_program: migrate_side_effect_program(),
            }],
            500,
        )),
        Err(HotSwapError::MigrationFailed { .. })
    ));

    // 3. Dying (`Die`) migration rolls back.
    assert!(matches!(
        session.migrate_state_cells(&MigrationBatchRequest::with_fuel(
            vec![StateMigration {
                cell: Name::new("App", "counter"),
                old_ty: int_ty(),
                new_ty: int_ty(),
                migrate_scheme: Some(pure_fun(int_ty(), int_ty())),
                migrate_entry: Name::new("Mig", "die"),
                migrate_program: migrate_die_program(),
            }],
            500,
        )),
        Err(HotSwapError::MigrationFailed { .. })
    ));

    // 4. Raising (`Raise`) migration rolls back.
    assert!(matches!(
        session.migrate_state_cells(&MigrationBatchRequest::with_fuel(
            vec![StateMigration {
                cell: Name::new("App", "counter"),
                old_ty: int_ty(),
                new_ty: int_ty(),
                migrate_scheme: Some(pure_fun(int_ty(), int_ty())),
                migrate_entry: Name::new("Mig", "raise"),
                migrate_program: migrate_raise_program(),
            }],
            500,
        )),
        Err(HotSwapError::MigrationFailed { .. })
    ));

    // 5. Partial-batch fuel exhaustion + simultaneous swap rolls back BOTH atomically.
    assert!(matches!(
        session.swap_with_migration(
            Some(SwapDefinitionsRequest::new(
                int_global_program("App", "version_tag", 99),
                vec![SwapBinding {
                    name: Name::new("App", "version_tag"),
                    new_scheme: Some(int_ty()),
                }],
            )),
            Some(&MigrationBatchRequest::with_fuel(
                vec![
                    StateMigration {
                        cell: Name::new("App", "counter"),
                        old_ty: int_ty(),
                        new_ty: int_ty(),
                        migrate_scheme: Some(pure_fun(int_ty(), int_ty())),
                        migrate_entry: Name::new("Mig", "add"),
                        migrate_program: add_const_program("Mig", "add", 100),
                    },
                    StateMigration {
                        cell: Name::new("App", "label"),
                        old_ty: string_ty(),
                        new_ty: string_ty(),
                        migrate_scheme: Some(pure_fun(string_ty(), string_ty())),
                        migrate_entry: Name::new("Mig", "spin"),
                        migrate_program: migrate_diverging_program(),
                    },
                ],
                32,
            )),
        ),
        Err(HotSwapError::MigrationFailed { .. })
    ));

    assert_eq!(session.heap_inventory().total, slots_before);
    assert_eq!(session.heap_live_bytes(), bytes_before);
    assert_eq!(
        session.state_cell_version(&Name::new("App", "counter")),
        Some(0)
    );
    assert_eq!(
        session.state_cell_version(&Name::new("App", "label")),
        Some(0)
    );
    assert_eq!(
        session.read_state_cell(&Name::new("App", "counter")),
        Ok(EvalOutcome::Value {
            rendered: "10".to_owned(),
            stdout: String::new(),
        })
    );
    assert_eq!(
        session.read_state_cell(&Name::new("App", "label")),
        Ok(EvalOutcome::Value {
            rendered: "\"v0\"".to_owned(),
            stdout: String::new(),
        })
    );
    assert_eq!(
        session.eval_existing_on_tier(ExecutionTier::Tier0, &Name::new("App", "version_tag")),
        EvalOutcome::Value {
            rendered: "1".to_owned(),
            stdout: String::new(),
        }
    );
    assert_eq!(session.shutdown(), Ok(()));
}
