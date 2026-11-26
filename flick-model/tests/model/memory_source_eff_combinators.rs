//! Checked source exercises genuine cold language-value adapters.
use crate::causes;
use flick_core::{
    build,
    ir::{
        ItemKind, Lit,
        memory::{Name, Program},
    },
    opt::perceus,
};
use flick_model::run;
use flick_vm::tier0::{Error, Heap, ProgramOutcome, Value, execute_program_outcome};
const DECLARATIONS: &str = r#"pure_eff : a^c -> (Eff r e a)^c
foreign pure pure_eff from "flick:rt" "succeed"
  retention call

flat_map_eff : (forall v. a^v -> (Eff r e b)^(c + v))^c -> (Eff r e a)^d -> (Eff r e b)^(c + d)
foreign pure flat_map_eff from "flick:rt" "flatMapEff"
  arity 2
  retention call

map_eff : (forall v. a^v -> b)^c -> (Eff r e a)^d -> (Eff r e b)^(c + d)
foreign pure map_eff from "flick:rt" "mapEff"
  arity 2
  retention call

map2_eff : (forall u v. a^u -> b^v -> x)^c -> (Eff r e a)^d -> (Eff r e b)^d -> (Eff r e x)^(c + d)
foreign pure map2_eff from "flick:rt" "map2Eff"
  arity 3
  retention call
"#;
fn program(body: &str, name: &str, release: bool) -> Program {
    let source = format!("module App.Adapters exposing (..)\n{DECLARATIONS}\n{body}");
    let modules = run::compile_source(&source).expect("genuine checked source");
    let ir: Vec<_> = modules
        .iter()
        .map(|m| {
            if release {
                build::optimized(&modules, m, false)
            } else {
                (*m.ir).clone()
            }
        })
        .collect();
    perceus::lower_with_declarations(
        ir.iter().chain(build::builtin_modules()),
        modules
            .iter()
            .map(|m| m.declarations.as_ref())
            .chain(build::builtin_declarations().iter().map(AsRef::as_ref)),
        &Name::new("App.Adapters", name),
    )
    .expect("closed adapters lower")
}
const SUCCESS: &str = r##"
service Read =
  read : Unit ->{} String
stale : Read.Impl
let stale = { read = fun () -> show (Int.unsafe_div 1 0) }
first : Read.Impl
let first = { read = fun () -> "1" }
second : Read.Impl
let second = { read = fun () -> "2" }
answer : String ->{} String
let answer seed =
  let (saved, mapped, chained, paired) = Layer.run (Layer.succeed Read stale) (fun () ->
    let saved : Eff {Read} none String = pure_eff seed
    let mapped = map_eff (fun value -> value ++ "m") (task { seed ++ Read.read () })
    let chained = flat_map_eff (fun value -> task { value ++ Read.read () }) (task { seed ++ Read.read () })
    let paired = map2_eff (fun left right -> left ++ "/" ++ right) (task { seed ++ "L" ++ Read.read () }) (task { seed ++ "R" ++ Read.read () })
    (saved, mapped, chained, paired))
  let a = Layer.run (Layer.succeed Read first) (fun () -> await saved ++ "|" ++ await mapped ++ "|" ++ await chained ++ "|" ++ await paired)
  let b = Layer.run (Layer.succeed Read second) (fun () -> await saved ++ "|" ++ await mapped ++ "|" ++ await chained ++ "|" ++ await paired)
  a ++ "#" ++ b
identity : Unit ->{} Bool
let identity () =
  let cell = Ref.make "kept"
  let saved : Eff none none (Ref String) = pure_eff cell
  let mapped = map_eff (fun value -> value) saved
  let chained = flat_map_eff (fun value -> task { value }) saved
  let paired = map2_eff (fun left _ -> left) saved saved
  let a = await saved
  let b = await mapped
  let c = await chained
  let d = await paired
  Ref.set a "changed"
  Ref.same a cell && Ref.same b cell && Ref.same c cell && Ref.same d cell && Ref.get cell == "changed"
nested : Unit ->{} String
let nested () =
  let saved : Eff none none (Eff none none String) = pure_eff (task { Host.raw_stdout_line "inner saved"; "S" })
  let mapped = map_eff (fun _ -> task { Host.raw_stdout_line "inner mapped"; "M" }) (task { "ignored" })
  let a = await saved
  let b = await mapped
  Host.raw_stdout_line "between"
  await a ++ await b
nested_services : Unit ->{} String
let nested_services () =
  let wrapped : Eff none none (Eff {Read} none String) = Layer.run (Layer.succeed Read stale) (fun () -> pure_eff (task { Read.read () }))
  let inner = Layer.run (Layer.succeed Read first) (fun () -> await wrapped)
  Layer.run (Layer.succeed Read second) (fun () -> await inner)
"##;
const EXPECTED: &str = "kept|kept1m|kept11|keptL1/keptR1#kept|kept2m|kept22|keptL2/keptR2";
#[test]
fn tasks_stay_cold_replay_and_use_each_await_callers_provider() {
    for release in [false, true] {
        let p = program(SUCCESS, "answer", release);
        let mut heap = Heap::new();
        let input = heap.literal(&Lit::String("kept".into())).expect("seed");
        let report = execute_program_outcome(
            &mut heap,
            &p,
            &Name::new("App.Adapters", "answer"),
            vec![input],
            100_000,
        );
        let ProgramOutcome::Returned(value) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "late services")
        };
        assert_eq!(heap.string(&value), Ok(EXPECTED));
        assert_eq!(report.stdout, "");
        heap.drop_value(value).expect("return release");
        heap.finish().expect("tasks and providers released");
    }
}
#[test]
fn captured_payload_identity_and_nested_task_values_are_not_auto_awaited() {
    for release in [false, true] {
        for name in ["identity", "nested", "nested_services"] {
            let p = program(SUCCESS, name, release);
            let mut heap = Heap::new();
            let report = execute_program_outcome(
                &mut heap,
                &p,
                &Name::new("App.Adapters", name),
                vec![Value::Unit],
                100_000,
            );
            let ProgramOutcome::Returned(value) = report.outcome else {
                causes::unexpected(&mut heap, report.outcome, "ordinary return")
            };
            if name == "identity" {
                assert_eq!(value, Value::Bool(true));
                assert_eq!(report.stdout, "");
            } else if name == "nested_services" {
                assert_eq!(heap.string(&value), Ok("2"));
                assert_eq!(report.stdout, "");
            } else {
                assert_eq!(heap.string(&value), Ok("SM"));
                assert_eq!(report.stdout, "between\ninner saved\ninner mapped\n");
            }
            heap.drop_value(value).expect("result release");
            heap.finish()
                .expect("all aliases and nested tasks released");
        }
    }
}
#[test]
fn every_replay_prefix_preserves_external_capture_and_releases_execution_roots() {
    for release in [false, true] {
        let p = program(SUCCESS, "answer", release);
        let mut completed = false;
        for fuel in 0..=12000 {
            let mut heap = Heap::new();
            let outside = heap
                .literal(&Lit::String("kept".into()))
                .expect("external root");
            let input = heap.dup(&outside).expect("actual captured alias");
            let report = execute_program_outcome(
                &mut heap,
                &p,
                &Name::new("App.Adapters", "answer"),
                vec![input],
                fuel,
            );
            assert_eq!(report.stdout, "");
            match report.outcome {
                ProgramOutcome::Returned(value) => {
                    assert_eq!(heap.string(&value), Ok(EXPECTED));
                    heap.drop_value(value).expect("return release");
                    completed = true;
                }
                ProgramOutcome::Fault(Error::OutOfFuel) => {}
                other => causes::unexpected(&mut heap, other, "task prefix"),
            }
            assert_eq!(heap.string(&outside), Ok("kept"));
            assert_eq!(heap.reference_count(&outside), Ok(1));
            heap.drop_value(outside).expect("external release");
            let stats = heap
                .finish()
                .expect("actual zero occupied heap, not a sweep");
            assert_eq!(stats.allocations, stats.frees);
            if completed {
                eprintln!("eff replay release={release} terminal={fuel}");
                break;
            }
        }
        assert!(completed);
    }
}
const FAILURES: &str = r#"
type error Stop = Stop String String
input : String -> String -> Bool -> Eff none {Stop} String
let input seed label failed = task {
  Host.raw_stdout_line label
  if failed then from_result (Err (Stop label seed)) else seed
}
right_die : String -> Eff none {Stop} String
let right_die seed = task { Host.raw_stdout_line "R"; show (Int.unsafe_rem 1 0) ++ seed }
bad_map : String -> String
let bad_map value = show (Int.unsafe_div 1 0) ++ value
bad_flat : String -> Eff none {Stop} String
let bad_flat value = if Int.unsafe_div 1 0 == 0 then task { value } else task { value ++ "!" }
bad_first : String -> String -> String
let bad_first _ = if Int.unsafe_div 1 0 == 0 then fun right -> right else fun right -> right ++ "!"
map_input : String ->{; Stop} String
let map_input seed = await (map_eff bad_map (input seed "L" true))
map_callback : String ->{; Stop} String
let map_callback seed = await (map_eff bad_map (input seed "L" false))
flat_input : String ->{; Stop} String
let flat_input seed = await (flat_map_eff bad_flat (input seed "L" true))
flat_callback : String ->{; Stop} String
let flat_callback seed = await (flat_map_eff bad_flat (input seed "L" false))
flat_body : String ->{; Stop} String
let flat_body seed = await (flat_map_eff (fun _ -> input seed "C" true) (input seed "L" false))
both_left : String ->{; Stop} String
let both_left seed = await (map2_eff bad_first (input seed "L" true) (input seed "R" false))
both_right : String ->{; Stop} String
let both_right seed = await (map2_eff bad_first (input seed "L" false) (input seed "R" true))
both_callback : String ->{; Stop} String
let both_callback seed = await (map2_eff bad_first (input seed "L" false) (input seed "R" false))
bad_second : String -> String -> String
let bad_second left right = show (Int.unsafe_rem 1 0) ++ left ++ right
both_second : String ->{; Stop} String
let both_second seed = await (map2_eff bad_second (input seed "L" false) (input seed "R" false))
both_body_die : String ->{; Stop} String
let both_body_die seed = await (map2_eff bad_first (input seed "L" false) (right_die seed))
ordinary_argument : String -> String ->{} String
let ordinary_argument label value =
  Host.raw_stdout_line ("build " ++ label)
  value
task_argument : String -> Eff none {Stop} String ->{} Eff none {Stop} String
let task_argument label value =
  Host.raw_stdout_line ("build " ++ label)
  value
cold : String ->{} String
let cold seed =
  let _saved : Eff none {Stop} String = pure_eff (ordinary_argument "S" seed)
  let _mapped = map_eff bad_map (task_argument "M" (input seed "run M" true))
  let _flat = flat_map_eff bad_flat (task_argument "F" (input seed "run F" true))
  let _pair = map2_eff bad_first (task_argument "L" (input seed "run L" true)) (task_argument "R" (input seed "run R" true))
  let _partial_map = map_eff bad_map
  let _partial_flat = flat_map_eff bad_flat
  let _partial_pair = map2_eff bad_first
  let _partial_left = map2_eff bad_first (input seed "unreachable" true)
  Host.raw_stdout_line "constructed"
  seed
factory_map : Unit -> (String -> String)
let factory_map () = if Int.unsafe_div 1 0 == 0 then bad_map else fun value -> value
factory_flat : Unit -> (String -> Eff none {Stop} String)
let factory_flat () = if Int.unsafe_div 1 0 == 0 then bad_flat else fun value -> task { value }
factory_pair : Unit -> (String -> String -> String)
let factory_pair () = if Int.unsafe_div 1 0 == 0 then bad_first else fun left right -> left ++ right
construct_map : String ->{} String
let construct_map seed =
  let _value = map_eff (factory_map ()) (task_argument "unreachable" (input seed "run" true))
  seed
construct_flat : String ->{} String
let construct_flat seed =
  let _value = flat_map_eff (factory_flat ()) (task_argument "unreachable" (input seed "run" true))
  seed
construct_pair : String ->{} String
let construct_pair seed =
  let _value = map2_eff (factory_pair ()) (task_argument "unreachable L" (input seed "run L" true)) (task_argument "unreachable R" (input seed "run R" true))
  seed
construct_saved : String ->{} String
let construct_saved seed =
  let _value : Eff none none Int = pure_eff (Int.unsafe_div 1 0)
  seed
"#;
#[derive(Clone, Copy)]
enum ExpectedFailure {
    Raised(&'static str),
    Die(bool),
}
fn failure_outcome(
    heap: &mut Heap,
    outcome: ProgramOutcome,
    expected: ExpectedFailure,
    outside: &Value,
) {
    match expected {
        ExpectedFailure::Raised(label) => {
            let ProgramOutcome::Raised(payload) = outcome else {
                causes::unexpected(
                    heap,
                    outcome,
                    "singleton typed failure, not Die/Fault/compound",
                )
            };
            assert_eq!(heap.tag(&payload), Ok("App.Adapters.Stop.Stop"));
            assert_eq!(
                heap.string(heap.field(&payload, 0).expect("label")),
                Ok(label)
            );
            assert_eq!(
                heap.field(&payload, 1)
                    .expect("same caller payload")
                    .object_id(),
                outside.object_id()
            );
            assert!(
                heap.field(&payload, 2).is_err(),
                "two ordinary fields and no evidence suffix"
            );
            heap.drop_value(payload)
                .expect("caller-owned failure release");
        }
        ExpectedFailure::Die(remainder) => {
            causes::assert_defect(heap, outcome, &Error::IntDivisionByZero { remainder });
        }
    }
}
#[test]
fn cold_construction_evaluates_all_argument_expressions_once_in_order() {
    for release in [false, true] {
        let p = program(FAILURES, "cold", release);
        let mut heap = Heap::new();
        let seed = heap.literal(&Lit::String("kept".into())).expect("seed");
        let report = execute_program_outcome(
            &mut heap,
            &p,
            &Name::new("App.Adapters", "cold"),
            vec![seed],
            100_000,
        );
        assert_eq!(
            report.stdout,
            "build S\nbuild M\nbuild F\nbuild L\nbuild R\nconstructed\n"
        );
        let ProgramOutcome::Returned(value) = report.outcome else {
            causes::unexpected(
                &mut heap,
                report.outcome,
                "task bodies and all callback stages remain cold",
            )
        };
        assert_eq!(heap.string(&value), Ok("kept"));
        heap.drop_value(value).expect("return release");
        heap.finish()
            .expect("all unused and partially applied tasks released");
    }
}
#[test]
fn every_failure_prefix_preserves_exact_reason_output_and_external_payload_alias() {
    let cases = [
        ("map_input", "L\n", ExpectedFailure::Raised("L")),
        ("map_callback", "L\n", ExpectedFailure::Die(false)),
        ("flat_input", "L\n", ExpectedFailure::Raised("L")),
        ("flat_callback", "L\n", ExpectedFailure::Die(false)),
        ("flat_body", "L\nC\n", ExpectedFailure::Raised("C")),
        ("both_left", "L\n", ExpectedFailure::Raised("L")),
        ("both_right", "L\nR\n", ExpectedFailure::Raised("R")),
        ("both_callback", "L\nR\n", ExpectedFailure::Die(false)),
        ("both_body_die", "L\nR\n", ExpectedFailure::Die(true)),
        ("both_second", "L\nR\n", ExpectedFailure::Die(true)),
        ("construct_map", "", ExpectedFailure::Die(false)),
        ("construct_flat", "", ExpectedFailure::Die(false)),
        ("construct_pair", "", ExpectedFailure::Die(false)),
        ("construct_saved", "", ExpectedFailure::Die(false)),
    ];
    for release in [false, true] {
        for (name, stdout, expected) in cases {
            let p = program(FAILURES, name, release);
            let mut completed = false;
            for fuel in 0..=8192 {
                let mut heap = Heap::new();
                let outside = heap
                    .literal(&Lit::String("kept".into()))
                    .expect("external root");
                let input = heap.dup(&outside).expect("actual payload/capture alias");
                let report = execute_program_outcome(
                    &mut heap,
                    &p,
                    &Name::new("App.Adapters", name),
                    vec![input],
                    fuel,
                );
                assert!(
                    stdout.starts_with(&report.stdout),
                    "{name}: prefix stdout {:?}",
                    report.stdout
                );
                match report.outcome {
                    ProgramOutcome::Fault(Error::OutOfFuel) => {}
                    outcome => {
                        assert_eq!(report.stdout, stdout);
                        failure_outcome(&mut heap, outcome, expected, &outside);
                        completed = true;
                    }
                }
                assert_eq!(heap.string(&outside), Ok("kept"));
                assert_eq!(heap.reference_count(&outside), Ok(1));
                heap.drop_value(outside).expect("caller release");
                let stats = heap
                    .finish()
                    .expect("actual zero occupied heap including parked tasks and causes");
                assert_eq!(stats.allocations, stats.frees);
                if completed {
                    eprintln!("eff failure {name} release={release} terminal={fuel}");
                    break;
                }
            }
            assert!(completed);
        }
    }
}

#[test]
fn copied_declarations_match_the_entire_genuine_builtin_foreign_abi() {
    let source = format!("module App.Adapters exposing (..)\n{DECLARATIONS}");
    let modules = run::compile_source(&source).expect("genuine copied declarations");
    let mut count = 0;
    for item in modules.iter().flat_map(|m| &m.ir.items) {
        let ItemKind::Foreign(actual) = &item.kind else {
            continue;
        };
        let expected = build::builtin_modules()
            .iter()
            .flat_map(|m| &m.items)
            .find_map(|item| match &item.kind {
                ItemKind::Foreign(value)
                    if value.name == actual.name && value.binding == actual.binding =>
                {
                    Some(value)
                }
                _ => None,
            })
            .expect("real corresponding builtin");
        assert_eq!(
            serde_json::to_value(actual).expect("actual ABI"),
            serde_json::to_value(expected).expect("genuine ABI")
        );
        count += 1;
    }
    assert_eq!(count, 4);
}
