//! Explicit shallow array-ABI normalization, without widening list patterns.
#[path = "support/causes.rs"]
pub mod causes;
use flick_core::{
    ir::{
        Lit,
        memory::{Block, Exit, Function, Global, Instruction, Intrinsic, Name, Operation, Program},
    },
    opt::perceus,
};
use flick_model::memory::{Error, Heap, ProgramOutcome, Value, execute_program_outcome};

fn function(params: Vec<u32>, operation: Operation) -> Function {
    perceus::insert(&Function {
        params,
        body: Block {
            instructions: vec![Instruction::Let {
                binding: 2,
                value: operation,
            }],
            exit: Exit::Return(2),
        },
    })
    .expect("ownership insertion")
}

fn program() -> Program {
    Program {
        globals: vec![Global {
            name: Name::new("ArrayObserver", "normalize"),
            initializer: 0,
        }],
        functions: [
            (
                0,
                function(
                    vec![],
                    Operation::Closure {
                        code: 1,
                        captures: vec![],
                    },
                ),
            ),
            (
                1,
                function(
                    vec![0, 1],
                    Operation::Intrinsic {
                        kind: Intrinsic::ArrayToList,
                        args: vec![1],
                    },
                ),
            ),
        ]
        .into(),
    }
}

fn execute(heap: &mut Heap, argument: Value, fuel: u64) -> ProgramOutcome {
    let report = execute_program_outcome(
        heap,
        &program(),
        &Name::new("ArrayObserver", "normalize"),
        vec![argument],
        fuel,
    );
    assert_eq!(report.stdout, "");
    report.outcome
}

#[test]
fn lists_keep_identity_and_tuples_shallowly_retain_arbitrary_width_fields() {
    for tag in ["$list", "$tuple"] {
        for width in [0, 2, 41] {
            let mut heap = Heap::new();
            let element = heap
                .literal(&Lit::String("shared".into()))
                .expect("external element");
            let fields = (0..width)
                .map(|_| heap.dup(&element).expect("container field"))
                .collect();
            let original = heap.construct(tag, fields).expect("external container");
            let argument = heap.dup(&original).expect("owned input");
            let owned_outcome = execute(&mut heap, argument, 10_000);
            let ProgramOutcome::Returned(result) = owned_outcome else {
                causes::unexpected(&mut heap, owned_outcome, "unexpected owned outcome")
            };
            assert_eq!(heap.tag(&result), Ok("$list"));
            assert_eq!(
                result.object_id() == original.object_id(),
                tag == "$list",
                "a list is retained; a tuple is not retagged or mutated"
            );
            assert_eq!(heap.tag(&original), Ok(tag));
            for index in 0..width {
                assert_eq!(
                    heap.field(&result, index)
                        .expect("result field")
                        .object_id(),
                    element.object_id(),
                    "normalization retains, not copies, elements"
                );
                assert_eq!(
                    heap.field(&original, index)
                        .expect("original field")
                        .object_id(),
                    element.object_id()
                );
            }
            heap.drop_value(result).expect("result owner");
            assert_eq!(heap.reference_count(&original), Ok(1));
            assert_eq!(heap.reference_count(&element), Ok(width + 1));
            heap.drop_value(original).expect("external container owner");
            assert_eq!(heap.string(&element), Ok("shared"));
            assert_eq!(heap.reference_count(&element), Ok(1));
            heap.drop_value(element).expect("external element owner");
            let stats = heap.finish().expect("actual zero occupied slots");
            assert_eq!(stats.allocations, stats.frees);
        }
    }
}

#[test]
fn unit_scalars_and_nominal_nonarrays_reject_and_release_the_owned_argument() {
    for case in 0..4 {
        let mut heap = Heap::new();
        let argument = match case {
            0 => Value::Unit,
            1 => Value::Int(2),
            2 => heap
                .literal(&Lit::String("not an array".into()))
                .expect("text"),
            _ => {
                let child = heap.literal(&Lit::String("owned".into())).expect("child");
                heap.construct("App.Box", vec![child]).expect("nominal")
            }
        };
        let outcome = execute(&mut heap, argument, 10_000);
        let correct = matches!(&outcome, ProgramOutcome::Fault(Error::UnsupportedPrimitive(message))
            if message == "array observer requires a list or tuple");
        outcome
            .release(&mut heap)
            .expect("every unexpected owned outcome releases");
        assert!(correct);
        let stats = heap.finish().expect("rejected argument is still consumed");
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn every_fuel_prefix_releases_partial_normalization_without_touching_external_roots() {
    let mut prefixes = 0;
    for tag in ["$list", "$tuple"] {
        for width in [0, 2, 41] {
            let mut completed = false;
            for fuel in 0..=10_000 {
                let mut heap = Heap::new();
                let element = heap
                    .literal(&Lit::String("outside".into()))
                    .expect("external element");
                let fields = (0..width)
                    .map(|_| heap.dup(&element).expect("container field"))
                    .collect();
                let original = heap.construct(tag, fields).expect("external container");
                let argument = heap.dup(&original).expect("owned input");
                match execute(&mut heap, argument, fuel) {
                    ProgramOutcome::Returned(result) => {
                        assert_eq!(heap.tag(&result), Ok("$list"));
                        assert_eq!(result.object_id() == original.object_id(), tag == "$list");
                        heap.drop_value(result).expect("returned owner");
                        completed = true;
                    }
                    ProgramOutcome::Fault(Error::OutOfFuel) => {}
                    outcome => {
                        let diagnostic = format!("{tag} width={width} fuel={fuel}: {outcome:?}");
                        causes::unexpected(&mut heap, outcome, &diagnostic)
                    }
                }
                assert_eq!(heap.tag(&original), Ok(tag));
                assert_eq!(heap.reference_count(&original), Ok(1));
                assert_eq!(heap.reference_count(&element), Ok(width + 1));
                heap.drop_value(original)
                    .expect("external container release");
                assert_eq!(heap.string(&element), Ok("outside"));
                heap.drop_value(element).expect("external element release");
                let stats = heap.finish().expect("all prefix roots released, no sweep");
                assert_eq!(stats.allocations, stats.frees);
                prefixes += 1;
                if completed {
                    eprintln!("{tag} width={width} first terminal fuel={fuel}");
                    break;
                }
            }
            assert!(completed, "the ordinary fuel cap reaches completion");
        }
    }
    eprintln!("checked {prefixes} raw array-normalization fuel prefixes");
}
