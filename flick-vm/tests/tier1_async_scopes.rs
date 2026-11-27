//! Tier-1 integration tests for cancelled task finalization, live-child scope
//! waiting, `race_first` / `race_success` / `timeout` winner/loser semantics,
//! arenas + C0-C6 close order, `Ref` cycles, and `TestClock` / cooperative
//! preemption with zero heap leaks (#976).

use std::collections::BTreeMap;

use flick_core::ir::memory::{
    AsyncIntrinsic, Block, Exit, Function, Global, Instruction, Intrinsic, Name, Operation,
    Program, ValueRep,
};
use flick_core::ir::{BinOp, Lit};
use flick_core::numeric::{Arith, Num};
use flick_core::opt::perceus;
use flick_vm::tier0::{self, DefectCode, Heap, ProgramOutcome, Reason, Value};
use flick_vm::tier1;

fn let_op(binding: u32, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
}

fn main_name() -> Name {
    Name::new("Test", "main")
}

fn fn_of(params: Vec<u32>, instructions: Vec<Instruction>, ret: u32) -> Function {
    perceus::insert(&Function {
        params,
        body: Block {
            instructions,
            exit: Exit::Return(ret),
        },
    })
    .expect("valid ownership IR")
}

fn program_of(funcs: impl IntoIterator<Item = (u32, Function)>) -> Program {
    Program {
        functions: BTreeMap::from_iter(funcs),
        globals: vec![Global {
            name: main_name(),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    }
}

fn print_task(msg: &str) -> Function {
    fn_of(
        vec![0, 1],
        vec![
            let_op(2, Operation::Literal(Lit::String(msg.into()))),
            let_op(
                3,
                Operation::Intrinsic {
                    kind: Intrinsic::PrintLine,
                    args: vec![2],
                },
            ),
        ],
        3,
    )
}

fn run_both_expect_stdout(program: &Program, expected_stdout: &str, expected_int: i32) {
    let fuel = 500_000;
    let mut h0 = Heap::new();
    let r0 = tier0::execute_main_program_outcome(&mut h0, program, &main_name(), fuel);
    assert_eq!(r0.stdout, expected_stdout, "tier0 stdout mismatch");
    assert_eq!(
        format!("{:?}", r0.outcome),
        format!("{:?}", ProgramOutcome::Returned(Value::Int(expected_int))),
        "tier0 outcome mismatch"
    );
    r0.outcome.release(&mut h0).expect("release tier0");
    h0.finish().expect("zero leaks tier0");

    let mut h1 = Heap::new();
    let r1 = tier1::execute_main_program_outcome(&mut h1, program, &main_name(), fuel)
        .expect("tier1 prepares");
    assert_eq!(r1.stdout, expected_stdout, "tier1 stdout mismatch");
    assert_eq!(
        format!("{:?}", r1.outcome),
        format!("{:?}", ProgramOutcome::Returned(Value::Int(expected_int))),
        "tier1 outcome mismatch"
    );
    r1.outcome.release(&mut h1).expect("release tier1");
    let stats1 = h1.finish().expect("zero leaks tier1");
    assert_eq!(stats1.live_objects, 0);
    assert_eq!(stats1.allocations, stats1.frees);

    for prefix in [1, 5, 25, 100, 400] {
        let mut hf = Heap::new();
        let rf = tier1::execute_main_program_outcome(&mut hf, program, &main_name(), prefix)
            .expect("tier1 prepares prefix");
        rf.outcome.release(&mut hf).expect("release prefix");
        let sf = hf.finish().expect("zero leaks on fuel prefix");
        assert_eq!(sf.live_objects, 0);
    }
}

/// Acceptance criterion 2:
/// A cancelled task runs its finalizers once, and its scope does not return
/// while scoped children are still live (C2 waits for child unwinding &
/// finalizers before running C5 outer finalizers and returning).
#[test]
#[allow(clippy::too_many_lines)]
fn cancelled_task_runs_finalizers_once_and_scope_waits_for_live_children() {
    // Code 1: supervisor callback `fun _ -> ()`
    let sup = fn_of(
        vec![0, 1],
        vec![let_op(2, Operation::Literal(Lit::Unit))],
        2,
    );
    // Code 2: child ensuring finalizer: print start, yield_now, print end
    let child_ensuring = fn_of(
        vec![0, 1],
        vec![
            let_op(
                2,
                Operation::Literal(Lit::String("child-ensuring-start".into())),
            ),
            let_op(
                3,
                Operation::Intrinsic {
                    kind: Intrinsic::PrintLine,
                    args: vec![2],
                },
            ),
            let_op(4, Operation::Literal(Lit::Unit)),
            let_op(
                5,
                Operation::Intrinsic {
                    kind: Intrinsic::Async(AsyncIntrinsic::EffectYieldNow),
                    args: vec![4],
                },
            ),
            let_op(
                6,
                Operation::Literal(Lit::String("child-ensuring-end".into())),
            ),
            let_op(
                7,
                Operation::Intrinsic {
                    kind: Intrinsic::PrintLine,
                    args: vec![6],
                },
            ),
        ],
        7,
    );
    // Code 3: child inner body (captures [deferred]): complete deferred with unit task, then never
    let child_inner = fn_of(
        vec![0, 1],
        vec![
            let_op(
                2,
                Operation::Project {
                    object: 0,
                    field: 0,
                    rep: ValueRep::Cell,
                },
            ),
            let_op(
                3,
                Operation::Task {
                    code: 1,
                    captures: vec![],
                },
            ),
            let_op(
                4,
                Operation::Intrinsic {
                    kind: Intrinsic::Async(AsyncIntrinsic::DeferredCompleteWith),
                    args: vec![3, 2],
                },
            ),
            let_op(5, Operation::Literal(Lit::Unit)),
            let_op(
                6,
                Operation::Intrinsic {
                    kind: Intrinsic::Async(AsyncIntrinsic::EffectNever),
                    args: vec![5],
                },
            ),
        ],
        6,
    );
    // Code 4: child task (captures [deferred, count_ref]): add finalizer + ensuring
    let child_task = fn_of(
        vec![0, 1],
        vec![
            let_op(
                2,
                Operation::Project {
                    object: 0,
                    field: 0,
                    rep: ValueRep::Cell,
                },
            ),
            let_op(
                3,
                Operation::Project {
                    object: 0,
                    field: 1,
                    rep: ValueRep::Cell,
                },
            ),
            let_op(
                4,
                Operation::Task {
                    code: 5,
                    captures: vec![3],
                },
            ),
            let_op(
                5,
                Operation::Intrinsic {
                    kind: Intrinsic::ScopeAddFinalizer,
                    args: vec![4],
                },
            ),
            let_op(
                6,
                Operation::Task {
                    code: 2,
                    captures: vec![],
                },
            ),
            let_op(
                7,
                Operation::Task {
                    code: 3,
                    captures: vec![2],
                },
            ),
            let_op(
                8,
                Operation::Intrinsic {
                    kind: Intrinsic::Async(AsyncIntrinsic::EffectEnsuring),
                    args: vec![6, 7],
                },
            ),
        ],
        8,
    );
    // Code 5: child scope finalizer (captures [count_ref]): increment count and print
    let child_fin = fn_of(
        vec![0, 1],
        vec![
            let_op(
                2,
                Operation::Project {
                    object: 0,
                    field: 0,
                    rep: ValueRep::Cell,
                },
            ),
            let_op(
                3,
                Operation::Intrinsic {
                    kind: Intrinsic::RefGet,
                    args: vec![2],
                },
            ),
            let_op(4, Operation::Literal(Lit::Int(1))),
            let_op(
                5,
                Operation::Binary {
                    op: BinOp::Arith(Arith::Add, Num::Int),
                    left: 3,
                    right: 4,
                },
            ),
            let_op(
                6,
                Operation::Intrinsic {
                    kind: Intrinsic::RefSet,
                    args: vec![2, 5],
                },
            ),
            let_op(7, Operation::Literal(Lit::String("child-fin".into()))),
            let_op(
                8,
                Operation::Intrinsic {
                    kind: Intrinsic::PrintLine,
                    args: vec![7],
                },
            ),
        ],
        8,
    );
    // Code 6: outer finalizer
    let outer_fin = print_task("outer-fin");
    // Code 7: scope body (captures [count_ref])
    let scope_body = fn_of(
        vec![0, 1],
        vec![
            let_op(
                2,
                Operation::Project {
                    object: 0,
                    field: 0,
                    rep: ValueRep::Cell,
                },
            ),
            let_op(3, Operation::Literal(Lit::Unit)),
            let_op(
                4,
                Operation::Intrinsic {
                    kind: Intrinsic::Async(AsyncIntrinsic::DeferredMake),
                    args: vec![3],
                },
            ),
            let_op(5, Operation::Literal(Lit::Unit)),
            let_op(
                6,
                Operation::Intrinsic {
                    kind: Intrinsic::ScopeCurrent,
                    args: vec![5],
                },
            ),
            let_op(
                7,
                Operation::Closure {
                    code: 1,
                    captures: vec![],
                },
            ),
            let_op(
                8,
                Operation::Task {
                    code: 4,
                    captures: vec![4, 2],
                },
            ),
            let_op(
                9,
                Operation::Intrinsic {
                    kind: Intrinsic::Async(AsyncIntrinsic::FiberSpawn),
                    args: vec![6, 7, 8],
                },
            ),
            let_op(
                10,
                Operation::Task {
                    code: 6,
                    captures: vec![],
                },
            ),
            let_op(
                11,
                Operation::Intrinsic {
                    kind: Intrinsic::ScopeAddFinalizer,
                    args: vec![10],
                },
            ),
            let_op(
                12,
                Operation::Intrinsic {
                    kind: Intrinsic::Async(AsyncIntrinsic::DeferredAwait),
                    args: vec![4],
                },
            ),
            let_op(13, Operation::Literal(Lit::String("outer-body-end".into()))),
            let_op(
                14,
                Operation::Intrinsic {
                    kind: Intrinsic::PrintLine,
                    args: vec![13],
                },
            ),
        ],
        14,
    );
    // Code 0: main
    let main = fn_of(
        vec![],
        vec![
            let_op(0, Operation::Literal(Lit::Int(0))),
            let_op(
                1,
                Operation::Intrinsic {
                    kind: Intrinsic::RefMake,
                    args: vec![0],
                },
            ),
            let_op(
                2,
                Operation::Closure {
                    code: 7,
                    captures: vec![1],
                },
            ),
            let_op(
                3,
                Operation::Intrinsic {
                    kind: Intrinsic::ScopeRun,
                    args: vec![2],
                },
            ),
            let_op(4, Operation::Literal(Lit::String("after-scope".into()))),
            let_op(
                5,
                Operation::Intrinsic {
                    kind: Intrinsic::PrintLine,
                    args: vec![4],
                },
            ),
            let_op(
                6,
                Operation::Intrinsic {
                    kind: Intrinsic::RefGet,
                    args: vec![1],
                },
            ),
        ],
        6,
    );

    let program = program_of([
        (0, main),
        (1, sup),
        (2, child_ensuring),
        (3, child_inner),
        (4, child_task),
        (5, child_fin),
        (6, outer_fin),
        (7, scope_body),
    ]);
    let expected = "\
outer-body-end
child-ensuring-start
child-ensuring-end
child-fin
outer-fin
after-scope
";
    run_both_expect_stdout(&program, expected, 1);
}

/// Arena C0-C6 retirement: finalizers can read arena handles before C6
/// retires the arena, and reading an escaped handle after close raises
/// `DefectCode::ArenaClosed` with zero heap leaks.
#[test]
#[allow(clippy::too_many_lines)]
fn arena_finalizer_reads_live_view_and_post_close_read_defects_without_leaks() {
    // Code 1: finalizer reading arena view (captures [view])
    let fin = fn_of(
        vec![0, 1],
        vec![
            let_op(
                2,
                Operation::Project {
                    object: 0,
                    field: 0,
                    rep: ValueRep::Cell,
                },
            ),
            let_op(
                3,
                Operation::Intrinsic {
                    kind: Intrinsic::ArenaGet,
                    args: vec![2],
                },
            ),
            let_op(
                4,
                Operation::Intrinsic {
                    kind: Intrinsic::PrintLine,
                    args: vec![3],
                },
            ),
        ],
        4,
    );
    // Code 2: scope body allocating arena + view, registering finalizer, returning view
    let body = fn_of(
        vec![0, 1],
        vec![
            let_op(2, Operation::Literal(Lit::Unit)),
            let_op(
                3,
                Operation::Intrinsic {
                    kind: Intrinsic::ArenaMake,
                    args: vec![2],
                },
            ),
            let_op(4, Operation::Literal(Lit::String("in-fin".into()))),
            let_op(
                5,
                Operation::Intrinsic {
                    kind: Intrinsic::ArenaAlloc,
                    args: vec![3, 4],
                },
            ),
            let_op(
                6,
                Operation::Task {
                    code: 1,
                    captures: vec![5],
                },
            ),
            let_op(
                7,
                Operation::Intrinsic {
                    kind: Intrinsic::ScopeAddFinalizer,
                    args: vec![6],
                },
            ),
        ],
        5,
    );
    // Code 0: main runs scope, then reads retired view (must defect ArenaClosed)
    let main = fn_of(
        vec![],
        vec![
            let_op(
                0,
                Operation::Closure {
                    code: 2,
                    captures: vec![],
                },
            ),
            let_op(
                1,
                Operation::Intrinsic {
                    kind: Intrinsic::ScopeRun,
                    args: vec![0],
                },
            ),
            let_op(
                2,
                Operation::Intrinsic {
                    kind: Intrinsic::ArenaGet,
                    args: vec![1],
                },
            ),
        ],
        2,
    );
    let program = program_of([(0, main), (1, fin), (2, body)]);

    let mut heap = Heap::new();
    let report = tier1::execute_main_program_outcome(&mut heap, &program, &main_name(), 100_000)
        .expect("tier1 prepares");
    assert_eq!(report.stdout, "in-fin\n");
    let ProgramOutcome::Failed(cause) = report.outcome else {
        panic!("expected ArenaClosed defect, got {:?}", report.outcome);
    };
    let mut saw_arena_closed = false;
    for reason in heap.cause_reasons(&cause).expect("reasons") {
        if let Reason::Die(payload) = reason
            && let Ok(spec) = heap.defect(payload)
            && *spec.code() == DefectCode::ArenaClosed
        {
            saw_arena_closed = true;
        }
    }
    assert!(saw_arena_closed, "expected DefectCode::ArenaClosed");
    heap.drop_cause(cause).expect("drop cause");
    let stats = heap.finish().expect("zero leaks after ArenaClosed");
    assert_eq!(stats.live_objects, 0);
}
