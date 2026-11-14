//! Inert lexical evidence and the default resource owner are independent.
use flick_core::ir::Lit;
use flick_core::ir::memory::{
    Block, Exit, Function, Global, HandlerEffect, HandlerOperation, Instruction, Intrinsic, Name,
    Operation, Program,
};
use flick_core::opt::perceus;
use flick_core::types::OpClass;
use flick_model::memory::{Error, Heap, Value, execute_program};

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
    .expect("valid evidence ownership")
}
fn entry() -> Name {
    Name::new("Evidence", "main")
}
fn program() -> Program {
    let functions = vec![
        function(
            &[],
            vec![
                bind(0, Operation::Literal(Lit::Unit)),
                intrinsic(1, Intrinsic::ScopeCurrent, &[0]),
                bind(
                    2,
                    Operation::Closure {
                        code: 1,
                        captures: vec![1],
                    },
                ),
                intrinsic(3, Intrinsic::ScopeRun, &[2]),
                bind(4, Operation::ReadOwnerEvidence),
                bind(
                    5,
                    Operation::Construct {
                        tag: "$tuple".into(),
                        fields: vec![3, 4],
                    },
                ),
            ],
            5,
        ),
        function(
            &[0, 1],
            vec![
                bind(
                    2,
                    Operation::Project {
                        object: 0,
                        field: 0,
                    },
                ),
                bind(
                    3,
                    Operation::WithOwnerEvidence {
                        owner: 2,
                        code: 2,
                        args: vec![1],
                    },
                ),
            ],
            3,
        ),
        function(
            &[0],
            vec![
                bind(1, Operation::ReadOwnerEvidence),
                bind(2, Operation::Literal(Lit::Unit)),
                bind(
                    3,
                    Operation::WithOwnerEvidence {
                        owner: 2,
                        code: 3,
                        args: vec![0],
                    },
                ),
                bind(4, Operation::ReadOwnerEvidence),
                bind(5, Operation::Literal(Lit::Unit)),
                intrinsic(6, Intrinsic::ScopeCurrent, &[5]),
                bind(
                    7,
                    Operation::Construct {
                        tag: "$tuple".into(),
                        fields: vec![1, 3, 4, 6],
                    },
                ),
            ],
            7,
        ),
        function(
            &[0],
            vec![
                bind(1, Operation::ReadOwnerEvidence),
                bind(2, Operation::Literal(Lit::Unit)),
                intrinsic(3, Intrinsic::ScopeCurrent, &[2]),
                bind(
                    4,
                    Operation::Construct {
                        tag: "$tuple".into(),
                        fields: vec![1, 3],
                    },
                ),
            ],
            4,
        ),
    ];
    Program {
        functions: functions
            .into_iter()
            .enumerate()
            .map(|(i, f)| (u32::try_from(i).expect("code"), f))
            .collect(),
        globals: vec![Global {
            name: entry(),
            initializer: 0,
        }],
    }
}

#[test]
fn nested_absence_restores_evidence_without_replacing_default_owner() {
    let mut heap = Heap::new();
    let ran =
        execute_program(&mut heap, &program(), &entry(), vec![], 1000).expect("evidence program");
    let body = heap.field(&ran.value, 0).expect("scope result");
    let candidate = heap.field(body, 0).expect("outer evidence");
    let default = heap.field(body, 3).expect("default owner");
    assert!(matches!(candidate, Value::Owner(_)));
    assert!(matches!(default, Value::Owner(_)));
    assert_ne!(candidate, default, "evidence must not select default owner");
    assert_eq!(heap.field(body, 2).expect("restored evidence"), candidate);
    let absent = heap.field(body, 1).expect("absence bracket");
    assert_eq!(
        heap.field(absent, 0).expect("explicit absence"),
        &Value::Unit
    );
    assert_eq!(heap.field(absent, 1).expect("unchanged default"), default);
    assert_eq!(
        heap.field(&ran.value, 1).expect("scope exit evidence"),
        &Value::Unit
    );
    heap.drop_value(ran.value).expect("returned root");
    assert_eq!(heap.finish().expect("actual empty heap").live_objects, 0);
}

#[test]
fn every_evidence_fuel_prefix_releases_real_roots_not_external_aliases() {
    let program = program();
    let mut completed = false;
    for fuel in 0..256 {
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("caller".into()))
            .expect("caller root");
        match execute_program(&mut heap, &program, &entry(), vec![], fuel) {
            Ok(ran) => {
                completed = true;
                heap.drop_value(ran.value).expect("outcome");
            }
            Err(error) => assert_eq!(error, Error::OutOfFuel, "fuel={fuel}"),
        }
        assert_eq!(heap.string(&external), Ok("caller"));
        assert_eq!(heap.stats().live_objects, 1);
        heap.drop_value(external).expect("caller releases root");
        let stats = heap.finish().expect("ordinary drops reclaim every slot");
        assert_eq!(stats.allocations, stats.frees);
    }
    assert!(completed);
}

fn handler_program(class: OpClass) -> Program {
    let functions = vec![
        function(
            &[],
            vec![
                bind(
                    0,
                    Operation::Closure {
                        code: 1,
                        captures: vec![],
                    },
                ),
                bind(
                    1,
                    Operation::Closure {
                        code: 2,
                        captures: vec![],
                    },
                ),
                bind(
                    2,
                    Operation::Handle {
                        body: 0,
                        effects: vec![HandlerEffect {
                            effect: "Read".into(),
                            operations: vec![HandlerOperation {
                                name: "read".into(),
                                class,
                                dicts: 0,
                                arity: 1,
                                function: 1,
                            }],
                        }],
                        ret: None,
                        errors: None,
                    },
                ),
            ],
            2,
        ),
        function(
            &[0, 1],
            vec![
                bind(
                    2,
                    Operation::Closure {
                        code: 3,
                        captures: vec![],
                    },
                ),
                intrinsic(3, Intrinsic::ScopeRun, &[2]),
            ],
            3,
        ),
        function(
            &[0, 1],
            vec![
                bind(2, Operation::ReadOwnerEvidence),
                bind(3, Operation::Literal(Lit::Unit)),
                intrinsic(4, Intrinsic::ScopeCurrent, &[3]),
                bind(
                    5,
                    Operation::Construct {
                        tag: "$tuple".into(),
                        fields: vec![1, 2, 4],
                    },
                ),
            ],
            5,
        ),
        function(
            &[0, 1],
            vec![bind(
                2,
                Operation::Perform {
                    effect: "Read".into(),
                    op: "read".into(),
                    dicts: vec![],
                    args: vec![1],
                },
            )],
            2,
        ),
    ];
    Program {
        functions: functions
            .into_iter()
            .enumerate()
            .map(|(i, f)| (u32::try_from(i).expect("code"), f))
            .collect(),
        globals: vec![Global {
            name: entry(),
            initializer: 0,
        }],
    }
}

#[test]
fn tail_and_abort_arms_receive_performer_evidence_not_installation_default() {
    for class in [OpClass::Tail, OpClass::Abort] {
        let program = handler_program(class);
        let mut completed = false;
        for fuel in 0..256 {
            let mut heap = Heap::new();
            match execute_program(&mut heap, &program, &entry(), vec![], fuel) {
                Ok(ran) => {
                    completed = true;
                    let owner = heap.field(&ran.value, 0).expect("performer owner");
                    assert_eq!(
                        heap.field(&ran.value, 1).expect("performer evidence"),
                        owner
                    );
                    assert_ne!(
                        heap.field(&ran.value, 2).expect("installation default"),
                        owner
                    );
                    heap.drop_value(ran.value).expect("release result");
                }
                Err(error) => assert_eq!(error, Error::OutOfFuel, "fuel={fuel}"),
            }
            let stats = heap.finish().expect("all active and parked roots released");
            assert_eq!(stats.allocations, stats.frees);
        }
        assert!(completed);
    }
}
