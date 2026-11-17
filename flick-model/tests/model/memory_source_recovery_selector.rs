//! Actual checked guards select a deferred arm, not an already executed body.
use crate::causes;
use flick_core::{
    build,
    ir::{
        Lit,
        memory::{self, Name},
    },
    opt::perceus,
};
use flick_model::{
    memory::{Error, Heap, ProgramOutcome, execute_program_outcome},
    run,
};

#[test]
fn nested_guard_false_or_shadow_and_ref_capture_keep_original_suffix_environment() {
    for handler_errors in [false, true] {
        let effect_body = if handler_errors {
            "    ignore (Gate.gate ())\n"
        } else {
            ""
        };
        let effect_arm = if handler_errors {
            "  | Gate.gate () -> true\n"
        } else {
            ""
        };
        let text = format!(
            r#"module App.RecoveryProtocol exposing (answer)
effect Gate =
  gate : Unit ->{{}} Bool
type error Denied = Denied String
class Label a =
  label : a -> String
instance Label Int =
  let label value = show value
answer : String ->{{}} String
let answer seed = choose 7 seed
choose : Label a => a -> String ->{{}} String
let choose value seed =
  let cell = Ref.make seed
  let guard value =
    try from_result (Err (Denied value)) with
    | Denied _ ->
      Platform.stdout_line "nested"
      false
  try
{effect_body}    from_result (Err (Denied "shadow"))
  with
{effect_arm}  | Denied seed | Denied seed when guard seed ->
    Platform.stdout_line "wrong first body"
    "wrong first body"
  | Denied _ ->
    Platform.stdout_line "body"
    label value ++ "/" ++ Ref.get cell ++ "/" ++ seed
"#
        );
        let modules = run::compile_source(&text).expect("real checked guard source");
        let entry = Name::new("App.RecoveryProtocol", "answer");
        for release in [false, true] {
            let ir: Vec<_> = modules
                .iter()
                .map(|module| {
                    if release {
                        build::optimized(&modules, module, false)
                    } else {
                        module.ir.clone()
                    }
                })
                .collect();
            let program: memory::Program =
                perceus::lower(ir.iter().chain(build::builtin_modules()), &entry)
                    .expect("real ownership lowering");
            let mut complete = false;
            for fuel in 0..2000 {
                let mut heap = Heap::new();
                let input = heap
                    .literal(&Lit::String("outer".into()))
                    .expect("owned input");
                let external = heap.dup(&input).expect("external ALIAS of consumed input");
                let report =
                    execute_program_outcome(&mut heap, &program, &entry, vec![input], fuel);
                assert!("nested\nbody\n".starts_with(&report.stdout), "{report:?}");
                match &report.outcome {
                    ProgramOutcome::Fault(Error::OutOfFuel) => {}
                    ProgramOutcome::Returned(value) => {
                        assert_eq!(heap.string(value).expect("body string"), "7/outer/outer");
                        assert_eq!(report.stdout, "nested\nbody\n");
                        complete = true;
                    }
                    other => {
                        let diagnostic = format!(
                            "release={release}, handler_errors={handler_errors}, fuel={fuel}: {other:?}"
                        );
                        causes::unexpected(&mut heap, report.outcome, &diagnostic)
                    }
                }
                assert_eq!(
                    heap.string(&external).expect("external alias preserved"),
                    "outer"
                );
                report
                    .outcome
                    .release(&mut heap)
                    .expect("explicit result release");
                heap.drop_value(external)
                    .expect("caller-owned alias release");
                let stats = heap.finish().expect("actual heap empty for every prefix");
                assert_eq!(stats.allocations, stats.frees);
                if complete {
                    break;
                }
            }
            assert!(complete, "actual selected body reached");
        }
    }
}
