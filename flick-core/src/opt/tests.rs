use crate::codegen::{Options, js};
use crate::ir::tests::check_text;

/// The JS of a self-contained module in the release profile.
fn release(text: &str) -> String {
    release_with(text, &|_, _| None)
}

/// `release` where `workers` names the workers modules export.
fn release_with(text: &str, workers: &dyn Fn(&str, &str) -> Option<usize>) -> String {
    let (mut ir, interface) = check_text("App", text).expect("lowers");
    super::optimize(&mut ir, &|name| interface.exports_value(name));
    let path = |m: &str| format!("{}.js", m.replace('.', "/"));
    let options = Options {
        path: &path,
        package: &|_| None,
        foreign: &|_, spec| spec.to_owned(),
        typescript: false,
        interface: &|m| (m == interface.module).then(|| interface.clone()),
        release: true,
        workers,
    };
    js(&ir, &options).code
}

#[test]
fn sum_pair_allocates_no_object() {
    let js = release(
        "module App exposing (sum_pair, total)\n\n\
         sum_pair : Number -> Number -> Number\n\
         let sum_pair left right =\n  let pair = { left = left; right = right }\n  pair.left + pair.right\n\n\
         total : Number -> Number\nlet total n = sum_pair n 1\n",
    );
    insta::assert_snapshot!(js);
}

#[test]
fn a_full_application_calls_the_worker_and_a_partial_one_keeps_the_closure() {
    let js = release(
        "module App exposing (add, both, inc, staged, first)\n\n\
         add : Number -> Number -> Number\nlet add left right =\n  if left > right then left - right else left + right\n\n\
         both : Number -> Number\nlet both x = add x 2\n\n\
         inc : Number -> Number\nlet inc = add 1\n\n\
         staged : Number -> Number -> Number\n\
         let staged x =\n  let checked = 10 / x\n  fun y -> checked + y\n\n\
         first : Number -> Number\nlet first y = staged 0 y\n",
    );
    assert!(js.contains("const add$2 = (left, right) =>"), "{js}");
    assert!(
        js.contains("export const both = (x) => add$2(x, 2);"),
        "{js}"
    );
    assert!(
        js.contains("export const inc = /*#__PURE__*/ add(1);"),
        "{js}"
    );
    // the first stage of `staged` computes: no worker joins its stages
    assert!(!js.contains("staged$2"), "{js}");
    // inlined, the first stage still runs before the second gets its argument
    assert!(js.contains("const checked$i1 = (10 / 0);"), "{js}");
}

#[test]
fn a_canonical_instance_is_called_directly_and_a_witness_stays_chosen() {
    let js = release(
        "module App exposing (..)\n\n\
         type Shape = Circle Number | Square Number\n\n\
         instance Show Shape =\n  let show s =\n    match s with\n    | Circle _ -> \"circle\"\n    | Square _ -> \"square\"\n\n\
         label : Shape -> String\nlet label s = show s\n\n\
         by_size : Shape -> Number\nlet by_size s =\n  match s with\n  | Circle r -> r\n  | Square r -> r\n\n\
         count : List Shape -> Number\n\
         let count shapes =\n  EqHash.with (fun a b -> by_size a == by_size b) by_size (fun witness ->\n    HashSet.size (HashSet.from_list witness shapes)\n  )\n",
    );
    assert!(
        js.contains("export const label = (s) => $i_Show_Shape_"),
        "{js}"
    );
    assert!(js.contains("$show(s);"), "{js}");
    assert!(js.contains("EqHash.with"), "{js}");
}

#[test]
fn forbidden_transformations_stay_out() {
    let js = release(
        "module App exposing (..)\n\n\
         type error Boom = Boom String\n\n\
         service Clock =\n  now : Unit ->{} Number\n\n\
         sum : Number -> Number -> Number -> Number\nlet sum a b c = (a + b) + c\n\n\
         zero : Number -> Number\nlet zero x = x * 0\n\n\
         twice : Eff none none Number -> Unit ->{} Number\nlet twice t () =\n  let a = await t\n  let b = await t\n  a + b\n\n\
         layers : Unit -> (Layer none none {Clock}, Layer none none {Clock})\n\
         let layers () =\n  (Layer.succeed Clock { now = fun () -> 1 }, Layer.succeed Clock { now = fun () -> 1 })\n\n\
         read : Layer none none {Clock} -> Unit ->{} Number\n\
         let read layer () =\n  provide layer {\n    Clock.now ()\n  }\n\n\
         checked : Unknown -> Unit\nlet checked input =\n  let decoded = Codec.decode_result (Codec.json Schema.number) input\n  ()\n",
    );
    // no reassociation, no `x * 0 -> 0`
    assert!(js.contains("((a + b) + c)"), "{js}");
    assert!(js.contains("(x * 0)"), "{js}");
    // a task runs as often as it is awaited
    assert_eq!(js.matches("(yield* t)").count(), 2, "{js}");
    // two layers stay two builds
    assert_eq!(js.matches("Layer.succeed").count(), 2, "{js}");
    // the lookup stays inside `provide`
    let provide = js.find("provide(").expect("provide");
    assert!(js[provide..].contains("operation("), "{js}");
    // a schema check is a call, never dropped for an unused result
    assert!(js.contains("Codec.decode_result"), "{js}");
}

#[test]
fn literal_expressions_fold_with_js_semantics() {
    let js = release(
        "module App exposing (..)\n\n\
         day : Number\nlet day = 60 * 60 * 24\n\n\
         text : String\nlet text = \"Hello, \" ++ \"world\"\n\n\
         sum : Bool\nlet sum = 0.1 + 0.2 == 0.3\n\n\
         minus_zero : Number\nlet minus_zero = 0 * -1\n\n\
         big : BigInt\nlet big = 2n ** 100n\n\n\
         by_zero : BigInt\nlet by_zero = 1n / 0n\n\n\
         rounded : Number\nlet rounded = 2 ** 0.5\n\n\
         pick : Number\nlet pick = if 1 < 2 then 10 else 20\n",
    );
    for expected in [
        "export const day = 86400;",
        "export const text = \"Hello, world\";",
        "export const sum = false;",
        "export const minus_zero = (-0);",
        "export const big = 1267650600228229401496703205376n;",
        "export const by_zero = (1n / 0n);",
        "export const rounded = (2 ** 0.5);",
        "export const pick = 10;",
    ] {
        assert!(js.contains(expected), "{expected} in\n{js}");
    }
}

#[test]
fn unused_pure_code_is_dropped_and_effects_stay() {
    let js = release(
        "module App exposing (main)\n\n\
         helper : Number -> Number\nlet helper x = x * 2 + 1\n\n\
         unused : Number -> Number\nlet unused x = x\n\n\
         main : Unit ->{Log.Log; none} Unit\n\
         let main () =\n  let later = task { Log.info \"never run\" }\n  let ignored = Log.info \"runs\"\n  ignore (helper 2)\n",
    );
    assert!(!js.contains("never run"), "{js}");
    assert!(js.contains("\"runs\""), "{js}");
    assert!(!js.contains("unused"), "{js}");
}

#[test]
fn a_lone_tail_effect_needs_no_generator() {
    let js = release(
        "module App exposing (once, twice, nested)\n\n\
         once : Number ->{} Unit\nlet once n = Platform.stdout_line (show n)\n\n\
         twice : Number ->{} Unit\n\
         let twice n =\n  Platform.stdout_line (show n)\n  Platform.stdout_line (show n)\n\n\
         nested : Unit ->{} Unit\nlet nested () = Platform.stdout_line (show (Platform.env \"HOME\"))\n",
    );
    assert!(
        js.contains("export const once = (n) => Effect.suspend(() => $m_Platform.stdout_line("),
        "{js}"
    );
    // two steps, or an effect inside the arguments, keep the generator
    assert_eq!(js.matches("Effect.gen(function* () {").count(), 2, "{js}");
}

#[test]
fn exported_workers_take_full_applications_across_modules() {
    let workers = |m: &str, name: &str| match (m, name) {
        ("Ui", "div") | ("App", "add") => Some(2),
        _ => None,
    };
    let js = release_with(
        "module App exposing (add, view)\n\n\
         add : Number -> Number -> Number\nlet add a b = a + b\n\n\
         view : Unit -> Ui none none\nlet view () = Ui.div { class_name = \"x\" } []\n",
        &workers,
    );
    assert!(js.contains("export const add$2 = (a, b) =>"), "{js}");
    assert!(
        js.contains("export const add = ($a0) => ($a1) => add$2($a0, $a1);"),
        "{js}"
    );
    assert!(
        js.contains("$m_Ui.div$2({ class_name: \"x\" }, [])"),
        "{js}"
    );
}
