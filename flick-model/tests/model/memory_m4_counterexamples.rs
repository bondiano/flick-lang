//! Consolidated M4 executable counterexample and assumption-boundary regressions (#463).
//!
//! Directly cross-referenced from `docs/formal/memory.md` §6 ("M4 proof summary,
//! assumptions, unsupported constructs, and implementation-proof boundary"):
//!
//! - **A. Premise-violation counterexamples**: pins what fails when a Lean theorem's
//!   premise or static/runtime discipline is violated (`DuplicateBinding`, `Leftover`,
//!   `NoToken`, `ReuseTokenEscape`, `AggregateReuseCandidate`, and equal-capacity
//!   incompatible `CellLayout` forcing fallback).
//! - **B. Deep `#744` unboxed aggregate rules**: 3-level nested inline aggregate
//!   projection surviving parent + intermediate drops, `BoxAggregate` dropped without
//!   unboxing releasing boxed descendants, and `BoxAggregate -> UnboxConsume` unique
//!   transfer vs shared alias preservation.
//! - **C. Failure / handler `abort` / `control` discontinuation / fuel-cut + Arena unwind**:
//!   real source + fuel-prefix sweeps exercising live `ReuseToken`, nested aggregates,
//!   and `Arena s` with LIFO finalizers + late arena allocation under abnormal exits.
//! - **D. Compiler-to-model / unproved gap pins**: root-arena `Ref` cycle retention
//!   (`cells_detached_on_teardown > 0`) vs prompt acyclic release, and non-reusable
//!   storage kinds (`Closure`, `String`, `Dictionary`) falling back cleanly.

use std::collections::{BTreeMap, BTreeSet};

use flick_core::build;
use flick_core::ir::memory::{
    self, Block, CellLayout, DictionaryEntry, Error as ValidateError, Exit, Function, Global,
    Instruction, Name, Operation, Program, ValueRep,
};
use flick_core::ir::{BinOp, Lit};
use flick_core::opt::perceus::{self, AggregatePolicy};
use flick_model::run::{CountedMain, compile_source, counted_main, main_of};
use flick_vm::tier0::{
    Error, Heap, ProgramOutcome, Stats, Value, execute, execute_program_outcome,
};

mod aggregates_and_unwind;

pub(super) fn bind(binding: u32, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
}

pub(super) fn single_fn_program(instructions: Vec<Instruction>, exit: Exit) -> (Program, Name) {
    let entry = Name::new("Main", "main");
    let program = Program {
        globals: vec![Global {
            name: entry.clone(),
            initializer: 0,
        }],
        functions: BTreeMap::from([(
            0,
            Function {
                params: vec![],
                body: Block { instructions, exit },
            },
        )]),
        op_spans: BTreeMap::new(),
    };
    memory::validate_program(&program).expect("valid memory program");
    (program, entry)
}

pub(super) fn run_program_expect_string(
    program: &Program,
    entry: &Name,
    expected: &str,
    label: &str,
) -> Stats {
    let mut heap = Heap::new();
    let report = execute_program_outcome(&mut heap, program, entry, vec![], 10_000);
    let ProgramOutcome::Returned(result) = report.outcome else {
        panic!("{label}: expected Returned, got {:?}", report.outcome);
    };
    assert_eq!(heap.string(&result), Ok(expected), "{label}");
    heap.drop_value(result).expect("drop result");
    let stats = heap.finish().expect("zero leaks");
    assert_eq!(stats.live_objects, 0, "{label}");
    assert_eq!(stats.allocations, stats.frees, "{label}");
    stats
}

pub(super) fn lower_source_with_policy(
    source: &str,
    release: bool,
    build_policy: impl FnOnce(&str) -> AggregatePolicy,
) -> (Program, Name) {
    let modules = compile_source(source).expect("source checks");
    let main_path = main_of(&modules).expect("main binding");
    let policy = build_policy(&main_path.module);
    let ir: Vec<_> = modules
        .iter()
        .map(|module| {
            if release {
                build::optimized(&modules, module, false)
            } else {
                (*module.ir).clone()
            }
        })
        .collect();
    let entry = Name::new(&main_path.module, &main_path.name);
    let program = perceus::lower_roots_with_aggregates(
        ir.iter().chain(build::builtin_modules()),
        modules.iter().map(|m| m.declarations.as_ref()),
        &[&entry],
        policy,
    )
    .expect("source aggregate memory lowering");
    memory::validate_program(&program).expect("lowered program validates");
    (program, entry)
}

// ============================================================================
// A. Premise-violation counterexamples
// ============================================================================

#[test]
fn m4_counterexample_duplicate_param_unbalanced_dup_and_stale_read_rejected() {
    let dup_param = Function {
        params: vec![0, 0],
        body: Block {
            instructions: vec![],
            exit: Exit::Return(0),
        },
    };
    assert_eq!(
        memory::validate(&dup_param),
        Err(ValidateError::DuplicateBinding(0))
    );

    let leaked_dup = Function {
        params: vec![0],
        body: Block {
            instructions: vec![Instruction::Dup(0)],
            exit: Exit::Return(0),
        },
    };
    assert_eq!(
        memory::validate(&leaked_dup),
        Err(ValidateError::Leftover(vec![(0, 1)]))
    );

    let stale_read = Function {
        params: vec![0],
        body: Block {
            instructions: vec![
                Instruction::Drop(0),
                bind(
                    1,
                    Operation::Project {
                        object: 0,
                        field: 0,
                        rep: ValueRep::Cell,
                    },
                ),
            ],
            exit: Exit::Return(1),
        },
    };
    assert_eq!(
        memory::validate(&stale_read),
        Err(ValidateError::NoToken(0))
    );
}

#[test]
fn m4_counterexample_reuse_token_escape_rejected() {
    let escape_return = Function {
        params: vec![0],
        body: Block {
            instructions: vec![bind(1, Operation::DropReuse(0))],
            exit: Exit::Return(1),
        },
    };
    memory::validate(&escape_return).expect("token-valid IR tests dynamic escape guard");

    let mut heap = Heap::new();
    let payload = heap.literal(&Lit::String("cell".into())).expect("str");
    let cell = heap
        .construct("Main.Box.Box", vec![payload])
        .expect("unique cell");
    let err = execute(&mut heap, &escape_return, vec![cell])
        .expect_err("returning a live ReuseToken must fail with ReuseTokenEscape");
    assert!(
        matches!(err, Error::ReuseTokenEscape),
        "expected ReuseTokenEscape, got {err:?}"
    );

    let escape_dup = Function {
        params: vec![0],
        body: Block {
            instructions: vec![
                bind(1, Operation::DropReuse(0)),
                Instruction::Dup(1),
                Instruction::Drop(1),
                Instruction::Drop(1),
                bind(2, Operation::Literal(Lit::Int(0))),
            ],
            exit: Exit::Return(2),
        },
    };
    memory::validate(&escape_dup).expect("token-valid IR tests dynamic Dup escape guard");

    let mut heap2 = Heap::new();
    let payload2 = heap2.literal(&Lit::String("cell2".into())).expect("str");
    let cell2 = heap2
        .construct("Main.Box.Box", vec![payload2])
        .expect("unique cell");
    let err2 = execute(&mut heap2, &escape_dup, vec![cell2])
        .expect_err("duplicating a live ReuseToken must fail with ReuseTokenEscape");
    assert!(
        matches!(err2, Error::ReuseTokenEscape),
        "expected ReuseTokenEscape, got {err2:?}"
    );
}

fn check_runtime_aggregate_rejected(name: &str, bad_op: Operation) {
    let entry = Name::new("Main", "main");
    let helper_body = if name == "DropReuse" {
        Block {
            instructions: vec![Instruction::Drop(1), bind(2, bad_op)],
            exit: Exit::Return(2),
        }
    } else {
        Block {
            instructions: vec![bind(2, bad_op)],
            exit: Exit::Return(2),
        }
    };
    let program = Program {
        globals: vec![Global {
            name: entry.clone(),
            initializer: 0,
        }],
        functions: BTreeMap::from([
            (
                0,
                Function {
                    params: vec![],
                    body: Block {
                        instructions: vec![
                            bind(0, Operation::Literal(Lit::String("agg-leaf".into()))),
                            bind(
                                1,
                                Operation::Construct {
                                    tag: "Main.Agg.Agg".into(),
                                    fields: vec![0],
                                    rep: ValueRep::Aggregate,
                                    layout: None,
                                },
                            ),
                            bind(2, Operation::Literal(Lit::String("field".into()))),
                            bind(
                                3,
                                Operation::Call {
                                    code: 1,
                                    args: vec![1, 2],
                                },
                            ),
                        ],
                        exit: Exit::Return(3),
                    },
                },
            ),
            (
                1,
                Function {
                    params: vec![0, 1],
                    body: helper_body,
                },
            ),
        ]),
        op_spans: BTreeMap::new(),
    };
    memory::validate_program(&program).expect("interprocedural callee tests dynamic guard");

    let mut heap = Heap::new();
    let external = heap
        .literal(&Lit::String("ext-root".into()))
        .expect("external");
    let report = execute_program_outcome(&mut heap, &program, &entry, vec![], 10_000);
    assert!(
        matches!(
            report.outcome,
            ProgramOutcome::Fault(Error::AggregateReuseCandidate)
        ),
        "{name}: expected Fault(AggregateReuseCandidate), got {:?}",
        report.outcome
    );
    assert_eq!(heap.string(&external), Ok("ext-root"));
    heap.drop_value(external).expect("drop external");
    let stats = heap
        .finish()
        .unwrap_or_else(|e| panic!("{name}: leaked heap objects after dynamic rejection: {e}"));
    assert_eq!(stats.live_objects, 0);
    assert_eq!(stats.allocations, stats.frees);
}

#[test]
fn m4_counterexample_runtime_aggregate_rejected_in_reuse_ops() {
    let ops = [
        ("DropReuse", Operation::DropReuse(0)),
        (
            "Reuse",
            Operation::Reuse {
                object: 0,
                tag: "Main.C.C".into(),
                fields: vec![1],
                rep: ValueRep::Cell,
                layout: None,
            },
        ),
        (
            "ReuseWithDicts",
            Operation::ReuseWithDicts {
                object: 0,
                tag: "Main.C.C".into(),
                fields: vec![1],
                dicts: vec![],
                rep: ValueRep::Cell,
                layout: None,
            },
        ),
        (
            "ReuseRecord",
            Operation::ReuseRecord {
                object: 0,
                fields: vec![("x".into(), 1)],
            },
        ),
    ];
    for (name, bad_op) in ops {
        check_runtime_aggregate_rejected(name, bad_op);
    }
}

fn check_incompatible_cell_layout_fallback(
    label: &str,
    cand_layout: CellLayout,
    target_layout: CellLayout,
) {
    let (program, entry) = single_fn_program(
        vec![
            bind(0, Operation::Literal(Lit::String("old".into()))),
            bind(1, Operation::Literal(Lit::Int(1))),
            bind(
                2,
                Operation::Construct {
                    tag: "Main.A.A".into(),
                    fields: vec![0, 1],
                    rep: ValueRep::Cell,
                    layout: Some(cand_layout),
                },
            ),
            bind(3, Operation::DropReuse(2)),
            bind(4, Operation::Literal(Lit::String("new".into()))),
            bind(5, Operation::Literal(Lit::Int(2))),
            bind(
                6,
                Operation::Reuse {
                    object: 3,
                    tag: "Main.B.B".into(),
                    fields: vec![4, 5],
                    rep: ValueRep::Cell,
                    layout: Some(target_layout),
                },
            ),
            bind(
                7,
                Operation::Project {
                    object: 6,
                    field: 0,
                    rep: ValueRep::Cell,
                },
            ),
            Instruction::Drop(6),
        ],
        Exit::Return(7),
    );
    let stats = run_program_expect_string(&program, &entry, "new", label);
    assert_eq!(
        stats.reuses, 0,
        "{label}: incompatible layout must not reuse"
    );
    assert!(
        stats.reuse_fallbacks >= 1,
        "{label}: expected fallback count >= 1, got {stats:?}"
    );
}

#[test]
fn m4_counterexample_equal_capacity_incompatible_cell_layout_forces_fallback() {
    check_incompatible_cell_layout_fallback(
        "differing_align",
        CellLayout {
            capacity: 2,
            align: 8,
            ownership: "boxed,imm".into(),
        },
        CellLayout {
            capacity: 2,
            align: 16,
            ownership: "boxed,imm".into(),
        },
    );
    check_incompatible_cell_layout_fallback(
        "differing_ownership",
        CellLayout {
            capacity: 2,
            align: 8,
            ownership: "boxed,imm".into(),
        },
        CellLayout {
            capacity: 2,
            align: 8,
            ownership: "boxed,boxed".into(),
        },
    );
}
