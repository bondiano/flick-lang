//! Scoped arenas own their slots independently of retained nonowning views.
use flick_core::ir::Lit;
use flick_core::ir::memory::{
    Block, Exit, Function, Global, Instruction, Intrinsic, Name, Operation, Program,
};
use flick_core::opt::perceus;
use flick_model::memory::{Error, Heap, execute_program};

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
    .expect("valid arena test operation")
}
fn program(bodies: Vec<Function>) -> Program {
    Program {
        functions: bodies
            .into_iter()
            .enumerate()
            .map(|(i, f)| (u32::try_from(i).expect("valid arena test operation"), f))
            .collect(),
        globals: vec![Global {
            name: Name::new("Main", "main"),
            initializer: 0,
        }],
    }
}
fn scope_program(after: Vec<Instruction>, result: u32, read_inside: bool) -> Program {
    let mut main = vec![
        bind(
            0,
            Operation::Closure {
                code: 1,
                captures: vec![],
            },
        ),
        intrinsic(1, Intrinsic::ScopeRun, &[0]),
    ];
    main.extend(after);
    let mut body = vec![
        bind(2, Operation::Literal(Lit::Unit)),
        intrinsic(3, Intrinsic::ArenaMake, &[2]),
        bind(4, Operation::Literal(Lit::String("owned".into()))),
        intrinsic(5, Intrinsic::ArenaAlloc, &[3, 4]),
    ];
    if read_inside {
        body.push(intrinsic(6, Intrinsic::ArenaGet, &[5]));
    }
    program(vec![
        function(&[], main, result),
        function(&[0, 1], body, if read_inside { 6 } else { 5 }),
    ])
}
#[test]
fn retained_view_is_safe_to_drop_but_storage_is_released() {
    let p = scope_program(vec![], 1, false);
    let mut heap = Heap::new();
    let ran = execute_program(&mut heap, &p, &Name::new("Main", "main"), vec![], 200)
        .expect("valid arena test operation");
    assert_eq!(heap.stats().arenas_allocated, 1);
    assert_eq!(heap.stats().arenas_freed, 1);
    assert_eq!(
        heap.stats().live_objects,
        0,
        "retained view owns no arena storage"
    );
    heap.drop_value(ran.value)
        .expect("valid arena test operation");
    heap.finish().expect("valid arena test operation");
}
#[test]
fn reading_after_scope_close_rejects_stale_view() {
    let p = scope_program(vec![intrinsic(2, Intrinsic::ArenaGet, &[1])], 2, false);
    let mut heap = Heap::new();
    assert_eq!(
        execute_program(&mut heap, &p, &Name::new("Main", "main"), vec![], 200)
            .expect_err("invalid arena operation"),
        Error::ClosedArena
    );
    heap.finish().expect("valid arena test operation");
}
#[test]
fn read_retains_contents_past_arena_close() {
    let p = scope_program(vec![], 1, true);
    let mut heap = Heap::new();
    let ran = execute_program(&mut heap, &p, &Name::new("Main", "main"), vec![], 200)
        .expect("valid arena test operation");
    assert_eq!(heap.string(&ran.value), Ok("owned"));
    assert_eq!(heap.stats().arenas_freed, 1);
    assert_eq!(heap.stats().live_objects, 1);
    heap.drop_value(ran.value)
        .expect("valid arena test operation");
    heap.finish().expect("valid arena test operation");
}
#[test]
fn every_fuel_cut_releases_actual_scope_roots() {
    let p = scope_program(vec![], 1, false);
    for fuel in 0..80 {
        let mut heap = Heap::new();
        match execute_program(&mut heap, &p, &Name::new("Main", "main"), vec![], fuel) {
            Ok(ran) => heap
                .drop_value(ran.value)
                .expect("valid arena test operation"),
            Err(error) => assert_eq!(error, Error::OutOfFuel),
        }
        assert_eq!(heap.stats().arenas_allocated, heap.stats().arenas_freed);
        heap.finish().expect("valid arena test operation");
    }
}

fn selected_owner_program(field: usize) -> Program {
    program(vec![
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
                intrinsic(1, Intrinsic::ScopeRun, &[0]),
            ],
            1,
        ),
        function(
            &[0, 1],
            vec![
                bind(
                    2,
                    Operation::Closure {
                        code: 2,
                        captures: vec![1],
                    },
                ),
                intrinsic(3, Intrinsic::ScopeRun, &[2]),
                bind(4, Operation::Project { object: 3, field }),
                intrinsic(5, Intrinsic::ArenaGet, &[4]),
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
                    Operation::Within {
                        owner: 2,
                        code: 3,
                        args: vec![],
                    },
                ),
                bind(4, Operation::Literal(Lit::Unit)),
                intrinsic(5, Intrinsic::ArenaMake, &[4]),
                bind(6, Operation::Literal(Lit::String("inner".into()))),
                intrinsic(7, Intrinsic::ArenaAlloc, &[5, 6]),
                bind(
                    8,
                    Operation::Construct {
                        tag: "$tuple".into(),
                        fields: vec![3, 7],
                    },
                ),
            ],
            8,
        ),
        function(
            &[],
            vec![
                bind(0, Operation::Literal(Lit::Unit)),
                intrinsic(1, Intrinsic::ArenaMake, &[0]),
                bind(2, Operation::Literal(Lit::String("outer".into()))),
                intrinsic(3, Intrinsic::ArenaAlloc, &[1, 2]),
            ],
            3,
        ),
    ])
}
#[test]
fn selected_outer_owner_survives_inner_close_and_context_is_restored() {
    for field in [0, 1] {
        let mut heap = Heap::new();
        let ran = execute_program(
            &mut heap,
            &selected_owner_program(field),
            &Name::new("Main", "main"),
            vec![],
            500,
        );
        if field == 0 {
            let ran = ran.expect("valid arena test operation");
            assert_eq!(heap.string(&ran.value), Ok("outer"));
            heap.drop_value(ran.value)
                .expect("valid arena test operation");
        } else {
            assert_eq!(
                ran.expect_err("invalid arena operation"),
                Error::ClosedArena,
                "unselected acquisition belongs to inner owner after Within returns"
            );
        }
        assert_eq!(heap.stats().arenas_allocated, 2);
        assert_eq!(heap.stats().arenas_freed, 2);
        heap.finish().expect("valid arena test operation");
    }
}
#[test]
fn ordinary_ref_cells_outlive_inner_scopes() {
    let p = program(vec![
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
                intrinsic(1, Intrinsic::ScopeRun, &[0]),
                intrinsic(2, Intrinsic::RefGet, &[1]),
            ],
            2,
        ),
        function(
            &[0, 1],
            vec![
                bind(2, Operation::Literal(Lit::String("root cell".into()))),
                intrinsic(3, Intrinsic::RefMake, &[2]),
            ],
            3,
        ),
    ]);
    let mut heap = Heap::new();
    let ran = execute_program(&mut heap, &p, &Name::new("Main", "main"), vec![], 200)
        .expect("valid arena test operation");
    assert_eq!(heap.string(&ran.value), Ok("root cell"));
    assert_eq!(heap.stats().cells_allocated, 1);
    assert_eq!(heap.stats().cells_freed, 1);
    heap.drop_value(ran.value)
        .expect("valid arena test operation");
    heap.finish().expect("valid arena test operation");
}

#[test]
fn returned_views_and_owners_cannot_be_used_in_another_execution() {
    let mut heap = Heap::new();
    let view = execute_program(
        &mut heap,
        &scope_program(vec![], 1, false),
        &Name::new("Main", "main"),
        vec![],
        200,
    )
    .expect("retained closed view")
    .value;
    let get = program(vec![
        function(
            &[],
            vec![bind(
                0,
                Operation::Closure {
                    code: 1,
                    captures: vec![],
                },
            )],
            0,
        ),
        function(&[0, 1], vec![intrinsic(2, Intrinsic::ArenaGet, &[1])], 2),
    ]);
    assert_eq!(
        execute_program(&mut heap, &get, &Name::new("Main", "main"), vec![view], 200)
            .expect_err("cross-execution view"),
        Error::ForeignArena
    );
    heap.finish().expect("closed view owns nothing");

    let owner_source = program(vec![function(
        &[],
        vec![
            bind(0, Operation::Literal(Lit::Unit)),
            intrinsic(1, Intrinsic::ScopeCurrent, &[0]),
        ],
        1,
    )]);
    let owner = execute_program(
        &mut heap,
        &owner_source,
        &Name::new("Main", "main"),
        vec![],
        200,
    )
    .expect("root owner capability")
    .value;
    let select = program(vec![
        function(
            &[],
            vec![bind(
                0,
                Operation::Closure {
                    code: 1,
                    captures: vec![],
                },
            )],
            0,
        ),
        function(
            &[0, 1],
            vec![bind(
                2,
                Operation::Within {
                    owner: 1,
                    code: 2,
                    args: vec![],
                },
            )],
            2,
        ),
        function(&[], vec![bind(0, Operation::Literal(Lit::Unit))], 0),
    ]);
    assert_eq!(
        execute_program(
            &mut heap,
            &select,
            &Name::new("Main", "main"),
            vec![owner],
            200
        )
        .expect_err("cross-execution owner"),
        Error::ForeignOwner
    );
    heap.finish()
        .expect("foreign capability argument released safely");
}

#[test]
fn a_closed_owner_cannot_receive_new_acquisitions() {
    let p = program(vec![
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
                intrinsic(1, Intrinsic::ScopeRun, &[0]),
                bind(
                    2,
                    Operation::Within {
                        owner: 1,
                        code: 2,
                        args: vec![],
                    },
                ),
            ],
            2,
        ),
        function(&[0, 1], vec![], 1),
        function(
            &[],
            vec![
                bind(0, Operation::Literal(Lit::Unit)),
                intrinsic(1, Intrinsic::ArenaMake, &[0]),
            ],
            1,
        ),
    ]);
    let mut heap = Heap::new();
    assert_eq!(
        execute_program(&mut heap, &p, &Name::new("Main", "main"), vec![], 200)
            .expect_err("retired owner rejects actual acquisition"),
        Error::ClosedOwner
    );
    let stats = heap
        .finish()
        .expect("closed owner argument safely released");
    assert_eq!(stats.arenas_allocated, 0);
    assert_eq!(stats.live_objects, 0);
    assert_eq!(stats.allocations, stats.frees);
}
#[test]
fn runtime_failure_inside_a_scope_releases_its_owned_slots() {
    let p = program(vec![
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
                intrinsic(1, Intrinsic::ScopeRun, &[0]),
            ],
            1,
        ),
        function(
            &[0, 1],
            vec![
                bind(2, Operation::Literal(Lit::Unit)),
                intrinsic(3, Intrinsic::ArenaMake, &[2]),
                bind(4, Operation::Literal(Lit::String("failed scope".into()))),
                intrinsic(5, Intrinsic::ArenaAlloc, &[3, 4]),
                bind(6, Operation::Not(3)),
            ],
            6,
        ),
    ]);
    let mut heap = Heap::new();
    assert_eq!(
        execute_program(&mut heap, &p, &Name::new("Main", "main"), vec![], 200)
            .expect_err("invalid primitive input"),
        Error::ExpectedBoolean
    );
    assert_eq!(heap.stats().arenas_allocated, 1);
    assert_eq!(heap.stats().arenas_freed, 1);
    heap.finish()
        .expect("error unwound actual scope and frame roots");
}
