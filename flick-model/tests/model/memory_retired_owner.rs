//! Selecting inert identity does not admit acquisitions into a retired owner.
use crate::recovery;
use flick_core::ir::{
    Lit,
    memory::{
        Block, Exit, Function, Global, Instruction, Intrinsic, Name, Operation, Program, ValueRep,
    },
};
use flick_core::opt::perceus;
use flick_vm::tier0::{Error, Heap, Value, execute_program};
use std::collections::BTreeMap;

fn bind(binding: u32, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
}
fn intrinsic(binding: u32, kind: Intrinsic, args: &[u32]) -> Instruction {
    bind(
        binding,
        Operation::Intrinsic {
            kind,
            args: args.to_vec(),
        },
    )
}
fn function(params: &[u32], instructions: Vec<Instruction>, result: u32) -> Function {
    perceus::insert(&Function {
        params: params.to_vec(),
        body: Block {
            instructions,
            exit: Exit::Return(result),
        },
    })
    .expect("valid retired owner test")
}
fn program(functions: Vec<Function>) -> Program {
    Program {
        functions: functions
            .into_iter()
            .enumerate()
            .map(|(i, f)| (u32::try_from(i).expect("small program"), f))
            .collect(),
        globals: vec![Global {
            name: entry(),
            initializer: 0,
        }],
        op_spans: BTreeMap::new(),
    }
}
fn entry() -> Name {
    Name::new("Retired", "main")
}
fn closure(binding: u32, code: u32, captures: &[u32]) -> Instruction {
    bind(
        binding,
        Operation::Closure {
            code,
            captures: captures.to_vec(),
        },
    )
}
fn tuple(binding: u32, fields: &[u32]) -> Instruction {
    bind(
        binding,
        Operation::Construct {
            tag: "$tuple".into(),
            fields: fields.to_vec(),
            rep: ValueRep::Cell,
            layout: None,
        },
    )
}
fn within(binding: u32, owner: u32, code: u32, args: &[u32]) -> Instruction {
    bind(
        binding,
        Operation::Within {
            owner,
            code,
            args: args.to_vec(),
        },
    )
}
fn inert_program() -> Program {
    program(vec![
        function(&[], vec![closure(0, 1, &[])], 0),
        function(
            &[0, 1],
            vec![
                closure(2, 2, &[]),
                intrinsic(3, Intrinsic::ScopeRun, &[2]),
                within(4, 3, 3, &[1]),
            ],
            4,
        ),
        function(&[0, 1], vec![], 1),
        function(&[0], vec![bind(1, Operation::Literal(Lit::Int(42)))], 1),
    ])
}
fn finish(heap: &Heap) {
    let stats = heap
        .finish()
        .expect("actual heap empty after caller release");
    assert_eq!(stats.live_objects, 0);
    assert_eq!(stats.allocations, stats.frees);
    assert_eq!(stats.arenas_allocated, 0);
    assert_eq!(stats.arenas_freed, 0);
}
#[test]
fn same_execution_retired_owner_can_select_an_inert_body() {
    let mut heap = Heap::new();
    let argument = heap
        .literal(&Lit::String("owned input".into()))
        .expect("input");
    let ran = execute_program(&mut heap, &inert_program(), &entry(), vec![argument], 200)
        .expect("inert selection is not acquisition");
    assert_eq!(ran.value, Value::Int(42));
    heap.drop_value(ran.value).expect("owned result release");
    finish(&heap);
}
#[test]
fn every_retired_selection_fuel_prefix_preserves_external_owned_alias() {
    let p = inert_program();
    let mut completed = false;
    for fuel in 0..200 {
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("caller".into()))
            .expect("caller root");
        let argument = heap.dup(&external).expect("argument alias");
        match execute_program(&mut heap, &p, &entry(), vec![argument], fuel) {
            Ok(ran) => {
                completed = true;
                assert_eq!(ran.value, Value::Int(42));
                heap.drop_value(ran.value).expect("owned result");
            }
            Err(error) => assert_eq!(error, Error::OutOfFuel, "fuel={fuel}"),
        }
        assert_eq!(heap.string(&external), Ok("caller"));
        assert_eq!(heap.stats().live_objects, 1);
        heap.drop_value(external).expect("caller release");
        finish(&heap);
    }
    assert!(completed, "prefix range includes completion");
}

fn restoration_program(raise: bool, handler_errors: bool) -> Program {
    let inspect = vec![
        bind(0, Operation::ReadOwnerEvidence),
        bind(1, Operation::Literal(Lit::Unit)),
        intrinsic(2, Intrinsic::ScopeCurrent, &[1]),
        tuple(3, &[0, 2]),
    ];
    let mut selected = inspect;
    let result = if raise {
        selected.push(intrinsic(4, Intrinsic::Raise, &[3]));
        4
    } else {
        3
    };
    let mut program = program(vec![
        function(
            &[],
            vec![
                bind(0, Operation::Literal(Lit::Unit)),
                intrinsic(1, Intrinsic::ScopeCurrent, &[0]),
                closure(2, 1, &[]),
                intrinsic(3, Intrinsic::ScopeRun, &[2]),
                bind(
                    4,
                    Operation::WithOwnerEvidence {
                        owner: 1,
                        code: 2,
                        args: vec![3],
                    },
                ),
                bind(5, Operation::ReadOwnerEvidence),
                tuple(6, &[1, 3, 4, 5]),
            ],
            6,
        ),
        function(&[0, 1], vec![], 1),
        function(
            &[0],
            vec![
                closure(1, 3, &[0]),
                closure(2, 4, &[]),
                bind(
                    3,
                    if handler_errors {
                        Operation::Handle {
                            body: 1,
                            effects: vec![],
                            ret: None,
                            errors: Some(2),
                        }
                    } else {
                        Operation::Intrinsic {
                            kind: Intrinsic::Try,
                            args: vec![1, 2],
                        }
                    },
                ),
                bind(4, Operation::ReadOwnerEvidence),
                bind(5, Operation::Literal(Lit::Unit)),
                intrinsic(6, Intrinsic::ScopeCurrent, &[5]),
                tuple(7, &[3, 4, 6]),
            ],
            7,
        ),
        function(
            &[0, 1],
            vec![
                bind(
                    2,
                    Operation::Project {
                        object: 0,
                        field: 0,
                        rep: ValueRep::Cell,
                    },
                ),
                within(3, 2, 5, &[]),
            ],
            3,
        ),
        function(&[0, 1], vec![], 1),
        function(&[], selected, result),
    ]);
    recovery::seal_bodies(&mut program, &[4]);
    program
}
fn assert_restoration(heap: &Heap, value: &Value) {
    let root = heap.field(value, 0).expect("root D");
    let retired = heap.field(value, 1).expect("retired D");
    assert_ne!(root, retired);
    let restored = heap.field(value, 2).expect("bracket result");
    let inside = heap.field(restored, 0).expect("selected body result");
    assert_eq!(heap.field(inside, 0).expect("C during selection"), root);
    assert_eq!(heap.field(inside, 1).expect("D during selection"), retired);
    assert_eq!(heap.field(restored, 1).expect("restored caller C"), root);
    assert_eq!(heap.field(restored, 2).expect("restored caller D"), root);
    assert_eq!(
        heap.field(value, 3).expect("restored outer absence"),
        &Value::Unit
    );
}
#[test]
fn retired_selection_preserves_lexical_evidence_and_restores_both_channels() {
    for (raise, handler_errors) in [(false, false), (true, false), (false, true), (true, true)] {
        let p = restoration_program(raise, handler_errors);
        let mut completed = false;
        for fuel in 0..300 {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("caller".into()))
                .expect("caller root");
            match execute_program(&mut heap, &p, &entry(), vec![], fuel) {
                Ok(ran) => {
                    completed = true;
                    assert_restoration(&heap, &ran.value);
                    heap.drop_value(ran.value).expect("owned result");
                }
                Err(error) => assert_eq!(
                    error,
                    Error::OutOfFuel,
                    "raise={raise} handler_errors={handler_errors} fuel={fuel}"
                ),
            }
            assert_eq!(heap.string(&external), Ok("caller"));
            assert_eq!(heap.stats().live_objects, 1);
            heap.drop_value(external).expect("caller release");
            finish(&heap);
        }
        assert!(completed);
    }
}

#[test]
fn every_retired_acquisition_fuel_prefix_rejects_only_actual_admission() {
    let mut p = inert_program();
    p.functions.insert(
        3,
        function(
            &[0],
            vec![
                bind(1, Operation::Literal(Lit::Unit)),
                intrinsic(2, Intrinsic::ArenaMake, &[1]),
            ],
            2,
        ),
    );
    let mut rejected = false;
    for fuel in 0..200 {
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("caller".into()))
            .expect("caller root");
        let argument = heap.dup(&external).expect("argument alias");
        let error = execute_program(&mut heap, &p, &entry(), vec![argument], fuel)
            .expect_err("actual acquisition cannot enter a retired owner");
        match error {
            Error::ClosedOwner => rejected = true,
            Error::OutOfFuel => {}
            other => panic!("unexpected fault at fuel={fuel}: {other:?}"),
        }
        assert_eq!(heap.string(&external), Ok("caller"));
        assert_eq!(heap.stats().live_objects, 1);
        heap.drop_value(external).expect("caller release");
        finish(&heap);
    }
    assert!(rejected);
}

#[test]
fn retired_default_does_not_block_access_to_a_live_arena() {
    let p = program(vec![
        function(
            &[],
            vec![
                bind(0, Operation::Literal(Lit::Unit)),
                intrinsic(1, Intrinsic::ArenaMake, &[0]),
                closure(2, 1, &[]),
                intrinsic(3, Intrinsic::ScopeRun, &[2]),
                within(4, 3, 2, &[1]),
            ],
            4,
        ),
        function(&[0, 1], vec![], 1),
        function(
            &[0],
            vec![
                bind(1, Operation::Literal(Lit::String("live contents".into()))),
                intrinsic(2, Intrinsic::ArenaAlloc, &[0, 1]),
                intrinsic(3, Intrinsic::ArenaGet, &[2]),
            ],
            3,
        ),
    ]);
    let mut heap = Heap::new();
    let ran = execute_program(&mut heap, &p, &entry(), vec![], 200)
        .expect("arena access validates its actual live handle, not default owner");
    assert_eq!(heap.string(&ran.value), Ok("live contents"));
    assert_eq!(heap.stats().arenas_allocated, 1);
    assert_eq!(heap.stats().arenas_freed, 1);
    heap.drop_value(ran.value)
        .expect("caller releases retained contents");
    let stats = heap.finish().expect("actual heap empty");
    assert_eq!(stats.live_objects, 0);
    assert_eq!(stats.allocations, stats.frees);
}
