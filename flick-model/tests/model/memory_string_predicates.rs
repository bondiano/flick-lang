//! Raw counted predicate semantics and actual-root cleanup on every outcome.
use crate::causes;
use flick_core::ir::{
    Lit,
    memory::{Block, Exit, Function, Global, Instruction, Intrinsic, Name, Operation, Program},
};
use flick_core::opt::perceus;
use flick_model::memory::{Error, Heap, ProgramOutcome, Value, execute_program_outcome};

fn bind(binding: u32, value: Operation) -> Instruction {
    Instruction::Let { binding, value }
}
fn function(params: &[u32], instructions: Vec<Instruction>, result: u32) -> Function {
    perceus::insert(&Function {
        params: params.to_vec(),
        body: Block {
            instructions,
            exit: Exit::Return(result),
        },
    })
    .expect("ownership insertion")
}
fn program(kind: Intrinsic) -> Program {
    Program {
        globals: vec![Global {
            name: Name::new("Strings", "test"),
            initializer: 0,
        }],
        functions: [
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
                    Operation::Closure {
                        code: 2,
                        captures: vec![1],
                    },
                )],
                2,
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
                    bind(3, Operation::Literal(Lit::String("before".into()))),
                    bind(
                        4,
                        Operation::Intrinsic {
                            kind: Intrinsic::PrintLine,
                            args: vec![3],
                        },
                    ),
                    bind(
                        5,
                        Operation::Intrinsic {
                            kind,
                            args: vec![2, 1],
                        },
                    ),
                ],
                5,
            ),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, f)| (u32::try_from(i).expect("small"), f))
        .collect(),
    }
}

#[test]
fn exact_subject_last_predicates_have_independent_expected_booleans() {
    for (needle, subject, starts, includes) in [
        ("", "", true, true),
        ("", "abc", true, true),
        ("a", "", false, false),
        ("ab", "abc", true, true),
        ("bc", "abc", false, true),
        ("abc", "ab", false, false),
        ("😀", "😀x", true, true),
        ("😀", "x😀", false, true),
        ("😁", "😀", false, false),
        ("😀x", "😀y", false, false),
        ("e\u{301}", "e\u{301}x", true, true),
        ("é", "e\u{301}", false, false),
        ("\u{301}", "e\u{301}", false, true),
        ("中", "文中文", false, true),
        ("\0", "x\0y", false, true),
    ] {
        for (kind, expected) in [
            (Intrinsic::StringStartsWith, starts),
            (Intrinsic::StringIncludes, includes),
        ] {
            let mut heap = Heap::new();
            let needle = heap.literal(&Lit::String(needle.into())).expect("needle");
            let outside = heap.literal(&Lit::String(subject.into())).expect("subject");
            let subject_arg = heap.dup(&outside).expect("external alias");
            let report = execute_program_outcome(
                &mut heap,
                &program(kind),
                &Name::new("Strings", "test"),
                vec![needle, subject_arg],
                1_000,
            );
            assert_eq!(report.stdout, "before\n");
            let ProgramOutcome::Returned(value) = report.outcome else {
                causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
            };
            assert!(matches!(value, Value::Bool(actual) if actual == expected));
            assert_eq!(heap.string(&outside), Ok(subject));
            heap.drop_value(value).expect("immediate result");
            heap.drop_value(outside).expect("external alias");
            let stats = heap.finish().expect("no execution roots remain");
            assert_eq!(stats.live_objects, 0);
            assert_eq!(stats.allocations, stats.frees);
        }
    }
}

#[test]
fn wrong_types_in_either_position_and_every_fuel_prefix_release_only_owned_roots() {
    for kind in [Intrinsic::StringStartsWith, Intrinsic::StringIncludes] {
        let program = program(kind);
        for bad_position in [0, 1] {
            let mut heap = Heap::new();
            let outside = heap
                .literal(&Lit::String("outside".into()))
                .expect("external root");
            let alias = heap.dup(&outside).expect("owned argument alias");
            let wrong = heap
                .literal(&Lit::BigInt("7".into()))
                .expect("owned wrong-type argument");
            let args = if bad_position == 0 {
                vec![wrong, alias]
            } else {
                vec![alias, wrong]
            };
            let report = execute_program_outcome(
                &mut heap,
                &program,
                &Name::new("Strings", "test"),
                args,
                1_000,
            );
            {
                let diagnostic = "exact outcome expectation";
                let observed_outcome = report.outcome;
                let correct = matches!(
                    &observed_outcome,
                    ProgramOutcome::Fault(Error::ExpectedString)
                );
                let detail = format!("{observed_outcome:?}");
                observed_outcome
                    .release(&mut heap)
                    .expect("every owned outcome released before assertion");
                assert!(correct, "{diagnostic}: {detail}");
            };
            assert_eq!(report.stdout, "before\n");
            assert_eq!(heap.string(&outside), Ok("outside"));
            heap.drop_value(outside)
                .expect("outside root survives fault");
            assert_eq!(heap.finish().expect("fault cleanup").live_objects, 0);
        }
        let mut exhausted_after_print = false;
        let mut completed = false;
        for fuel in 0..100 {
            let mut heap = Heap::new();
            let outside = heap
                .literal(&Lit::String("subject".into()))
                .expect("outside root");
            let needle = heap.literal(&Lit::String("sub".into())).expect("needle");
            let subject = heap.dup(&outside).expect("alias");
            let report = execute_program_outcome(
                &mut heap,
                &program,
                &Name::new("Strings", "test"),
                vec![needle, subject],
                fuel,
            );
            match report.outcome {
                ProgramOutcome::Fault(Error::OutOfFuel) => {
                    assert!(matches!(report.stdout.as_str(), "" | "before\n"));
                    exhausted_after_print |= report.stdout == "before\n";
                }
                ProgramOutcome::Returned(value) => {
                    assert!(matches!(value, Value::Bool(true)));
                    assert_eq!(report.stdout, "before\n");
                    heap.drop_value(value).expect("result");
                    completed = true;
                }
                other => {
                    let diagnostic = format!("unexpected fuel outcome: {other:?}");
                    causes::unexpected(&mut heap, other, &diagnostic)
                }
            }
            assert_eq!(heap.string(&outside), Ok("subject"));
            heap.drop_value(outside)
                .expect("outside survives every prefix");
            let stats = heap.finish().expect("fuel prefix releases actual roots");
            assert_eq!(stats.live_objects, 0);
            assert_eq!(stats.allocations, stats.frees);
        }
        assert!(exhausted_after_print && completed);
    }
}
