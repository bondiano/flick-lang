//! The genuine rank-two `Applicative Option` bridge stays inside owned Flick code.
use flick_core::{
    build,
    ir::{
        Lit,
        memory::{Name, Program},
    },
    opt::perceus,
};
use flick_model::run;
use flick_vm::tier0::{
    Error, Heap, ProgramOutcome, Value, execute_program, execute_program_outcome,
};
use std::panic::catch_unwind;

const SOURCE: &str = r#"module App.OptionMap2 exposing (answer)
answer : Unit -> String
let answer () =
  let result = map2 (fun left right -> left ++ ":" ++ right) (Some "left") (Some "right")
  match result with
  | Some text -> text
  | None -> "wrong None"
"#;
fn program(source: &str, name: &str, release: bool) -> Program {
    let modules = run::compile_source(source).expect("genuine Applicative Option source checks");
    let entry = Name::new("App.OptionMap2", name);
    let ir: Vec<_> = modules
        .iter()
        .map(|module| {
            if release {
                build::optimized(&modules, module, false)
            } else {
                (*module.ir).clone()
            }
        })
        .collect();
    perceus::lower(ir.iter().chain(build::builtin_modules()), &entry)
        .expect("counted Option map2 lowering")
}
#[test]
fn genuine_applicative_option_combines_left_then_right() {
    let programs = [false, true].map(|release| catch_unwind(|| program(SOURCE, "answer", release)));
    assert!(
        programs.iter().all(Result::is_ok),
        "both profiles lower genuine optionMap2"
    );
    for program in programs {
        let program = program.expect("both profiles lower");
        let mut heap = Heap::new();
        let ran = execute_program(
            &mut heap,
            &program,
            &Name::new("App.OptionMap2", "answer"),
            vec![Value::Unit],
            100_000,
        )
        .expect("valid Some inputs execute");
        assert_eq!(heap.string(&ran.value), Ok("left:right"));
        assert_eq!(ran.stdout, "");
        heap.drop_value(ran.value).expect("owned result release");
        let stats = heap.finish().expect("actual zero occupied heap slots");
        assert_eq!(stats.allocations, stats.frees);
    }
}

const CASES: &str = r#"module App.OptionMap2 exposing (matrix, cold, eager, combine, identity)
render : Option String -> String
let render value = match value with | None -> "none" | Some text -> text
pair : Option String -> Option String -> String
let pair left right = render (map2 (fun a b -> a ++ ":" ++ b) left right)
matrix : Unit -> String
let matrix () =
  pair (Some "L") (Some "R") ++ "|" ++ pair (Some "L") None
    ++ "|" ++ pair None (Some "R") ++ "|" ++ pair None None
spin : Unit -> a
let rec spin () = spin ()
wrong_first : String -> String -> String
let wrong_first _ = spin ()
wrong_second : String -> String -> String
let wrong_second _ _ = spin ()
cold : String -> String
let cold captured =
  let partial = map2 (fun left right -> captured ++ wrong_second left right)
  let partial_left = partial (Some "left")
  render (map2 wrong_first None (Some "right"))
    ++ "|" ++ render (map2 wrong_first (Some "left") None)
    ++ "|" ++ render (map2 wrong_second None None) ++ "|" ++ captured
argument : String -> Option String ->{} Option String
let argument label value =
  Host.raw_stdout_line label
  value
eager : Unit ->{} String
let eager () =
  let result = map2 wrong_first (argument "left" None) (argument "right" (Some "right"))
  render result
combine : String -> Option String -> Option String -> Option String
let combine captured left right = map2 (fun a b -> captured ++ ":" ++ a ++ "/" ++ b) left right
identity : Unit ->{} Bool
let identity () =
  let left = Ref.make "left"
  let right = Ref.make "right"
  let result = map2 (fun first _ -> first) (Some left) (Some right)
  match result with
  | Some chosen ->
    Ref.set chosen "changed"
    Ref.same chosen left && Ref.get left == "changed" && Ref.get right == "right"
  | None -> false
"#;

#[test]
fn four_option_combinations_and_skipped_callbacks_obey_source_control_flow() {
    for release in [false, true] {
        for (name, input, expected) in [
            ("matrix", Value::Unit, "L:R|none|none|none"),
            ("cold", Value::Unit, "none|none|none|kept"),
            ("eager", Value::Unit, "none"),
        ] {
            let program = program(CASES, name, release);
            let mut heap = Heap::new();
            let input = if name == "cold" {
                heap.literal(&Lit::String("kept".into())).expect("capture")
            } else {
                input
            };
            let ran = execute_program(
                &mut heap,
                &program,
                &Name::new("App.OptionMap2", name),
                vec![input],
                100_000,
            )
            .expect("None skips both stages of callback, not argument evaluation");
            assert_eq!(
                heap.string(&ran.value),
                Ok(expected),
                "{name} release={release}"
            );
            assert_eq!(
                ran.stdout,
                if name == "eager" { "left\nright\n" } else { "" }
            );
            heap.drop_value(ran.value).expect("owned return release");
            let stats = heap.finish().expect("no occupied slots");
            assert_eq!(stats.allocations, stats.frees);
        }
    }
}

#[test]
fn option_payloads_preserve_reference_identity() {
    for release in [false, true] {
        let program = program(CASES, "identity", release);
        let mut heap = Heap::new();
        let ran = execute_program(
            &mut heap,
            &program,
            &Name::new("App.OptionMap2", "identity"),
            vec![Value::Unit],
            100_000,
        )
        .expect("owned reference payloads stay inside Flick");
        assert_eq!(ran.value, Value::Bool(true));
        assert_eq!(ran.stdout, "");
        heap.drop_value(ran.value).expect("result release");
        let stats = heap
            .finish()
            .expect("reference cells, payloads and captures released");
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn every_fuel_prefix_preserves_shared_capture_and_payload_roots() {
    for release in [false, true] {
        let program = program(CASES, "combine", release);
        for (left_some, right_some) in [(true, true), (true, false), (false, true), (false, false)]
        {
            let mut completed = false;
            for fuel in 0..=4096 {
                let mut heap = Heap::new();
                let external = heap
                    .literal(&Lit::String("outside".into()))
                    .expect("external root");
                let capture = heap.dup(&external).expect("callback capture alias");
                let mut option = |some| {
                    let fields = if some {
                        vec![heap.dup(&external).expect("payload alias")]
                    } else {
                        vec![]
                    };
                    heap.construct(
                        if some {
                            "Prelude.Option.Some"
                        } else {
                            "Prelude.Option.None"
                        },
                        fields,
                    )
                    .expect("ordinary source Option input")
                };
                let left = option(left_some);
                let right = option(right_some);
                let report = execute_program_outcome(
                    &mut heap,
                    &program,
                    &Name::new("App.OptionMap2", "combine"),
                    vec![capture, left, right],
                    fuel,
                );
                assert_eq!(report.stdout, "");
                match report.outcome {
                    ProgramOutcome::Returned(value) => {
                        if left_some && right_some {
                            assert_eq!(heap.tag(&value), Ok("Prelude.Option.Some"));
                            assert_eq!(
                                heap.string(heap.field(&value, 0).expect("ordinary payload")),
                                Ok("outside:outside/outside")
                            );
                            assert!(
                                heap.field(&value, 1).is_err(),
                                "exact one physical field, no evidence suffix"
                            );
                        } else {
                            assert_eq!(heap.tag(&value), Ok("Prelude.Option.None"));
                            assert!(
                                heap.field(&value, 0).is_err(),
                                "None has no physical/evidence fields"
                            );
                        }
                        heap.drop_value(value).expect("owned result release");
                        completed = true;
                    }
                    ProgramOutcome::Fault(Error::OutOfFuel) => {}
                    other => {
                        let diagnostic = format!("release={release} fuel={fuel}: {other:?}");
                        other
                            .release(&mut heap)
                            .expect("release unexpected owned outcome");
                        panic!("{diagnostic}")
                    }
                }
                assert_eq!(heap.string(&external), Ok("outside"));
                assert_eq!(heap.reference_count(&external), Ok(1));
                assert_eq!(
                    heap.stats().live_objects,
                    1,
                    "only caller-owned external root remains"
                );
                heap.drop_value(external).expect("external release");
                let stats = heap.finish().expect("actual zero occupied slots, no sweep");
                assert_eq!(stats.allocations, stats.frees);
                if completed {
                    eprintln!(
                        "optionMap2 release={release} some=({left_some},{right_some}) first terminal fuel={fuel}"
                    );
                    break;
                }
            }
            assert!(completed, "consecutive prefixes reach completion");
        }
    }
}
