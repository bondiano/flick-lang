#[test]
fn constant_provider_returns_owned_field_and_finishes_without_sweep() {
    let mut heap = Heap::new();
    let report = execute_program_outcome(&mut heap, &constant_layer(), &entry(), vec![], 1000);
    {
        let diagnostic = format!("{:?}", report.outcome);
        let observed_outcome = report.outcome;
        let correct = matches!(&observed_outcome, ProgramOutcome::Returned(Value::Int(42)));
        let detail = format!("{observed_outcome:?}");
        observed_outcome
            .release(&mut heap)
            .expect("every owned outcome released before assertion");
        assert!(correct, "{diagnostic}: {detail}");
    };
    heap.finish().expect("all provider and tag roots released");
}

#[test]
fn every_fuel_cut_drains_provider_roots_but_preserves_external_aliases() {
    let program = constant_layer();
    let mut completed = false;
    for fuel in 0..100 {
        let mut heap = Heap::new();
        let alias = heap
            .service_tag("Service")
            .expect("valid counted Layer test operation");
        let identity = alias
            .object_id()
            .expect("valid counted Layer test operation");
        let report = execute_program_outcome(&mut heap, &program, &entry(), vec![], fuel);
        match report.outcome {
            ProgramOutcome::Returned(value) => {
                assert!(matches!(value, Value::Int(42)));
                completed = true;
                heap.drop_value(value)
                    .expect("valid counted Layer test operation");
            }
            ProgramOutcome::Fault(Error::OutOfFuel) => {}
            other => {
                let diagnostic = format!("fuel {fuel}: {other:?}");
                causes::unexpected(&mut heap, other, &diagnostic)
            }
        }
        assert!(heap.is_live(identity));
        let second = heap
            .service_tag("Service")
            .expect("valid counted Layer test operation");
        assert_eq!(second.object_id(), Some(identity));
        heap.drop_value(second)
            .expect("valid counted Layer test operation");
        heap.drop_value(alias)
            .expect("valid counted Layer test operation");
        heap.finish().expect("valid counted Layer test operation");
    }
    assert!(completed, "sweep includes successful executions");
}

#[test]
fn nominal_tags_retire_reallocate_and_are_heap_local() {
    let mut heap = Heap::new();
    let a = heap
        .service_tag("__proto__")
        .expect("valid counted Layer test operation");
    let first = a.object_id().expect("valid counted Layer test operation");
    let b = heap
        .service_tag("__proto__")
        .expect("valid counted Layer test operation");
    assert_eq!(a.object_id(), b.object_id());
    heap.drop_value(a)
        .expect("valid counted Layer test operation");
    assert!(heap.is_live(first));
    heap.drop_value(b)
        .expect("valid counted Layer test operation");
    assert!(!heap.is_live(first));
    heap.finish().expect("valid counted Layer test operation");
    let c = heap
        .service_tag("__proto__")
        .expect("valid counted Layer test operation");
    assert_ne!(c.object_id(), Some(first));
    let mut foreign = Heap::new();
    assert!(foreign.dup(&c).is_err());
    foreign
        .finish()
        .expect("valid counted Layer test operation");
    heap.drop_value(c)
        .expect("valid counted Layer test operation");
    heap.finish().expect("valid counted Layer test operation");
}

fn layer_with_callback(callback: Function, implementation: Operation) -> Program {
    program(vec![
        function(
            &[],
            vec![
                bind(0, Operation::ServiceTag("Service".into())),
                bind(1, implementation),
                intrinsic(2, Intrinsic::LayerSucceed, &[0, 1]),
                closure(3, 1),
                intrinsic(4, Intrinsic::LayerRun, &[2, 3]),
            ],
            4,
        ),
        callback,
    ])
}
#[test]
fn callback_receives_real_unit_and_layer_is_reusable() {
    let p = program(vec![
        function(
            &[],
            vec![
                bind(0, Operation::ServiceTag("Service".into())),
                bind(1, Operation::Literal(Lit::Int(1))),
                intrinsic(2, Intrinsic::LayerSucceed, &[0, 1]),
                closure(3, 1),
                intrinsic(4, Intrinsic::LayerRun, &[2, 3]),
                intrinsic(5, Intrinsic::LayerRun, &[2, 3]),
            ],
            5,
        ),
        function(&[0, 1], vec![], 1),
    ]);
    let mut heap = Heap::new();
    {
        let diagnostic = "exact outcome expectation";
        let observed_outcome =
            execute_program_outcome(&mut heap, &p, &entry(), vec![], 1000).outcome;
        let correct = matches!(&observed_outcome, ProgramOutcome::Returned(Value::Unit));
        let detail = format!("{observed_outcome:?}");
        observed_outcome
            .release(&mut heap)
            .expect("every owned outcome released before assertion");
        assert!(correct, "{diagnostic}: {detail}");
    };
    heap.finish().expect("valid counted Layer test operation");
}
#[test]
fn matching_provider_is_complete_even_for_malformed_implementation() {
    let callback = function(
        &[0, 1],
        vec![bind(
            2,
            Operation::Perform {
                effect: "Service".into(),
                op: "missing".into(),
                dicts: vec![],
                args: vec![],
            },
        )],
        2,
    );
    let p = layer_with_callback(callback, Operation::Literal(Lit::Unit));
    let mut heap = Heap::new();
    {
        let diagnostic = "exact outcome expectation";
        let observed_outcome =
            execute_program_outcome(&mut heap, &p, &entry(), vec![], 1000).outcome;
        let correct = matches!(
            &observed_outcome,
            ProgramOutcome::Fault(Error::ExpectedRecord)
        );
        let detail = format!("{observed_outcome:?}");
        observed_outcome
            .release(&mut heap)
            .expect("every owned outcome released before assertion");
        assert!(correct, "{diagnostic}: {detail}");
    };
    heap.finish().expect("valid counted Layer test operation");
}
#[test]
fn layer_is_opaque_and_does_not_reuse_as_constructor_storage() {
    let mut p = constant_layer();
    p.functions.insert(
        0,
        function(
            &[],
            vec![
                bind(0, Operation::ServiceTag("Service".into())),
                bind(1, Operation::Literal(Lit::String("owned".into()))),
                intrinsic(2, Intrinsic::LayerSucceed, &[0, 1]),
            ],
            2,
        ),
    );
    let mut heap = Heap::new();
    let layer = match execute_program_outcome(&mut heap, &p, &entry(), vec![], 1000).outcome {
        ProgramOutcome::Returned(v) => v,
        other => causes::unexpected(&mut heap, other, "unexpected owned outcome"),
    };
    assert!(heap.project(&layer, 0).is_err());
    assert!(heap.project_named(&layer, "_tag").is_err());
    let identity = layer
        .object_id()
        .expect("valid counted Layer test operation");
    let reused = heap
        .reuse(layer, "Box", vec![])
        .expect("valid counted Layer test operation");
    assert_ne!(reused.object_id(), Some(identity));
    heap.drop_value(reused)
        .expect("valid counted Layer test operation");
    heap.finish().expect("valid counted Layer test operation");
}

fn perform(binding: Local, effect: &str, op: &str, dicts: &[Local], args: &[Local]) -> Instruction {
    bind(
        binding,
        Operation::Perform {
            effect: effect.into(),
            op: op.into(),
            dicts: dicts.to_vec(),
            args: args.to_vec(),
        },
    )
}
fn handle(
    binding: Local,
    body: Local,
    effect: &str,
    operation: &str,
    method: Local,
) -> Instruction {
    bind(
        binding,
        Operation::Handle {
            body,
            effects: vec![HandlerEffect {
                effect: effect.into(),
                operations: vec![HandlerOperation {
                    name: operation.into(),
                    class: OpClass::Tail,
                    dicts: 0,
                    arity: 1,
                    function: method,
                }],
            }],
            ret: None,
            errors: None,
        },
    )
}
fn run_int(p: &Program, expected: i32) {
    let mut heap = Heap::new();
    let report = execute_program_outcome(&mut heap, p, &entry(), vec![], 1000);
    {
        let diagnostic = format!("{:?}", report.outcome);
        let observed_outcome = report.outcome;
        let correct =
            matches!(&observed_outcome, ProgramOutcome::Returned(Value::Int(n)) if *n ==expected);
        let detail = format!("{observed_outcome:?}");
        observed_outcome
            .release(&mut heap)
            .expect("every owned outcome released before assertion");
        assert!(correct, "{diagnostic}: {detail}");
    };
    heap.finish().expect("valid counted Layer test operation");
}
