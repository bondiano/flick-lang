//! End-to-end release optimizer tests for the bounded concrete evaluator (#803):
//! formula fixture (#815), distinct closure captures, staged traps & pipe order,
//! dead branches, large data / deep recursion budgets, source-map spans, and
//! deterministic JS + Go emission.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::tests::{builtin_arity, builtin_generic, release};
use crate::codegen::test_options::resolved_by;
use crate::codegen::{EffectBridge, Known, Labels, Options, js};
use crate::ir::lang_items;
use crate::ir::tests::check_text;
use crate::types;
use flick_backend_support::opt as portable;

const FORMULA_SOURCE: &str = "\
module App exposing (price, total)

type Formula = Input | Number Int | Add Formula Formula | Times Formula Formula

eval : Formula -> Int -> Int
let rec eval formula x =
  match formula with
  | Input -> x
  | Number n -> n
  | Add a b -> eval a x + eval b x
  | Times a b -> eval a x * eval b x

price : Int -> Int
let price x =
  eval (Add (Times Input (Number 3)) (Add (Number 2) (Number 5))) x

total : Int
let total = price 10
";

fn emit_js_with_config(
    text: &str,
    config: &portable::eval::EvalConfig,
) -> (String, portable::eval::EvalCounters) {
    let (mut ir, interface) = check_text("App", text).expect("lowers");
    let table = lang_items();
    let known = Known::of(&table);
    let builtin = |module: &str| {
        types::builtin_sources()
            .iter()
            .any(|(name, _)| *name == module)
    };
    let counters = portable::optimize_with_eval(
        &mut ir,
        &|name: &str| interface.exports_value(name),
        &builtin_generic,
        &builtin_arity,
        &known,
        &builtin,
        config,
        &portable::eval::NoPrimitives,
    );
    let resolved = resolved_by(&ir, &str::to_owned);
    let options = Options {
        builtins: &[],
        paths: None,
        packages: &BTreeMap::new(),
        resolved: &resolved,
        typescript: None,
        release: true,
        workers: &HashMap::new(),
        direct: &HashSet::new(),
        labels: Labels::default(),
        rt: false,
        effect_bridge: EffectBridge::Absent,
        lang: &known,
    };
    (js(&ir, &options).code, counters)
}

#[test]
fn formula_price_10_folds_to_37_and_keeps_dynamic_price() {
    let js_first = release(FORMULA_SOURCE);
    let js_second = release(FORMULA_SOURCE);
    assert_eq!(
        js_first, js_second,
        "release JS output must be deterministic"
    );
    assert!(js_first.contains("export const total = 37;"), "{js_first}");
    assert!(
        js_first.contains("export const price = (x) =>"),
        "{js_first}"
    );
}

#[test]
fn closures_with_distinct_captures_and_capture_avoidance_fold_independently() {
    let src = "\
module App exposing (pair, guarded)

make_adder : Int -> Int -> Int
let make_adder n =
  let bias = n * 2
  fun x -> bias + x

pair : (Int, Int)
let pair =
  let f = make_adder 5
  let g = make_adder 20
  (f 1, g 1)

guarded : Int
let guarded =
  let x = 100
  let shadow = make_adder 3
  shadow x
";
    let js = release(src);
    assert!(js.contains("export const pair = [11, 41];"), "{js}");
    assert!(js.contains("export const guarded = 106;"), "{js}");
}

#[test]
fn staged_trap_pipe_order_discarded_throw_and_dead_branch_preserve_semantics() {
    let src = "\
module App exposing (ordinary_trap, piped_trap, piped_ok, discarded_throw, dead_branch)

staged : BigInt -> BigInt -> BigInt
let staged x =
  let checked = 2n ** x
  let more = checked + 1n
  let extra = more + 1n
  let keep = extra - 2n
  fun y -> keep + y

ext_call : Unit -> BigInt
foreign pure ext_call from \"./ext.mjs\" \"extCall\"

const_seven : BigInt -> Int
let const_seven _ = 7

ordinary_trap : Unit -> BigInt
let ordinary_trap () = staged (-1n) (ext_call ())

piped_trap : Unit -> BigInt
let piped_trap () = ext_call () |> staged (-1n)

piped_ok : BigInt
let piped_ok = 5n |> staged 3n

discarded_throw : Unit -> Int
let discarded_throw () = const_seven (2n ** (-1n))

dead_branch : Int
let dead_branch =
  let pick b = if b then 7 else Int.unsafe_div 1 0
  pick true
";
    let js = release(src);
    // Stage-1 trap stays before arg 2 (`staged(-1n)($f_ext_call())` or inlined `2n ** (-1n)` first).
    assert!(js.contains("ordinary_trap"), "{js}");
    let ord = js
        .split("export const ordinary_trap")
        .nth(1)
        .and_then(|s| s.split("export const piped_trap").next())
        .expect("ordinary_trap");
    let trap_pos = ord
        .find("staged(-1n)")
        .or_else(|| ord.find("staged((-1n))"))
        .or_else(|| ord.find("2n ** (-1n)"))
        .unwrap_or_else(|| panic!("ordinary_trap keeps stage-1 trap:\n{ord}"));
    let ext_pos = ord
        .find("$f_ext_call")
        .expect("ordinary_trap keeps foreign arg");
    assert!(trap_pos < ext_pos, "stage 1 must run before arg 2: {ord}");

    // Pipe evaluates argument first before calling `staged (-1n)`.
    let piped = js
        .split("export const piped_trap")
        .nth(1)
        .and_then(|s| s.split("export const piped_ok").next())
        .expect("piped_trap");
    let p_ext = piped
        .find("$f_ext_call")
        .expect("piped_trap keeps foreign arg");
    let p_stage = piped
        .find("staged(-1n)")
        .or_else(|| piped.find("staged((-1n))"))
        .or_else(|| piped.find("2n ** (-1n)"))
        .unwrap_or_else(|| panic!("piped_trap keeps stage 1:\n{piped}"));
    assert!(
        p_ext < p_stage,
        "arg-first pipe must evaluate arg first: {piped}"
    );

    // Valid piped staged call folds to `2^3 + 5 = 13n`.
    assert!(js.contains("export const piped_ok = 13n;"), "{js}");
    // Discarded throwing arg keeps `2n ** (-1n)`.
    let disc = js
        .split("export const discarded_throw")
        .nth(1)
        .and_then(|s| s.split("export const dead_branch").next())
        .expect("discarded_throw");
    assert!(
        disc.contains("2n ** (-1n)"),
        "discarded throw must stay: {disc}"
    );
    // Dead branch with unsupported call (`Int.unsafe_div`) is eliminated.
    assert!(js.contains("export const dead_branch = 7;"), "{js}");
    assert!(!js.contains("unsafe_div"), "{js}");
}

#[test]
fn large_data_deep_recursion_and_zero_budget_fallback_stay_bounded() {
    let src = "\
module App exposing (huge, deep, closed)

grow : Int -> String -> String
let rec grow n s =
  if n <= 0 then s else grow (n - 1) (s ++ s)

depth : Int -> Int
let rec depth n =
  if n <= 0 then 0 else 1 + depth (n - 1)

huge : String
let huge = grow 14 \"x\"

deep : Int
let deep = depth 300

closed : Int
let closed = depth 5
";
    let (default_js, default_counters) =
        emit_js_with_config(src, &portable::eval::EvalConfig::default());
    assert!(
        default_js.contains("export const closed = 5;"),
        "{default_js}"
    );
    assert!(default_js.contains("grow$2(14, \"x\")"), "{default_js}");
    assert!(default_js.contains("depth(300)"), "{default_js}");
    assert!(default_counters.budget >= 2, "{default_counters:?}");

    let (zero_js, zero_counters) = emit_js_with_config(
        src,
        &portable::eval::EvalConfig {
            max_module_steps: 0,
            ..portable::eval::EvalConfig::default()
        },
    );
    assert_eq!(zero_counters.success, 0);
    assert!(zero_js.contains("depth(5)"), "{zero_js}");
}

#[cfg(feature = "native-project")]
#[test]
fn release_source_map_preserves_call_site_span_and_go_folds_formula() {
    use crate::build::builtin_modules;
    use crate::build::go::bindings;
    use crate::codegen::{go, sourcemap};
    use crate::ir::Module;
    use crate::source::SourceMap;

    // 1. Check JS source map points folded `37` back to the `total` / `price 10` source line.
    let (mut ir, interface) = check_text("App", FORMULA_SOURCE).expect("lowers");
    super::optimize(
        &mut ir,
        &|name: &str| interface.exports_value(name),
        &builtin_generic,
        &builtin_arity,
    );
    let mut sources = SourceMap::default();
    let _file = sources.add("App.fl", FORMULA_SOURCE);
    let table = lang_items();
    let known = Known::of(&table);
    let resolved = resolved_by(&ir, &str::to_owned);
    let options = Options {
        builtins: &[],
        paths: None,
        packages: &BTreeMap::new(),
        resolved: &resolved,
        typescript: None,
        release: true,
        workers: &HashMap::new(),
        direct: &HashSet::new(),
        labels: Labels::default(),
        rt: false,
        effect_bridge: EffectBridge::Absent,
        lang: &known,
    };
    let output = js(&ir, &options);
    assert!(
        output.code.contains("export const total = 37;"),
        "{}",
        output.code
    );
    let map = sourcemap::source_map("App.js", &output, &sources, &|_| "App.fl".into());
    assert!(map.contains("\"sources\":[\"App.fl\"]"), "{map}");
    assert!(!output.mappings.is_empty());

    // 2. Check Go release emission folds `total` to `int32(37)` deterministically.
    let go_source = format!(
        "{FORMULA_SOURCE}\nmain : Unit ->{{Host.Console; AppError}} Unit\nlet main () = Platform.stdout_line (show total)\n"
    )
    .replace("exposing (price, total)", "exposing (main, price, total)");
    let (mut go_ir, go_iface) = check_text("App", &go_source).expect("lowers");
    super::optimize(
        &mut go_ir,
        &|name: &str| go_iface.exports_value(name),
        &builtin_generic,
        &builtin_arity,
    );
    let builtins: Vec<&Module> = builtin_modules().iter().collect();
    let emit_go = || {
        let out = go::go(&go::Input {
            own: &[&go_ir],
            builtins: &builtins,
            entry: &go_ir.name,
            suites: &[],
            test: false,
            paths: &BTreeMap::new(),
            lines: &[],
            foreign: &BTreeMap::new(),
            bindings: bindings(),
            lang: &known,
        });
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        out.files
            .into_iter()
            .find_map(|(name, code)| name.ends_with("m_App.go").then_some(code))
            .expect("m_App.go")
    };
    let go_first = emit_go();
    let go_second = emit_go();
    assert_eq!(
        go_first, go_second,
        "release Go output must be deterministic"
    );
    assert!(
        go_first.contains("F_Platform__stdout_line(fb, \"37\")"),
        "expected folded \"37\" in release Go:\n{go_first}"
    );
    assert!(
        !go_first.contains("F_App__eval") && !go_first.contains("F_App__price"),
        "closed formula call should not emit eval/price in Go:\n{go_first}"
    );
}
