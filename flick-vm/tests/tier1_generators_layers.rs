//! Tier-1 integration tests for strict/lazy sequences & generators, `Ref`
//! cycles, `Lazy` cells, and dynamic layers with zero heap leaks (#976).

use std::collections::BTreeMap;

use flick_core::ir::memory::{
    Block, Exit, Function, Global, Instruction, Intrinsic, LayerIntrinsic, Name, Operation,
    Program, ValueRep,
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

#[test]
#[allow(clippy::too_many_lines)]
fn strict_and_lazy_generators_with_early_drop_cleanup_match_tier0() {
    // Code 1: generator body yielding 10, 20, 30
    let gen_body = fn_of(
        vec![0, 1],
        vec![
            let_op(2, Operation::Literal(Lit::Int(10))),
            let_op(3, Operation::Yield(2)),
            let_op(4, Operation::Literal(Lit::Int(20))),
            let_op(5, Operation::Yield(4)),
            let_op(6, Operation::Literal(Lit::Int(30))),
            let_op(7, Operation::Yield(6)),
        ],
        7,
    );
    // Code 2: helper pulling only ONE element from lazy seq then returning (tests iterator cleanup)
    let pull_one = fn_of(
        vec![0],
        vec![
            let_op(1, Operation::SeqOpen(0)),
            let_op(2, Operation::SeqNext(1)),
            let_op(3, Operation::UnwrapOption(2)),
        ],
        3,
    );
    // Code 0: main runs GenerateStrict + GenerateLazy + pull_one + SeqToList
    let main = fn_of(
        vec![],
        vec![
            let_op(0, Operation::Literal(Lit::Unit)),
            let_op(1, Operation::Literal(Lit::Unit)),
            let_op(
                2,
                Operation::GenerateStrict {
                    code: 1,
                    args: vec![0, 1],
                },
            ),
            let_op(
                3,
                Operation::Project {
                    object: 2,
                    field: 2,
                    rep: ValueRep::Cell,
                },
            ),
            let_op(
                4,
                Operation::Intrinsic {
                    kind: Intrinsic::Show,
                    args: vec![3],
                },
            ),
            let_op(
                11,
                Operation::Intrinsic {
                    kind: Intrinsic::PrintLine,
                    args: vec![4],
                },
            ),
            let_op(
                5,
                Operation::GenerateLazy {
                    code: 1,
                    captures: vec![],
                    limit: None,
                    sync: false,
                },
            ),
            let_op(
                6,
                Operation::Call {
                    code: 2,
                    args: vec![5],
                },
            ),
            let_op(
                7,
                Operation::GenerateLazy {
                    code: 1,
                    captures: vec![],
                    limit: None,
                    sync: false,
                },
            ),
            let_op(
                8,
                Operation::Intrinsic {
                    kind: Intrinsic::SeqToList,
                    args: vec![7],
                },
            ),
            let_op(
                9,
                Operation::Project {
                    object: 8,
                    field: 2,
                    rep: ValueRep::Cell,
                },
            ),
            let_op(
                10,
                Operation::Intrinsic {
                    kind: Intrinsic::Show,
                    args: vec![9],
                },
            ),
            let_op(
                12,
                Operation::Intrinsic {
                    kind: Intrinsic::PrintLine,
                    args: vec![10],
                },
            ),
        ],
        6,
    );
    let program = program_of([(0, main), (1, gen_body), (2, pull_one)]);

    let mut h0 = Heap::new();
    let r0 = tier0::execute_main_program_outcome(&mut h0, &program, &main_name(), 50_000);
    let mut h1 = Heap::new();
    let r1 = tier1::execute_main_program_outcome(&mut h1, &program, &main_name(), 50_000)
        .expect("tier1 prepares generators");

    assert_eq!(r0.stdout, "30\n30\n");
    assert_eq!(r1.stdout, r0.stdout);
    assert!(matches!(
        r0.outcome,
        ProgramOutcome::Returned(Value::Int(10))
    ));
    assert!(matches!(
        r1.outcome,
        ProgramOutcome::Returned(Value::Int(10))
    ));
    r0.outcome.release(&mut h0).expect("release tier0");
    h0.finish().expect("zero leaks tier0");
    r1.outcome.release(&mut h1).expect("release tier1");
    let stats = h1.finish().expect("zero leaks tier1");
    assert_eq!(stats.live_objects, 0);
}

#[test]
#[allow(clippy::too_many_lines)]
fn ref_cycles_lazy_memoization_and_dynamic_layer_lookup_release_cleanly() {
    // Code 1: lazy thunk incrementing counter ref (captures [counter_ref]) -> returns counter
    let lazy_thunk = fn_of(
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
            let_op(
                7,
                Operation::Intrinsic {
                    kind: Intrinsic::RefGet,
                    args: vec![2],
                },
            ),
        ],
        7,
    );
    // Code 2: layer body reading service via LayerLookupService
    let layer_body = fn_of(
        vec![0, 1],
        vec![
            let_op(2, Operation::ServiceTag("Test.Svc".into())),
            let_op(
                3,
                Operation::Intrinsic {
                    kind: Intrinsic::Layer(LayerIntrinsic::LayerLookupService),
                    args: vec![2],
                },
            ),
        ],
        3,
    );
    // Code 0: main creates ref cycle + forces Lazy twice + runs LayerSucceed/LayerRun
    let main = fn_of(
        vec![],
        vec![
            // Ref cycle: a -> Some(b), b -> Some(a)
            let_op(
                0,
                Operation::Construct {
                    tag: "Prelude.Option.None".into(),
                    fields: vec![],
                    rep: ValueRep::Cell,
                    layout: None,
                },
            ),
            let_op(
                1,
                Operation::Intrinsic {
                    kind: Intrinsic::RefMake,
                    args: vec![0],
                },
            ),
            let_op(
                2,
                Operation::Construct {
                    tag: "Prelude.Option.Some".into(),
                    fields: vec![1],
                    rep: ValueRep::Cell,
                    layout: None,
                },
            ),
            let_op(
                3,
                Operation::Intrinsic {
                    kind: Intrinsic::RefMake,
                    args: vec![2],
                },
            ),
            let_op(
                4,
                Operation::Construct {
                    tag: "Prelude.Option.Some".into(),
                    fields: vec![3],
                    rep: ValueRep::Cell,
                    layout: None,
                },
            ),
            let_op(
                5,
                Operation::Intrinsic {
                    kind: Intrinsic::RefSet,
                    args: vec![1, 4],
                },
            ),
            // Lazy forced twice runs thunk only once
            let_op(6, Operation::Literal(Lit::Int(0))),
            let_op(
                7,
                Operation::Intrinsic {
                    kind: Intrinsic::RefMake,
                    args: vec![6],
                },
            ),
            let_op(
                8,
                Operation::Task {
                    code: 1,
                    captures: vec![7],
                },
            ),
            let_op(
                9,
                Operation::Intrinsic {
                    kind: Intrinsic::LazyMake,
                    args: vec![8],
                },
            ),
            let_op(
                10,
                Operation::Intrinsic {
                    kind: Intrinsic::LazyForce,
                    args: vec![9],
                },
            ),
            let_op(
                11,
                Operation::Intrinsic {
                    kind: Intrinsic::LazyForce,
                    args: vec![9],
                },
            ),
            let_op(
                12,
                Operation::Binary {
                    op: BinOp::Arith(Arith::Add, Num::Int),
                    left: 10,
                    right: 11,
                },
            ),
            // Static layer provide + lookup
            let_op(13, Operation::ServiceTag("Test.Svc".into())),
            let_op(14, Operation::Literal(Lit::Int(40))),
            let_op(
                15,
                Operation::Intrinsic {
                    kind: Intrinsic::LayerSucceed,
                    args: vec![13, 14],
                },
            ),
            let_op(
                16,
                Operation::Closure {
                    code: 2,
                    captures: vec![],
                },
            ),
            let_op(
                17,
                Operation::Intrinsic {
                    kind: Intrinsic::LayerRun,
                    args: vec![15, 16],
                },
            ),
            let_op(
                18,
                Operation::Binary {
                    op: BinOp::Arith(Arith::Add, Num::Int),
                    left: 12,
                    right: 17,
                },
            ),
        ],
        18,
    );
    let program = program_of([(0, main), (1, lazy_thunk), (2, layer_body)]);

    let mut h0 = Heap::new();
    let r0 = tier0::execute_main_program_outcome(&mut h0, &program, &main_name(), 50_000);
    let mut h1 = Heap::new();
    let r1 = tier1::execute_main_program_outcome(&mut h1, &program, &main_name(), 50_000)
        .expect("tier1 prepares");

    assert!(matches!(
        r0.outcome,
        ProgramOutcome::Returned(Value::Int(42))
    ));
    assert!(matches!(
        r1.outcome,
        ProgramOutcome::Returned(Value::Int(42))
    ));
    r0.outcome.release(&mut h0).expect("release tier0");
    h0.finish().expect("zero leaks tier0");
    r1.outcome.release(&mut h1).expect("release tier1");
    let stats = h1.finish().expect("zero leaks tier1");
    assert_eq!(stats.live_objects, 0);
}
