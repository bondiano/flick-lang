//! Independent primitive total-order and rt/Go hash vectors through checked source.
use flick_core::{build, ir::memory::Name, opt::perceus};
use flick_model::run;
use flick_vm::tier0::{Heap, Value, execute_program};

#[test]
fn checked_total_compare_is_nan_last_and_preserves_partial_application() {
    let text = r#"module App.Numeric exposing (answer)
render : Ordering -> String
let render value = match value with | LT -> "LT" | EQ -> "EQ" | GT -> "GT"
answer : Unit -> String
let answer () =
  let nan : Float = 0.0 / 0.0
  let infinity : Float = 1.0 / 0.0
  let after = compare (1 : Int)
  render (after 2) ++ "|" ++ render (compare nan infinity)
    ++ "|" ++ render (compare nan nan) ++ "|" ++ render (compare (-0.0) 0.0)
    ++ "|" ++ render (compare "😀" "\u{e000}")
    ++ "|" ++ render (compare 123456789012345678901234567890n 123456789012345678901234567891n)
    ++ "|" ++ render (compare true false)
"#;
    check(text, "LT|GT|EQ|EQ|LT|LT|GT");
}

#[test]
fn checked_hash_matches_rt_go_primitive_and_nominal_ordering_vectors() {
    let text = r#"module App.Numeric exposing (answer)
answer : Unit -> String
let answer () =
  show (hash "") ++ "|" ++ show (hash (0 : Int))
    ++ "|" ++ show (hash (1.5 : Float)) ++ "|" ++ show (hash (Some "x"))
    ++ "|" ++ show (hash true) ++ "|" ++ show (hash false)
    ++ "|" ++ show (hash "😀") ++ "|" ++ show (hash (0x18ee90ff6c373e0ee4e3f0ad2n))
    ++ "|" ++ show (hash EQ) ++ "|" ++ show (hash ())
"#;
    check(
        text,
        "5381|0|-182423092|177629|438880922|671588899|5191800|-235053180|743422407|0",
    );
}

#[test]
fn canonical_equals_adapter_keeps_existing_same_value_zero_and_payload_semantics() {
    let text = r#"module App.Numeric exposing (answer)
equal_value : a -> a -> Bool
foreign pure equal_value from "flick:rt" "equals"
answer : Unit -> String
let answer () =
  let nan : Float = 0.0 / 0.0
  show (equal_value nan nan) ++ "|" ++ show (equal_value (-0.0) 0.0)
    ++ "|" ++ show (equal_value 0x10000000000000000n 18446744073709551616n)
    ++ "|" ++ show (equal_value EQ EQ) ++ "|" ++ show (equal_value LT GT)
"#;
    check(text, "true|true|true|true|false");
}

fn check(text: &str, expected: &str) {
    let modules = run::compile_source(text).expect("accepted numeric source");
    let entry = Name::new("App.Numeric", "answer");
    for release in [false, true] {
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
        let program = perceus::lower(ir.iter().chain(build::builtin_modules()), &entry)
            .expect("counted primitive numeric adapters");
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 100_000)
            .expect("primitive execution");
        assert_eq!(heap.string(&ran.value), Ok(expected), "release={release}");
        assert_eq!(ran.stdout, "");
        heap.drop_value(ran.value).expect("release result");
        let memory = heap.finish().expect("all actual roots released");
        assert_eq!(memory.allocations, memory.frees);
    }
}
