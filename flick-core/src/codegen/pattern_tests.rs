//! The JS of patterns.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::Labels;
use super::test_options::resolved_by;
use super::tests::compile_with;
use super::ts::Typing;
use crate::effects::effect_specifier;
use crate::ir::{lang_items, tests::lower_text};

fn emit(text: &str, release: bool, typescript: bool) -> String {
    emit_target(text, release, typescript, false)
}

fn emit_target(text: &str, release: bool, typescript: bool, rt: bool) -> String {
    let ir = lower_text("App", text).expect("lowers");
    let lang = lang_items();
    let known = super::Known::of(&lang);
    let resolved = resolved_by(&ir, &effect_specifier);
    let prepared = typescript.then(|| {
        let path = |module: &str| format!("{}.ts", module.replace('.', "/"));
        let foreign = |_: &str, spec: &str| effect_specifier(spec);
        let typing = Typing {
            // Match the original standalone emitter's absent interfaces while
            // still preparing the real TypeScript shape for every physical file.
            interface: &|_| None,
            package: &|_| None,
            foreign: &foreign,
            path: &path,
            rt,
        };
        typing.prepare(&ir)
    });
    let options = super::Options {
        builtins: &[],
        paths: None,
        packages: &BTreeMap::new(),
        resolved: &resolved,
        typescript: prepared.as_ref(),
        release,
        workers: &HashMap::new(),
        direct: &HashSet::new(),
        labels: Labels::default(),
        rt,
        effect_bridge: super::EffectBridge::Absent,
        lang: &known,
    };
    super::js(&ir, &options).code
}

#[test]
fn the_last_arm_of_a_release_build_still_binds_the_names_of_an_or_pattern() {
    let text = "module App exposing (extent)\n\n\
         type Shape = Circle Float | Rect Float Float | Empty\n\n\
         extent : Shape -> Float\n\
         let extent shape =\n  match shape with\n  | Empty -> 0\n  | Circle r | Rect r _ -> r\n";
    for release in [false, true] {
        let js = compile_with(text, release, Labels::default()).code;
        // the name is given its value in the test of each alternative
        assert_eq!(js.matches("r = shape._0").count(), 2, "{js}");
    }
    // the last arm needs no test in a release build when it binds nothing there: one `if`, of the first arm
    let js = compile_with(
        &text.replace("Circle r | Rect r _ -> r", "_ -> 1"),
        true,
        Labels::default(),
    )
    .code;
    assert_eq!(js.matches("if (").count(), 1, "{js}");
}

#[test]
fn a_runtime_match_guard_runs_after_the_first_successful_or_pattern() {
    let text = r#"module App exposing (select)
type Pair = Pair Int Int

accept : Int ->{} Bool
foreign effect accept from "./bindings.mjs" "accept"

select : Pair ->{} String
let select pair =
  match pair with
  | Pair x _ | Pair _ x when accept x -> "yes"
  | _ -> "no"
"#;
    for release in [false, true] {
        for typescript in [false, true] {
            let code = emit(text, release, typescript);
            assert_eq!(code.matches("yield* accept(").count(), 1, "{code}");
            let guard = code.find("yield* accept(").expect("runtime guard");
            let first = (code.find("= pair._0")).unwrap_or_else(|| panic!("{code}"));
            assert!(first < guard, "{code}");
            // the decision graph: the first alternative matches every `Pair`,
            // so the second binds nothing
            assert!(!code.contains("pair._1"), "{code}");
            assert!(code[guard..].contains("return \"yes\";"), "{code}");
            assert!(code[guard..].contains("return \"no\";"), "{code}");
            assert!(!code.contains("runIn("), "{code}");
        }
    }
}

#[test]
fn pure_guards_stay_plain_conditions_in_pure_functions() {
    let text = r#"module App exposing (select)
type Pair = Pair Int Int

select : Pair -> String
let select pair =
  match pair with
  | Pair x _ | Pair _ x when x > 1 -> "yes"
  | _ -> "no"
"#;
    for release in [false, true] {
        for typescript in [false, true] {
            let code = emit(text, release, typescript);
            assert!(code.contains("if (x > 1)"), "{code}");
            assert!(!code.contains("yield"), "{code}");
            assert!(!code.contains("Effect."), "{code}");
        }
    }
}

#[test]
fn record_updates_preserve_the_runtime_stage_and_written_evaluation_order() {
    let text = r#"module App exposing (update)
base : Unit ->{} { left : String; right : String }
foreign effect base from "./bindings.mjs" "base"

next : String -> String ->{} String
foreign effect next from "./bindings.mjs" "next"

update : Unit ->{} { left : String; right : String }
let update () =
  { base () with right = next "right-effect" "r"; left = next "left-effect" "l" }
"#;
    for release in [false, true] {
        for typescript in [false, true] {
            let code = emit(text, release, typescript);
            assert_eq!(code.matches("yield*").count(), 3, "{code}");
            let base = code.find("yield* base(").expect("base runs first");
            let right = code.find("\"right-effect\"").expect("first replacement");
            let left = code.find("\"left-effect\"").expect("second replacement");
            assert!(base < right && right < left, "{code}");
        }
    }
}

#[test]
fn typed_error_selectors_stay_plain_while_the_selected_body_runs_its_runtime_stage() {
    let text = r#"module App exposing (recover)
type error Oops = Oops Int

read : Unit ->{; Oops} Int
foreign effect read from "./bindings.mjs" "read"

noted : Int ->{} Int
foreign effect noted from "./bindings.mjs" "noted"

recover : Unit ->{} Int
let recover () =
  try read () with
  | Oops n when n > 0 -> noted n
  | _ -> 0
"#;
    for release in [false, true] {
        for typescript in [false, true] {
            let code = emit(text, release, typescript);
            assert!(code.contains("tryWith("), "{code}");
            assert!(code.contains("if (n > 0)"), "{code}");
            assert!(!code.contains("if ((yield*"), "{code}");
            assert!(
                code.contains("yield* noted(") || code.contains("suspend(() => noted("),
                "{code}"
            );
        }
    }
}

#[test]
fn typed_error_guards_run_in_a_deferred_runtime_stage() {
    let text = r#"module App exposing (recover)
type error Oops = Oops Int
type error GuardFailed = GuardFailed

read : Unit ->{; Oops} Int
foreign effect read from "./bindings.mjs" "read"

accept : Int ->{Async; GuardFailed} Bool
foreign effect accept from "./bindings.mjs" "accept"

recover : Unit ->{Async; Oops, GuardFailed} Int
let recover () =
  try read () with
  | Oops n when accept n -> n
"#;
    for release in [false, true] {
        for typescript in [false, true] {
            for rt in [false, true] {
                let code = emit_target(text, release, typescript, rt);
                let guard = if rt {
                    "yield accept("
                } else {
                    "yield* accept("
                };
                assert!(code.contains(guard), "{code}");
                assert!(code.contains("=== undefined ?"), "{code}");
                assert!(!code.contains("runIn("), "{code}");
                if typescript {
                    assert!(code.contains(": any, $t"), "{code}");
                }
            }
        }
    }
}

#[test]
fn effect_operation_names_are_own_properties_not_prototype_setters() {
    let text = r"module App exposing (answer)
effect Proto =
  __proto__ : Unit ->{} Int

answer : Unit ->{} Int
let answer () =
  try Proto.__proto__ () with
  | Proto.__proto__ () -> 1
";
    for release in [false, true] {
        for typescript in [false, true] {
            let code = emit(text, release, typescript);
            assert!(code.contains("[\"__proto__\"]:"), "{code}");
            assert!(!code.contains("{ \"__proto__\":"), "{code}");
        }
    }
}

// the decision graph is the JS backend's where it builds projects
#[cfg(feature = "native-project")]
#[test]
fn the_arms_of_one_constructor_test_it_once() {
    let text = "module App exposing (name)\n\n\
         type Shape = Dot | Line Int | Pair Shape Shape\n\n\
         name : Shape -> String\nlet name s =\n  match s with\n  \
         | Pair Dot _ -> \"dot first\"\n  | Pair (Line n) _ -> \"line\"\n  \
         | Pair (Pair _ _) Dot -> \"nested\"\n  | _ -> \"other\"\n";
    for release in [false, true] {
        let js = emit(text, release, false);
        // one test of `Pair` for its three arms, then of its first field
        assert_eq!(js.matches("s._tag === \"Pair#").count(), 1, "{js}");
        assert_eq!(js.matches("s._0._tag === \"Line#").count(), 1, "{js}");
        // the tests that fail fall to the catch-all, written once
        assert_eq!(js.matches("return \"other\"").count(), 1, "{js}");
    }
}

#[cfg(feature = "native-project")]
#[test]
fn a_match_that_would_write_an_arm_twice_tries_its_arms_in_turn() {
    // the guarded arm under `Line` and under `Pair`
    let text = "module App exposing (first)\n\n\
         type Shape = Dot | Line Int | Pair Shape Shape\n\n\
         first : Shape -> Int\nlet first s =\n  match s with\n  \
         | Line n | Pair (Line n) _ when n > 0 -> n\n  | _ -> 0\n";
    for release in [false, true] {
        let js = emit(text, release, false);
        assert_eq!(js.matches("return n;").count(), 1, "{js}");
        assert_eq!(js.matches("s._tag === \"Line#").count(), 1, "{js}");
        assert!(js.contains(" || "), "{js}");
    }
}
