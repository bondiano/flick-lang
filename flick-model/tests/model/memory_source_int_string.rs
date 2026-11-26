//! Exact primitive contracts on checked source, without substitution fallback.
use crate::causes;
use flick_core::ir::Lit;
use flick_model::run;
use flick_vm::tier0::{Error, Heap, ProgramOutcome};
use std::{fs, path::Path};

fn success(source: &str, expected: &str) {
    let modules = run::compile_source(source).expect("source checks");
    let main = run::main_of(&modules).expect("main");
    for release in [false, true] {
        let ran = run::run_main_counted(&modules, &main, 100_000, release)
            .expect("exact primitive lowers and executes");
        assert_eq!(ran.stdout, expected, "release={release}");
        assert_eq!(ran.memory.live_objects, 0);
        assert_eq!(ran.memory.allocations, ran.memory.frees);
    }
}

#[test]
fn unsafe_division_truncates_toward_zero_and_wraps_the_signed_overflow() {
    success(
        r"module App.IntDivide exposing (main)
main : Unit ->{Host.Console} Unit
let main () =
  Platform.stdout_line (show (Int.unsafe_div 7 3))
  Platform.stdout_line (show (Int.unsafe_div (-7) 3))
  Platform.stdout_line (show (Int.unsafe_div 7 (-3)))
  Platform.stdout_line (show (Int.unsafe_div (-7) (-3)))
  Platform.stdout_line (show (Int.unsafe_div (-2147483647 - 1) (-1)))
  Platform.stdout_line (show (Int.unsafe_div 0 (-1)))
",
        "2\n-2\n-2\n2\n-2147483648\n0\n",
    );
}

#[test]
fn unsafe_remainder_keeps_the_dividend_sign_and_has_no_negative_zero() {
    success(
        r"module App.IntRemainder exposing (main)
main : Unit ->{Host.Console} Unit
let main () =
  Platform.stdout_line (show (Int.unsafe_rem 7 3))
  Platform.stdout_line (show (Int.unsafe_rem (-7) 3))
  Platform.stdout_line (show (Int.unsafe_rem 7 (-3)))
  Platform.stdout_line (show (Int.unsafe_rem (-7) (-3)))
  Platform.stdout_line (show (Int.unsafe_rem (-2147483647 - 1) (-1)))
  Platform.stdout_line (show (Int.unsafe_rem (-6) 3))
",
        "1\n-1\n1\n-1\n0\n0\n",
    );
}

#[test]
fn int_parse_uses_ecmascript_whitespace_and_32_bit_signed_bounds() {
    success(
        r#"module App.IntParse exposing (main)
main : Unit ->{Host.Console} Unit
let main () =
  let cases = [
    "\u{FEFF}36",
    "36\u{FEFF}",
    "\u{FEFF}36\u{00A0}",
    "\u{00A0}36\u{FEFF}",
    " \t\n\u{000B}\u{000C}\r 36 \r\u{000C}\u{000B}\n\t ",
    "\u{0085}36",
    "36\u{0085}",
    "3 6",
    "3\u{00A0}6",
    "3\u{FEFF}6",
    "+ 36",
    "- 36",
    "",
    "   ",
    "+",
    "-",
    "+36",
    "-36",
    "-0",
    "+0",
    "-2147483648",
    "-2147483649",
    "2147483647",
    "2147483648",
  ]
  List.each_effect (fun s -> Platform.stdout_line (show (Int.parse s))) cases
"#,
        concat!(
            "Some 36\n",
            "Some 36\n",
            "Some 36\n",
            "Some 36\n",
            "Some 36\n",
            "None\n",
            "None\n",
            "None\n",
            "None\n",
            "None\n",
            "None\n",
            "None\n",
            "None\n",
            "None\n",
            "None\n",
            "None\n",
            "Some 36\n",
            "Some -36\n",
            "Some 0\n",
            "Some 0\n",
            "Some -2147483648\n",
            "None\n",
            "Some 2147483647\n",
            "None\n",
        ),
    );
}

#[test]
fn string_length_counts_utf16_units_without_normalizing_text() {
    success(
        r#"module App.TextLength exposing (main)
main : Unit ->{Host.Console} Unit
let main () =
  Platform.stdout_line (show (String.length ""))
  Platform.stdout_line (show (String.length "abc"))
  Platform.stdout_line (show (String.length "😀"))
  Platform.stdout_line (show (String.length "a😀中"))
  Platform.stdout_line (show (String.length "é"))
  Platform.stdout_line (show (String.length "e\u{301}"))
"#,
        "0\n3\n2\n4\n1\n2\n",
    );
}

fn zero_fault(operation: &str) {
    let source = format!(
        "module App.IntFault exposing (main)\n\nmain : Unit ->{{Host.Console}} Unit\nlet main () =\n  let kept = Ref.make \"held\"\n  Platform.stdout_line (Ref.get kept)\n  ignore (Int.{operation} 7 0)\n  Platform.stdout_line \"unreachable\"\n"
    );
    let modules = run::compile_source(&source).expect("defect source checks");
    let main = run::main_of(&modules).expect("main");
    for release in [false, true] {
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("outside".into()))
            .expect("external root");
        let report = run::run_main_counted_outcome(&mut heap, &modules, &main, 100_000, release)
            .expect("zero division is an executed defect, not unsupported lowering");
        assert_eq!(report.stdout, "held\n");
        let ProgramOutcome::Failed(cause) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "division by zero is owned Die")
        };
        let error = causes::defect_error(&mut heap, cause);
        assert_eq!(
            error.to_string(),
            format!("RangeError: Int.{operation}: division by zero")
        );
        assert_eq!(heap.string(&external), Ok("outside"));
        heap.drop_value(external)
            .expect("external owner releases its root");
        let stats = heap.finish().expect("fault releases all execution roots");
        assert_eq!(stats.live_objects, 0);
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn zero_divisor_is_a_division_defect_and_releases_owned_roots() {
    zero_fault("unsafe_div");
}

#[test]
fn zero_divisor_is_a_remainder_defect_and_releases_owned_roots() {
    zero_fault("unsafe_rem");
}

#[test]
fn existing_corpus_defects_keep_evaluation_order_and_release_the_counted_heap() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run");
    for case in [
        "codegen/defect",
        "codegen/apply_order",
        "codegen/composition_method",
    ] {
        let modules = run::compile(&root.join(format!("{case}.fl"))).expect("corpus source checks");
        let main = run::main_of(&modules).expect("main");
        let stdout =
            fs::read_to_string(root.join(format!("{case}.stdout"))).expect("independent stdout");
        for release in [false, true] {
            let mut heap = Heap::new();
            let report =
                run::run_main_counted_outcome(&mut heap, &modules, &main, 100_000, release)
                    .expect("existing defect source lowers without substitution");
            assert_eq!(report.stdout, stdout, "{case} release={release}");
            causes::assert_defect(
                &mut heap,
                report.outcome,
                &Error::IntDivisionByZero { remainder: false },
            );
            let stats = heap
                .finish()
                .expect("defect releases actual occupied slots");
            assert_eq!(stats.live_objects, 0);
            assert_eq!(stats.allocations, stats.frees);
        }
    }
}
