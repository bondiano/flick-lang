#[test]
fn fuel_unwind_drops_only_execution_roots_not_external_aliases() {
    let p = program(vec![
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
                Operation::Apply {
                    function: 0,
                    argument: 1,
                },
            )],
            2,
        ),
    ]);
    let mut heap = Heap::new();
    let argument = heap
        .literal(&Lit::String("external".into()))
        .expect("argument");
    let external = heap.dup(&argument).expect("caller retains an alias");
    assert!(matches!(
        execute_program(
            &mut heap,
            &p,
            &Name::new("Main", "main"),
            vec![argument],
            30
        ),
        Err(Error::OutOfFuel)
    ));
    assert_eq!(heap.string(&external), Ok("external"));
    assert_eq!(heap.reference_count(&external), Ok(1));
    assert_eq!(heap.stats().live_objects, 1);
    heap.drop_value(external).expect("external root");
    assert!(heap.finish().is_ok());
}

#[test]
fn returned_closures_cannot_rebind_their_code_id_in_another_execution() {
    let first = program(vec![
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
        function(&[0, 1], vec![], 1),
    ]);
    let second = program(vec![
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
            vec![
                bind(2, Operation::Literal(Lit::Unit)),
                bind(
                    3,
                    Operation::Apply {
                        function: 1,
                        argument: 2,
                    },
                ),
            ],
            3,
        ),
    ]);
    let mut heap = Heap::new();
    let closure = execute_program(&mut heap, &first, &Name::new("Main", "main"), vec![], 100)
        .expect("returned closure")
        .value;
    assert!(matches!(
        execute_program(
            &mut heap,
            &second,
            &Name::new("Main", "main"),
            vec![closure],
            100
        ),
        Err(Error::ForeignClosure)
    ));
    assert!(heap.finish().is_ok());
}

#[test]
fn zero_fuel_and_invalid_entry_release_accepted_owned_arguments() {
    let p = program(vec![function(
        &[],
        vec![bind(0, Operation::Literal(Lit::Unit))],
        0,
    )]);
    for missing in [false, true] {
        let mut heap = Heap::new();
        let argument = heap
            .literal(&Lit::String("owned argument".into()))
            .expect("argument");
        let entry = Name::new("Main", if missing { "missing" } else { "main" });
        let error =
            execute_program(&mut heap, &p, &entry, vec![argument], 0).expect_err("not executed");
        assert!(matches!(error, Error::UnknownGlobal(_)) == missing);
        if !missing {
            assert_eq!(error, Error::OutOfFuel);
        }
        assert!(heap.finish().is_ok());
    }
}

#[test]
fn nested_failure_drains_suspended_caller_values_and_callee_scratch() {
    use flick_core::ir::BinOp;
    let p = program(vec![
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
            vec![
                bind(2, Operation::Literal(Lit::Bool(true))),
                bind(
                    3,
                    Operation::Call {
                        code: 2,
                        args: vec![2],
                    },
                ),
                bind(
                    4,
                    Operation::Construct {
                        tag: "Unreachable".into(),
                        fields: vec![1, 3],
                    },
                ),
            ],
            4,
        ),
        function(
            &[0],
            vec![
                bind(1, Operation::Literal(Lit::String("callee scratch".into()))),
                bind(
                    2,
                    Operation::Binary {
                        op: BinOp::Concat,
                        left: 0,
                        right: 1,
                    },
                ),
            ],
            2,
        ),
    ]);
    let mut heap = Heap::new();
    let argument = heap
        .literal(&Lit::String("suspended caller".into()))
        .expect("argument");
    assert!(
        execute_program(
            &mut heap,
            &p,
            &Name::new("Main", "main"),
            vec![argument],
            100
        )
        .is_err()
    );
    assert_eq!(
        heap.stats().allocations,
        3,
        "argument, entry closure, callee string"
    );
    assert_eq!(heap.finish().expect("explicit error unwind").frees, 3);
}

#[test]
fn fuel_unwind_releases_values_retained_by_many_suspended_frames() {
    let p = program(vec![
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
            vec![
                bind(2, Operation::Literal(Lit::String("retained".into()))),
                bind(
                    3,
                    Operation::Apply {
                        function: 0,
                        argument: 1,
                    },
                ),
                bind(
                    4,
                    Operation::Construct {
                        tag: "Unreachable".into(),
                        fields: vec![2, 3],
                    },
                ),
            ],
            4,
        ),
    ]);
    let mut heap = Heap::new();
    assert!(matches!(
        execute_program(
            &mut heap,
            &p,
            &Name::new("Main", "main"),
            vec![Value::Unit],
            1_000
        ),
        Err(Error::OutOfFuel)
    ));
    assert!(
        heap.stats().peak_live_objects > 100,
        "suspended frames really retain their strings"
    );
    assert!(heap.finish().is_ok());
}

#[test]
fn closure_storage_is_never_reused_as_constructor_storage() {
    let p = program(vec![
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
            vec![
                bind(
                    2,
                    Operation::Closure {
                        code: 2,
                        captures: vec![],
                    },
                ),
                // Already annotated reuse is deliberate here; source insertion
                // does not manufacture a closure-to-constructor storage cast.
                bind(
                    3,
                    Operation::Construct {
                        tag: "New".into(),
                        fields: vec![],
                    },
                ),
            ],
            3,
        ),
        function(&[0, 1], vec![], 1),
    ]);
    let mut p = p;
    // Replace the dead local closure's Drop+Construct with explicit Reuse.
    let function = p.functions.get_mut(&1).expect("function");
    let drop = function
        .body
        .instructions
        .iter()
        .position(|i| matches!(i, Instruction::Drop(2)))
        .expect("dead closure drop");
    function.body.instructions.remove(drop);
    let Instruction::Let { value, .. } = function
        .body
        .instructions
        .iter_mut()
        .find(|i| matches!(i, Instruction::Let { binding: 3, .. }))
        .expect("constructor")
    else {
        unreachable!()
    };
    *value = Operation::Reuse {
        object: 2,
        tag: "New".into(),
        fields: vec![],
    };
    let mut heap = Heap::new();
    let ran = execute_program(
        &mut heap,
        &p,
        &Name::new("Main", "main"),
        vec![Value::Unit],
        100,
    )
    .expect("safe fallback");
    assert_eq!(heap.tag(&ran.value), Ok("New"));
    assert_eq!(heap.stats().reuses, 0);
    assert_eq!(heap.stats().allocations, 3);
    heap.drop_value(ran.value).expect("result");
    assert!(heap.finish().is_ok());
}
