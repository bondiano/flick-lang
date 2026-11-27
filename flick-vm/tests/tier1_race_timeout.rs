//! Tier-1 integration tests for cancelled task finalization, live-child scope
//! waiting, `race_first` / `race_success` / `timeout` winner/loser semantics,
//! arenas + C0-C6 close order, `Ref` cycles, and `TestClock` / cooperative
//! preemption with zero heap leaks (#976).

use std::collections::BTreeMap;

use flick_core::ir::memory::{
    AsyncIntrinsic, Block, DurationIntrinsic, Exit, Function, Global, Instruction, Intrinsic, Name,
    Operation, Program,
};
use flick_core::ir::{BinOp, Lit};
use flick_core::numeric::{Arith, Num};
use flick_core::opt::perceus;
use flick_vm::tier0::{self, Heap, ProgramOutcome, Value};
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

/// Acceptance criterion 3:
/// `race_first`, `race_success`, and `timeout` keep the winner's result and
/// interrupt and await the losers before returning.
#[test]
#[allow(clippy::too_many_lines, clippy::similar_names)]
fn race_first_race_success_and_timeout_keep_winner_and_await_losers() {
    // Helper: sleep_ms(ms) = Duration(Millis)(ms) -> EffectSleep
    let sleep_task = |ms: f64, msg: &str, ret_int: i32| {
        fn_of(
            vec![0, 1],
            vec![
                let_op(2, Operation::Literal(Lit::Float(ms))),
                let_op(
                    3,
                    Operation::Intrinsic {
                        kind: Intrinsic::Duration(DurationIntrinsic::Millis),
                        args: vec![2],
                    },
                ),
                let_op(
                    4,
                    Operation::Intrinsic {
                        kind: Intrinsic::FromResult,
                        args: vec![3],
                    },
                ),
                let_op(
                    5,
                    Operation::Intrinsic {
                        kind: Intrinsic::Async(AsyncIntrinsic::EffectSleep),
                        args: vec![4],
                    },
                ),
                let_op(6, Operation::Literal(Lit::String(msg.into()))),
                let_op(
                    7,
                    Operation::Intrinsic {
                        kind: Intrinsic::PrintLine,
                        args: vec![6],
                    },
                ),
                let_op(8, Operation::Literal(Lit::Int(ret_int))),
            ],
            8,
        )
    };
    let loser_wrapped = |fin_code: u32, body_code: u32| {
        fn_of(
            vec![0, 1],
            vec![
                let_op(
                    2,
                    Operation::Task {
                        code: fin_code,
                        captures: vec![],
                    },
                ),
                let_op(
                    3,
                    Operation::Task {
                        code: body_code,
                        captures: vec![],
                    },
                ),
                let_op(
                    4,
                    Operation::Intrinsic {
                        kind: Intrinsic::Async(AsyncIntrinsic::EffectEnsuring),
                        args: vec![2, 3],
                    },
                ),
            ],
            4,
        )
    };
    // Code 1: rf winner (10ms -> 11)
    let rf_win = sleep_task(10.0, "rf-win", 11);
    // Code 2: rf loser fin
    let rf_fin = print_task("rf-loser-cleaned");
    // Code 3: rf loser body (200ms)
    let rf_loser_body = sleep_task(200.0, "rf-loser-never", 22);
    // Code 4: rf loser wrapped in ensuring
    let rf_loser = loser_wrapped(2, 3);
    // Code 5: rs fast failing branch (5ms -> Raise)
    let rs_fail = fn_of(
        vec![0, 1],
        vec![
            let_op(2, Operation::Literal(Lit::Float(5.0))),
            let_op(
                3,
                Operation::Intrinsic {
                    kind: Intrinsic::Duration(DurationIntrinsic::Millis),
                    args: vec![2],
                },
            ),
            let_op(
                4,
                Operation::Intrinsic {
                    kind: Intrinsic::FromResult,
                    args: vec![3],
                },
            ),
            let_op(
                5,
                Operation::Intrinsic {
                    kind: Intrinsic::Async(AsyncIntrinsic::EffectSleep),
                    args: vec![4],
                },
            ),
            let_op(6, Operation::Literal(Lit::String("rs-fail".into()))),
            let_op(
                7,
                Operation::Intrinsic {
                    kind: Intrinsic::PrintLine,
                    args: vec![6],
                },
            ),
            let_op(8, Operation::Literal(Lit::String("boom".into()))),
            let_op(
                9,
                Operation::Intrinsic {
                    kind: Intrinsic::Raise,
                    args: vec![8],
                },
            ),
        ],
        9,
    );
    // Code 6: rs slow winning branch (25ms -> 33)
    let rs_win = sleep_task(25.0, "rs-win", 33);
    // Code 7: timeout loser fin
    let to_fin = print_task("to-loser-cleaned");
    // Code 8: timeout loser body (500ms)
    let to_body = sleep_task(500.0, "to-loser-never", 77);
    // Code 9: timeout loser wrapped in ensuring
    let to_loser = loser_wrapped(7, 8);

    // Code 0: main runs race_first + race_success + timeout_option, returns rf + rs
    let main = fn_of(
        vec![],
        vec![
            let_op(
                0,
                Operation::Task {
                    code: 1,
                    captures: vec![],
                },
            ),
            let_op(
                1,
                Operation::Task {
                    code: 4,
                    captures: vec![],
                },
            ),
            let_op(
                2,
                Operation::Intrinsic {
                    kind: Intrinsic::Async(AsyncIntrinsic::RaceFirst),
                    args: vec![0, 1],
                },
            ),
            let_op(3, Operation::Literal(Lit::String("after-rf".into()))),
            let_op(
                4,
                Operation::Intrinsic {
                    kind: Intrinsic::PrintLine,
                    args: vec![3],
                },
            ),
            let_op(
                5,
                Operation::Task {
                    code: 5,
                    captures: vec![],
                },
            ),
            let_op(
                6,
                Operation::Task {
                    code: 6,
                    captures: vec![],
                },
            ),
            let_op(
                7,
                Operation::Intrinsic {
                    kind: Intrinsic::Async(AsyncIntrinsic::RaceSuccess),
                    args: vec![5, 6],
                },
            ),
            let_op(8, Operation::Literal(Lit::String("after-rs".into()))),
            let_op(
                9,
                Operation::Intrinsic {
                    kind: Intrinsic::PrintLine,
                    args: vec![8],
                },
            ),
            let_op(10, Operation::Literal(Lit::Float(15.0))),
            let_op(
                13,
                Operation::Task {
                    code: 9,
                    captures: vec![],
                },
            ),
            let_op(
                14,
                Operation::Intrinsic {
                    kind: Intrinsic::Async(AsyncIntrinsic::TimeoutOption),
                    args: vec![10, 13],
                },
            ),
            let_op(
                15,
                Operation::Intrinsic {
                    kind: Intrinsic::Show,
                    args: vec![14],
                },
            ),
            let_op(
                16,
                Operation::Intrinsic {
                    kind: Intrinsic::PrintLine,
                    args: vec![15],
                },
            ),
            let_op(
                17,
                Operation::Binary {
                    op: BinOp::Arith(Arith::Add, Num::Int),
                    left: 2,
                    right: 7,
                },
            ),
        ],
        17,
    );

    let program = program_of([
        (0, main),
        (1, rf_win),
        (2, rf_fin),
        (3, rf_loser_body),
        (4, rf_loser),
        (5, rs_fail),
        (6, rs_win),
        (7, to_fin),
        (8, to_body),
        (9, to_loser),
    ]);
    let expected = "\
rf-win
rf-loser-cleaned
after-rf
rs-fail
rs-win
after-rs
to-loser-cleaned
None
";
    run_both_expect_stdout(&program, expected, 44);
}
