use crate::build::builtin_modules;
use crate::codegen::{Labels, Options, js};
use crate::ir::Expr;
use crate::ir::tests::check_text;
use crate::types::builtins;

/// A generic function of a built-in module, as a build offers it.
fn builtin_generic(module: &str, name: &str) -> Option<(Vec<String>, Expr)> {
    let ir = builtin_modules().iter().find(|m| m.name == module)?;
    let interface = builtins().iter().find(|i| i.module == module)?;
    super::names_only(ir, name, &|n| interface.exports_value(n))
}

/// The JS of a self-contained module in the release profile.
fn release(text: &str) -> String {
    release_with(text, &|_, _| None)
}

/// `release` where `workers` names the workers modules export.
fn release_with(text: &str, workers: &dyn Fn(&str, &str) -> Option<usize>) -> String {
    let (mut ir, interface) = check_text("App", text).expect("lowers");
    super::optimize(
        &mut ir,
        &|name| interface.exports_value(name),
        &builtin_generic,
    );
    let path = |m: &str| format!("{}.js", m.replace('.', "/"));
    let options = Options {
        path: &path,
        package: &|_| None,
        foreign: &|_, spec| spec.to_owned(),
        typescript: false,
        interface: &|m| (m == interface.module).then(|| interface.clone()),
        release: true,
        workers,
        direct: &|_, _, _| false,
        labels: Labels::default(),
    };
    js(&ir, &options).code
}

#[test]
fn sum_pair_allocates_no_object() {
    let js = release(
        "module App exposing (sum_pair, total)\n\n\
         sum_pair : Float -> Float -> Float\n\
         let sum_pair left right =\n  let pair = { left = left; right = right }\n  pair.left + pair.right\n\n\
         total : Float -> Float\nlet total n = sum_pair n 1\n",
    );
    insta::assert_snapshot!(js);
}

#[test]
fn a_full_application_calls_the_worker_and_a_partial_one_keeps_the_closure() {
    let js = release(
        "module App exposing (add, both, inc, staged, first)\n\n\
         add : Float -> Float -> Float\nlet add left right =\n  if left > right then left - right else left + right\n\n\
         both : Float -> Float\nlet both x = add x 2\n\n\
         inc : Float -> Float\nlet inc = add 1\n\n\
         staged : BigInt -> BigInt -> BigInt\n\
         let staged x =\n  let checked = 2n ** x\n  fun y -> checked + y\n\n\
         first : BigInt -> BigInt\nlet first y = staged (-1n) y\n",
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
    assert!(js.contains("const checked$i1 = 2n ** (-1n);"), "{js}");
}

#[test]
fn a_canonical_instance_is_called_directly_and_a_witness_stays_chosen() {
    let js = release(
        "module App exposing (..)\n\n\
         type Shape = Circle Int | Square Int\n\n\
         instance Show Shape =\n  let show s =\n    match s with\n    | Circle _ -> \"circle\"\n    | Square _ -> \"square\"\n\n\
         label : Shape -> String\nlet label s = show s\n\n\
         by_size : Shape -> Int\nlet by_size s =\n  match s with\n  | Circle r -> r\n  | Square r -> r\n\n\
         count : List Shape -> Int\n\
         let count shapes =\n  EqHash.with (fun a b -> by_size a == by_size b) by_size (fun witness ->\n    HashSet.size (HashSet.from_list witness shapes)\n  )\n",
    );
    assert!(
        js.contains("export const label = (s) => $i_Show_Shape$show(s);"),
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
         service Clock =\n  now : Unit ->{} Float\n\n\
         sum : Float -> Float -> Float -> Float\nlet sum a b c = (a + b) + c\n\n\
         zero : Float -> Float\nlet zero x = x * 0\n\n\
         twice : Eff none none Float -> Unit ->{} Float\nlet twice t () =\n  let a = await t\n  let b = await t\n  a + b\n\n\
         layers : Unit -> (Layer none none {Clock}, Layer none none {Clock})\n\
         let layers () =\n  (Layer.succeed Clock { now = fun () -> 1 }, Layer.succeed Clock { now = fun () -> 1 })\n\n\
         read : Layer none none {Clock} -> Unit ->{} Float\n\
         let read layer () =\n  provide layer {\n    Clock.now ()\n  }\n\n\
         checked : Unknown -> Unit\nlet checked input =\n  let decoded = Codec.decode_result (Codec.json Schema.number) input\n  ()\n",
    );
    // no reassociation (`a + b + c` is `(a + b) + c`), no `x * 0 -> 0`
    assert!(js.contains("a + b + c"), "{js}");
    assert!(js.contains("x * 0"), "{js}");
    // a task runs as often as it is awaited
    assert_eq!(js.matches("yield* t;").count(), 2, "{js}");
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
         day : Float\nlet day = 60 * 60 * 24\n\n\
         text : String\nlet text = \"Hello, \" ++ \"world\"\n\n\
         sum : Bool\nlet sum = 0.1 + 0.2 == 0.3\n\n\
         minus_zero : Float\nlet minus_zero = 0 * -1\n\n\
         big : BigInt\nlet big = 2n ** 100n\n\n\
         by_zero : BigInt\nlet by_zero = 1n / 0n\n\n\
         rounded : Float\nlet rounded = 2 ** 0.5\n\n\
         pick : Float\nlet pick = if 1 < 2 then 10 else 20\n",
    );
    for expected in [
        "export const day = 86400;",
        "export const text = \"Hello, world\";",
        "export const sum = false;",
        "export const minus_zero = -0;",
        "export const big = 1267650600228229401496703205376n;",
        "export const by_zero = 0n;",
        "export const rounded = 2 ** 0.5;",
        "export const pick = 10;",
    ] {
        assert!(js.contains(expected), "{expected} in\n{js}");
    }
}

#[test]
fn unused_pure_code_is_dropped_and_effects_stay() {
    let js = release(
        "module App exposing (main)\n\n\
         helper : Float -> Float\nlet helper x = x * 2 + 1\n\n\
         unused : Float -> Float\nlet unused x = x\n\n\
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
         once : Float ->{} Unit\nlet once n = Platform.stdout_line (show n)\n\n\
         twice : Float ->{} Unit\n\
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
         add : Float -> Float -> Float\nlet add a b = a + b\n\n\
         view : Unit -> Ui\nlet view () = Ui.div { class_name = \"x\" } []\n",
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

#[test]
fn equality_whose_identity_decides_is_strict() {
    let js = release(
        "module App exposing (same)\n\n\
         type Locale = En | Ru\n\n\
         same : String -> Locale -> Float -> Bool -> Bool\n\
         let same s l n b = s == \"a\" && l != Ru && n == 1 && b == true\n",
    );
    assert!(js.contains("s === \"a\" &&"), "{js}");
    assert!(js.contains("&& l._tag !== Ru._tag &&"), "{js}");
    // SameValueZero on Float stays the runtime's
    assert!(js.contains("$rt.equals(n, 1)"), "{js}");
    assert!(js.contains("&& b === true"), "{js}");
}

#[test]
fn known_instances_and_plain_foreigns_are_called_directly() {
    let direct = |m: &str, instance: &str, method: &str| {
        (m, instance, method) == ("Instances", "Show Float", "show")
    };
    let workers = |_: &str, _: &str| None;
    let (mut ir, interface) = check_text(
        "App",
        "module App exposing (label, both)\n\n\
         label : Float -> String\nlet label n = show n\n\n\
         join : String -> String -> String\nforeign pure join from \"./x.mjs\" \"join\"\n\n\
         both : String -> String\nlet both s = join s s\n",
    )
    .expect("lowers");
    super::optimize(
        &mut ir,
        &|name| interface.exports_value(name),
        &builtin_generic,
    );
    let options = Options {
        path: &|m| format!("{}.js", m.replace('.', "/")),
        package: &|_| None,
        foreign: &|_, spec| spec.to_owned(),
        typescript: false,
        interface: &|m| (m == interface.module).then(|| interface.clone()),
        release: true,
        workers: &workers,
        direct: &direct,
        labels: Labels::default(),
    };
    let js = js(&ir, &options).code;
    assert!(js.contains("$m_Instances.$i_Show_Float$show(n)"), "{js}");
    assert!(!js.contains(".show("), "{js}");
    assert!(js.contains("$f_join(s, s)"), "{js}");
}

#[test]
fn matches_switch_on_tags_and_skip_the_last_test() {
    let js = release(
        "module App exposing (area, name, unit)\n\n\
         type Shape = Circle Float | Rect Float Float | Dot\n\n\
         area : Shape -> Float\n\
         let area s =\n  match s with\n  | Circle r -> r * r\n  | Rect w h -> w * h\n  | Dot -> 0\n\n\
         name : Shape -> String\n\
         let name s =\n  match s with\n  | Circle _ -> \"circle\"\n  | _ -> \"other\"\n\n\
         unit : Unit -> Float\nlet unit () = area (Rect 2 3)\n",
    );
    assert!(js.contains("switch (s._tag) {"), "{js}");
    assert!(js.contains("case \"Circle#"), "{js}");
    assert!(js.contains("default: {"), "{js}");
    // two arms: one test, the last arm runs whatever reaches it
    assert_eq!(js.matches("s._tag ===").count(), 1, "{js}");
    assert!(js.contains("{ _tag: \"Rect#"), "{js}");
    assert!(js.contains("_0: 2, _1: 3 }"), "{js}");
}

#[test]
fn a_worker_names_each_ignored_parameter_apart() {
    let workers = |m: &str, name: &str| ((m, name) == ("App", "first")).then_some(3);
    let js = release_with(
        "module App exposing (first)\n\n\
         first : Float -> String -> Bool -> Float\nlet first n _ _ = n\n",
        &workers,
    );
    assert!(
        js.contains("export const first$3 = (n, $_, $_2) => n;"),
        "{js}"
    );
}

#[test]
fn a_plain_foreign_named_as_a_value_is_defined_in_its_module() {
    let workers = |m: &str, name: &str| ((m, name) == ("App", "join")).then_some(2);
    let js = release_with(
        "module App exposing (join, both, pick)\n\n\
         join : String -> String -> String\nforeign pure join from \"./x.mjs\" \"join\"\n\n\
         upper : String -> String\nforeign pure upper from \"./x.mjs\" \"upper\"\n\n\
         both : List String -> List String\nlet both xs = List.map upper xs\n\n\
         pick : (String -> String -> String)\nlet pick = join\n",
        &workers,
    );
    assert!(js.contains("export const upper = $f_upper;"), "{js}");
    assert!(js.contains("export const join$2 = $f_join;"), "{js}");
    assert!(
        js.contains("export const join = ($a0) => ($a1) => join$2($a0, $a1);"),
        "{js}"
    );
}

#[test]
fn show_of_a_literal_folds_into_the_string() {
    let js = release(
        "module App exposing (main)\n\n\
         main : Unit ->{; AppError} Unit\n\
         let main () =\n  let test = 2 + 3\n  Platform.stdout_line (\"Hello from Flick\" ++ show test)\n",
    );
    assert!(js.contains("stdout_line(\"Hello from Flick5\")"), "{js}");
    assert!(!js.contains("$show"), "{js}");
}

#[test]
fn show_folds_as_the_runtime_spells_it() {
    let js = release(
        "module App exposing (shown, fraction)\n\n\
         shown : String\nlet shown = show true ++ show \"a\\n\\\"\" ++ show 12n ++ show (-0.0)\n\n\
         fraction : String\nlet fraction = show (10.0 + 10.01) ++ \" \" ++ show (1000000000.0 * 1000000000000.0) \
         ++ \" \" ++ show (1.0 / 10000000.0) ++ \" \" ++ show 1658206780088562.25\n",
    );
    assert!(
        js.contains(r#"export const shown = "true\"a\\n\\\"\"12n-0";"#),
        "{js}"
    );
    // an exact tie spells the even digit, as JS does
    assert!(
        js.contains(
            r#"export const fraction = "20.009999999999998 1e+21 1e-7 1658206780088562.2";"#
        ),
        "{js}"
    );
}

#[test]
fn non_finite_floats_fold_to_the_js_globals() {
    let js = release(
        "module App exposing (shown, named, sum, neg)\n\n\
         shown : String\nlet shown = show (1.0 / 0.0) ++ show (-1.0 / 0.0) ++ show (0.0 / 0.0)\n\n\
         named : String\nlet named = show Float.infinity ++ show Float.nan\n\n\
         sum : Float\nlet sum = Float.infinity + 1.0\n\n\
         neg : Float\nlet neg = 0.0 - 1.0 / 0.0\n",
    );
    assert!(
        js.contains(r#"export const shown = "Infinity-InfinityNaN";"#),
        "{js}"
    );
    assert!(
        js.contains(r#"export const named = "InfinityNaN";"#),
        "{js}"
    );
    assert!(js.contains("export const sum = Infinity;"), "{js}");
    assert!(js.contains("export const neg = -Infinity;"), "{js}");
}

#[test]
fn a_private_literal_constant_inlines_at_its_uses() {
    let js = release(
        "module App exposing (label)\n\n\
         size : Float\nlet size = 128 * 2\n\n\
         label : Unit -> String\nlet label () = \"w\" ++ show size\n",
    );
    assert!(js.contains("\"w256\""), "{js}");
    assert!(!js.contains("const size"), "{js}");
}

#[test]
fn combinators_of_a_for_fuse_into_its_loop() {
    let js = release(
        "module App exposing (chain, listed, firsts, nested)\n\n\
         chain : List Float -> List Float\n\
         let chain xs = Seq.to_list (Seq.map (fun x -> x * 10) (Seq.filter (fun x -> x > 1) (for { x <- xs } yield x)))\n\n\
         listed : List Float -> List Float\n\
         let listed xs = List.map (fun y -> y + 1) (List.filter (fun y -> y > 2) [for x <- xs yield x * 2])\n\n\
         firsts : Seq Int\nlet firsts = Seq.take 3 (Seq.flat_map (fun x -> Seq.of_list [x, x]) (for { x <- Seq.from 1 } yield x))\n\n\
         nested : List Float -> List (Float, Float)\n\
         let nested xs = [for { x <- for { y <- xs; if y > 0 } yield y * 2; z <- [1, 2] } yield (x, z)]\n",
    );
    insta::assert_snapshot!(js);
}

#[test]
fn a_known_function_fuses_and_a_sync_for_is_left() {
    let js = release(
        "module App exposing (doubled, read)\n\n\
         double : Float -> Float\nlet double x = x * 2\n\n\
         doubled : List Float -> List Float\nlet doubled xs = List.map double [for x <- xs yield x + 1]\n\n\
         count : Atom.Writable Float\nlet count = Atom.make 1\n\n\
         read : Unit ->{Atom.Read;} List Float\n\
         let read () = Seq.to_list (Seq.map double (for { x <- [1, 2] } yield x + Atom.value count))\n",
    );
    assert!(js.contains("$t1.push((x + 1) * 2);"), "{js}");
    // its steps read the services it captured, so they stay in its generator
    assert!(js.contains("$m_Seq.map(double)($rt.seqIn("), "{js}");
}

#[test]
fn a_strict_for_that_may_throw_is_built_before_its_consumer() {
    let js = release(
        "module App exposing (first, powers, sums)\n\n\
         first : Option BigInt\n\
         let first = Seq.first (for x <- [for y <- [1n, 0n] yield 2n ** (0n - y)] yield x)\n\n\
         powers : List BigInt\n\
         let powers = List.map (fun x -> 2n ** (0n - x)) [for y <- [1n, 0n] yield 2n ** (0n - y)]\n\n\
         sums : List BigInt -> List BigInt\n\
         let sums xs = List.map (fun x -> 2n ** x) [for y <- xs yield y + 1n]\n",
    );
    // the negative powers run, in order, before anything reads the list
    assert_eq!(js.matches(".push(2n ** (0n - y))").count(), 2, "{js}");
    // a producer that cannot throw still fuses with a callback that may
    assert!(js.contains(".push(2n ** (y + 1n))"), "{js}");
}

#[test]
fn a_numeric_function_is_copied_for_the_instances_it_is_called_with() {
    let js = release(
        "module App exposing (both)\n\n\
         import Numeric exposing (Numeric)\n\n\
         scale : Numeric a => a -> a -> a\nlet scale k x = k * x + 1\n\n\
         both : (Int, Float) -> (Int, Float)\nlet both (i, f) = (scale 3 i, scale 2 f)\n",
    );
    assert!(
        js.contains("scale$Numeric_Int = (k) => (x) => Math.imul(k, x) + 1 | 0"),
        "{js}"
    );
    assert!(
        js.contains("scale$Numeric_Float = (k) => (x) => k * x + 1"),
        "{js}"
    );
    // inlined, the copies leave plain arithmetic at the calls
    assert!(
        js.contains("return [Math.imul(3, i) + 1 | 0, 2 * f + 1];"),
        "{js}"
    );
}

#[test]
fn a_numeric_function_of_another_module_is_copied_too() {
    let js = release(
        "module App exposing (total)\n\n\
         total : List Int -> Int\nlet total xs = List.sum xs\n",
    );
    assert!(js.contains("List$Core$sum$Numeric_Int"), "{js}");
    assert!(js.contains("total + x | 0"), "{js}");
    assert!(!js.contains("$i_Numeric_Int"), "{js}");
}
