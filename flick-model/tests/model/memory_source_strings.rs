//! String predicates use their specified subject-last order and exact text.
use crate::causes;
use flick_core::{
    build,
    ir::{Lit, memory::Name},
    opt::perceus,
};
use flick_model::{
    memory::{Error, Heap, ProgramOutcome, Value, execute_program, execute_program_outcome},
    run,
};

#[test]
fn predicates_preserve_partial_application_aliases_and_unicode_in_both_profiles() {
    let text = r#"module App.StringPredicates exposing (answer)
import String
answer : String ->{} Bool
let answer subject =
  let prefix = String.starts_with "😀"
  let includes = String.contains
  let middle = includes "e\u{301}"
  let matched =
    prefix subject && middle subject
      && String.contains "" subject
      && String.starts_with "" subject
      && not (String.contains "é" subject)
      && not (String.starts_with "e\u{301}" subject)
      && not (String.contains "😁" subject)
  Platform.stdout_line (if matched then "matched" else "wrong")
  matched
"#;
    let modules = run::compile_source(text).expect("accepted String source");
    let entry = Name::new("App.StringPredicates", "answer");
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
        let program = perceus::lower(ir.iter().chain(build::builtin_modules()), &entry)
            .expect("exact String predicates lower to counted code");
        let mut heap = Heap::new();
        let outside = heap
            .literal(&Lit::String("😀/e\u{301}/end".into()))
            .expect("subject");
        let input = heap.dup(&outside).expect("independent external alias");
        let ran =
            execute_program(&mut heap, &program, &entry, vec![input], 100_000).expect("execution");
        assert!(matches!(ran.value, Value::Bool(true)));
        assert_eq!(ran.stdout, "matched\n");
        assert_eq!(heap.string(&outside), Ok("😀/e\u{301}/end"));
        heap.drop_value(ran.value).expect("result");
        heap.drop_value(outside).expect("external root");
        let stats = heap.finish().expect("all actual roots released");
        assert_eq!(stats.live_objects, 0);
        assert_eq!(stats.allocations, stats.frees);
    }
}

const JOIN_SOURCE: &str = r#"module App.StringJoin exposing (answer, escaped, shared)
mark : String ->{} String
let mark text =
  Platform.stdout_line text
  text
escaped : String -> List String -> String
let escaped separator = String.join separator
shared : String -> String
let shared separator =
  let joiner = escaped separator
  let first = joiner ["a", "b", ""]
  joiner [first, "😀"]
answer : Unit ->{} String
let answer () =
  let joiner = escaped (mark "/")
  let empty = joiner []
  let singleton = joiner [""]
  let joined = joiner [mark "😀", mark "e\u{301}", mark ""]
  empty ++ "|" ++ singleton ++ "|" ++ joined
    ++ "|" ++ String.join "" ["a", "b", "c"]
    ++ "|" ++ String.join "unused" ["solo"]
    ++ "|" ++ String.join "::" ["a", "b"]
"#;

#[test]
fn join_preserves_empty_singleton_unicode_and_argument_order() {
    let modules = run::compile_source(JOIN_SOURCE).expect("String.join source checks");
    let entry = Name::new("App.StringJoin", "answer");
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
        let program = perceus::lower(ir.iter().chain(build::builtin_modules()), &entry)
            .expect("String.join has an explicit owned implementation");
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("String.join execution");
        assert_eq!(heap.string(&ran.value), Ok("||😀/e\u{301}/|abc|solo|a::b"));
        assert_eq!(ran.stdout, "/\n😀\ne\u{301}\n\n");
        heap.drop_value(ran.value).expect("release joined result");
        let stats = heap.finish().expect("joined strings and tails released");
        assert_eq!(stats.live_objects, 0);
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn join_fuel_prefixes_preserve_escaped_shared_separator() {
    let modules = run::compile_source(JOIN_SOURCE).expect("shared separator source checks");
    let entry = Name::new("App.StringJoin", "shared");
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
        let program = perceus::lower(ir.iter().chain(build::builtin_modules()), &entry)
            .expect("escaped joiner lowers");
        let mut completed = false;
        for fuel in 0..=4096 {
            let mut heap = Heap::new();
            let external = heap.literal(&Lit::String("/".into())).expect("separator");
            let input = heap.dup(&external).expect("external separator alias");
            let report = execute_program_outcome(&mut heap, &program, &entry, vec![input], fuel);
            assert_eq!(report.stdout, "");
            match report.outcome {
                ProgramOutcome::Returned(value) => {
                    assert_eq!(heap.string(&value), Ok("a/b//😀"));
                    heap.drop_value(value).expect("release result");
                    completed = true;
                }
                ProgramOutcome::Fault(Error::OutOfFuel) => {}
                other => {
                    let diagnostic = format!("release={release} fuel={fuel}: {other:?}");
                    causes::unexpected(&mut heap, other, &diagnostic)
                }
            }
            assert_eq!(heap.string(&external), Ok("/"));
            assert_eq!(heap.stats().live_objects, 1, "only external alias survives");
            heap.drop_value(external).expect("release external root");
            let stats = heap.finish().expect("no sweep or hidden roots");
            assert_eq!(stats.live_objects, 0);
            assert_eq!(stats.allocations, stats.frees);
            if completed {
                eprintln!("String.join release={release}: all {} prefixes", fuel + 1);
                break;
            }
        }
        assert!(completed, "fuel range reaches successful source execution");
    }
}
