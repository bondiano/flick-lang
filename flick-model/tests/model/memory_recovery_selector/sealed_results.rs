#[test]
fn ordinary_selector_value_is_not_an_implicit_selected_body_result() {
    let mut heap = Heap::new();
    let report = execute_program_outcome(
        &mut heap,
        &ordinary_selector(),
        &Name::new("Recovery", "main"),
        vec![],
        1000,
    );
    let expected = matches!(
        &report.outcome,
        ProgramOutcome::Fault(Error::ExpectedRecoveryResult)
    );
    let actual = format!("{:?}", report.outcome);
    report
        .outcome
        .release(&mut heap)
        .expect("every owned outcome released");
    let stats = heap.finish().expect("actual no occupied roots");
    assert_eq!(stats.allocations, stats.frees);
    assert!(
        expected,
        "ordinary unsealed selector return was wrongly accepted as selected result: {actual}; actualheap0"
    );
}

fn sealed_selector(hit: bool) -> Program {
    let mut program = ordinary_selector();
    let mut instructions = vec![];
    let selected = hit.then(|| {
        instructions.push(bind(
            2,
            Operation::Closure {
                code: 3,
                captures: vec![],
            },
        ));
        2
    });
    instructions.push(bind(3, Operation::RecoveryResult { selected }));
    program
        .functions
        .insert(2, function(&[0, 1], instructions, 3));
    program.functions.insert(
        3,
        function(&[0, 1], vec![bind(2, Operation::Literal(Lit::Int(42)))], 2),
    );
    program
}

#[test]
fn sealed_hit_runs_deferred_body_and_miss_preserves_original_payload() {
    for hit in [false, true] {
        let mut heap = Heap::new();
        let report = execute_program_outcome(
            &mut heap,
            &sealed_selector(hit),
            &Name::new("Recovery", "main"),
            vec![],
            1000,
        );
        match &report.outcome {
            ProgramOutcome::Returned(Value::Int(42)) if hit => {}
            ProgramOutcome::Raised(value) if !hit => {
                assert_eq!(heap.string(value).expect("original string"), "original");
            }
            outcome => {
                let diagnostic = format!("wrong sealed result: {outcome:?}");
                causes::unexpected(&mut heap, report.outcome, &diagnostic)
            }
        }
        report.outcome.release(&mut heap).expect("owned outcome");
        let stats = heap.finish().expect("actual empty heap");
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn sealed_results_have_no_authority_outside_executing_selection() {
    let mut program = ordinary_selector();
    program.functions.insert(
        0,
        function(
            &[],
            vec![bind(0, Operation::RecoveryResult { selected: None })],
            0,
        ),
    );
    let mut heap = Heap::new();
    let report = execute_program_outcome(
        &mut heap,
        &program,
        &Name::new("Recovery", "main"),
        vec![],
        1000,
    );
    assert!(
        matches!(
            report.outcome,
            ProgramOutcome::Fault(Error::RecoveryOutsideSelection)
        ),
        "{:?}",
        report.outcome
    );
    report.outcome.release(&mut heap).expect("owned outcome");
    heap.finish().expect("actual empty heap");
}

#[test]
fn sealed_selection_every_fuel_prefix_preserves_external_owned_alias() {
    for hit in [false, true] {
        let program = sealed_selector(hit);
        let mut completed = false;
        for fuel in 0..1000 {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("external".into()))
                .expect("external owned root");
            let report = execute_program_outcome(
                &mut heap,
                &program,
                &Name::new("Recovery", "main"),
                vec![],
                fuel,
            );
            completed = !matches!(report.outcome, ProgramOutcome::Fault(Error::OutOfFuel));
            assert_eq!(
                heap.string(&external).expect("external survives"),
                "external"
            );
            if completed {
                assert!(
                    matches!(
                        report.outcome,
                        ProgramOutcome::Returned(Value::Int(42)) | ProgramOutcome::Raised(_)
                    ),
                    "{:?}",
                    report.outcome
                );
            }
            report.outcome.release(&mut heap).expect("owned outcome");
            heap.drop_value(external).expect("external caller release");
            let stats = heap.finish().expect("actual empty heap for each prefix");
            assert_eq!(stats.allocations, stats.frees);
            if completed {
                break;
            }
        }
        assert!(
            completed,
            "reached actual result, not a fixed prefix budget"
        );
    }
}

fn forbidden_marker_edges() -> Vec<Operation> {
    use flick_core::{ir::memory::DictionaryEntry, numeric::Num};
    vec![
        Operation::DropReuse(2),
        Operation::Reuse {
            object: 2,
            tag: "Box".into(),
            fields: vec![],
        },
        Operation::ReuseWithDicts {
            object: 2,
            tag: "Box".into(),
            fields: vec![],
            dicts: vec![],
        },
        Operation::ReuseRecord {
            object: 2,
            fields: vec![("value".into(), 1)],
        },
        Operation::ReuseRecord {
            object: 1,
            fields: vec![("value".into(), 2)],
        },
        Operation::Update {
            object: 1,
            fields: vec![("field".into(), 2)],
        },
        Operation::Neg {
            num: Num::BigInt,
            value: 2,
        },
        Operation::Construct {
            tag: "Box".into(),
            fields: vec![2],
        },
        Operation::ConstructWithDicts {
            tag: "Box".into(),
            fields: vec![],
            dicts: vec![2],
        },
        Operation::Record {
            fields: vec![("value".into(), 2)],
        },
        Operation::Dictionary {
            entries: vec![("value".into(), DictionaryEntry::Value(2))],
        },
        Operation::Dictionary {
            entries: vec![("get".into(), DictionaryEntry::Getter(2))],
        },
        Operation::Closure {
            code: 3,
            captures: vec![2],
        },
        Operation::Task {
            code: 3,
            captures: vec![2],
        },
    ]
}

fn forbidden_marker_operations() -> Vec<Operation> {
    use flick_core::ir::{BinOp, Pat};
    let mut operations = forbidden_marker_edges();
    operations.extend([
        Operation::Project {
            object: 2,
            field: 0,
        },
        Operation::Field {
            object: 2,
            field: "value".into(),
        },
        Operation::Method {
            dictionary: 2,
            field: "method".into(),
        },
        Operation::Structure {
            object: 2,
            expected: 0,
        },
        Operation::Match {
            object: 2,
            pattern: Box::new(Pat::Wild),
        },
        Operation::Match {
            object: 2,
            pattern: Box::new(Pat::Bind("bound".into())),
        },
        Operation::Call {
            code: 4,
            args: vec![2],
        },
        Operation::Apply {
            function: 2,
            argument: 2,
        },
        Operation::Await(2),
        Operation::Not(2),
        Operation::Handle {
            body: 2,
            effects: vec![],
            ret: None,
            errors: None,
        },
        Operation::Perform {
            effect: "Missing".into(),
            op: "op".into(),
            dicts: vec![2],
            args: vec![],
        },
        Operation::RecoveryResult { selected: Some(2) },
        Operation::Within {
            owner: 2,
            code: 4,
            args: vec![],
        },
        Operation::WithOwnerEvidence {
            owner: 2,
            code: 4,
            args: vec![],
        },
    ]);
    for op in [BinOp::Eq, BinOp::NotEq, BinOp::Same, BinOp::NotSame] {
        for (left, right) in [(2, 2), (2, 1), (1, 2)] {
            operations.push(Operation::Binary { op, left, right });
        }
    }
    // Both one- and two-input intrinsics must reject before their early
    // dictionary/effect/layout dispatch or primitive equality shortcuts.
    for kind in [
        Intrinsic::Show,
        Intrinsic::RefMake,
        Intrinsic::RefGet,
        Intrinsic::ArenaMake,
        Intrinsic::ArenaGet,
        Intrinsic::Raise,
        Intrinsic::FromResult,
        Intrinsic::PrimitiveHash,
        Intrinsic::ToBigInt,
        Intrinsic::RefSet,
        Intrinsic::RefModify,
        Intrinsic::RefSame,
        Intrinsic::ArenaAlloc,
        Intrinsic::Try,
        Intrinsic::LayerSucceed,
        Intrinsic::LayerRun,
        Intrinsic::LayerMerge,
        Intrinsic::PrimitiveCompare,
    ] {
        operations.push(Operation::Intrinsic {
            kind,
            args: vec![2; kind.arity()],
        });
    }
    operations
}
