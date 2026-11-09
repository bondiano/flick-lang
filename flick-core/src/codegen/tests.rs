use super::{EffectBridge, Labels, Options, js};
use crate::ir::tests::lower_text;
use crate::types::{Constructor, Rep};

fn compile(text: &str) -> String {
    compile_with(text, false, Labels::default()).code
}

fn compile_with(text: &str, release: bool, labels: Labels) -> super::Output {
    let ir = lower_text("App", text).expect("lowers");
    let path = |m: &str| format!("{}.js", m.replace('.', "/"));
    let package = |m: &str| (m == "App.Users").then(|| "@acme/users".to_owned());
    let foreign = |_: &str, spec: &str| spec.to_owned();
    let options = Options {
        path: &path,
        package: &package,
        foreign: &foreign,
        typescript: false,
        interface: &|_| None,
        release,
        workers: &|_, _| None,
        direct: &|_, _, _| false,
        labels,
        rt: false,
        effect_bridge: EffectBridge::Absent,
    };
    js(&ir, &options)
}

/// The code of `text` on Flick's own runtime, in debug.
fn on_rt(text: &str) -> String {
    on_rt_with_bridge(text, EffectBridge::Absent)
}

fn on_rt_with_bridge(text: &str, effect_bridge: EffectBridge) -> String {
    let ir = lower_text("App", text).expect("lowers");
    let path = |m: &str| format!("{}.js", m.replace('.', "/"));
    let package = |_: &str| None;
    let foreign = |_: &str, spec: &str| spec.to_owned();
    let options = Options {
        path: &path,
        package: &package,
        foreign: &foreign,
        typescript: false,
        interface: &|_| None,
        release: false,
        workers: &|_, _| None,
        direct: &|_, _, _| false,
        labels: Labels::default(),
        rt: true,
        effect_bridge,
    };
    js(&ir, &options).code
}

#[test]
fn effect_bridge_converts_the_public_chunks_underlying_core_type() {
    let code = on_rt_with_bridge(
        "module App exposing (convert)\n\
         convert : Chunk.Chunk Int ->{} Chunk.Chunk Int\n\
         foreign effect convert from \"@flick-lang/effect-ts/effect/TxChunk\" \"convert\"\n",
        EffectBridge::Available,
    );
    assert!(code.contains("chunkToEffect"), "{code}");
    assert!(code.contains("chunkFromEffect"), "{code}");
}

#[test]
fn effect_bridge_converts_optional_effect_values_before_unwrapping_them() {
    let code = on_rt_with_bridge(
        "module App exposing (choose)\n\
         choose : Option (Eff none none Int) -> Eff none none Int\n\
         foreign pure choose from \"./bindings.mjs\" \"choose\"\n  optional 0\n",
        EffectBridge::Available,
    );
    let compact: String = code.split_whitespace().collect();
    assert!(
        compact.contains("optionalArg($bridge_optionToEffect("),
        "{code}"
    );
    assert!(compact.contains("$bridge_callEffect("), "{code}");
}

#[test]
fn on_flick_runtime_a_runtime_stage_is_a_generator_and_a_tail_call_is_returned() {
    let code = on_rt(
        "module App exposing (f, g)\n\
         g : Int ->{Async;} Int\n\
         let g n = n\n\
         f : Int ->{Async;} Int\n\
         let f n =\n  let a = g n\n  g a\n",
    );
    assert!(code.contains("function* (n)"), "{code}");
    assert!(code.contains("const a = yield g(n);"), "{code}");
    assert!(code.contains("return g(a);"), "{code}");
    assert!(!code.contains("Effect."), "{code}");
}

#[test]
fn on_flick_runtime_a_jump_of_a_generator_is_a_step_of_the_driver() {
    let code = on_rt(
        "module App exposing (spin)\n\
         spin : Int ->{Async;} Int\n\
         let rec spin n = if n == 0 then 0 else spin (n - 1)\n",
    );
    assert!(code.contains("yield;\n"), "{code}");
    assert!(code.contains("continue;"), "{code}");
}

#[test]
fn top_level_atoms_views_and_actions_are_labelled_by_their_names() {
    let text = "module App.Counter exposing (..)\n\
         count : Atom.Writable Float\nlet count = Atom.make 0\n\
         reset : Action.Action Ui.ClickSnapshot\n\
         let reset = Action.exhaust (fun _ -> task { Atom.set count 0 })\n\
         counter : Ui.View Unit\n\
         let counter = Ui.view (fun () -> ui { button { disabled = false; on_click = reset } [text (show (Atom.value count))] })\n\
         start : Float\nlet start = 0\n";
    let labels = |release, labels| {
        let code = compile_with(text, release, labels).code;
        assert_eq!(
            code.contains("$rt.label("),
            labels != Labels::None || !release,
            "{code}"
        );
        // a write's `CallSite` names its definition as data, not as a label
        let code = code.replace("definition: \"App.Counter.reset\"", "");
        ["count", "reset", "counter", "start"]
            .into_iter()
            // a long call breaks after `$rt.label(`
            .filter(|name| code.contains(&format!("\"App.Counter.{name}\"")))
            .collect::<Vec<_>>()
    };
    assert_eq!(labels(false, Labels::None), ["count", "reset", "counter"]);
    assert_eq!(labels(true, Labels::All), ["count", "reset", "counter"]);
    assert_eq!(labels(true, Labels::Components), ["reset", "counter"]);
    assert!(labels(true, Labels::None).is_empty());
    #[cfg(feature = "source-maps")]
    {
        let named: Vec<_> = compile_with(text, false, Labels::default())
            .mappings
            .into_iter()
            .filter_map(|(.., name)| name)
            .collect();
        assert_eq!(named, ["count", "reset", "counter", "start"]);
    }
}

#[test]
fn pure_functions_are_plain_js() {
    let js = compile(
        "module App.Text exposing (..)\n\
         identity : a -> a\nlet identity value = value\n\
         greet : String -> String -> String\nlet greet prefix name = prefix ++ \", \" ++ name\n\
         hello : String -> String\nlet hello = greet \"Hello\"\n\
         with_default : a -> Option a -> a\n\
         let with_default fallback value =\n  match value with\n  | None -> fallback\n  | Some x -> x\n",
    );
    assert!(!js.contains("Effect"), "{js}");
    assert!(!js.contains("yield"), "{js}");
    insta::assert_snapshot!(js);
}

#[test]
fn runtime_stages_yield_only_on_runtime_calls() {
    let js = compile(
        "module App.Users exposing (..)\n\
         type alias User = { id : Float; name : String }\n\
         type error NotFound = NotFound Float\n\
         service Users =\n  get : Float ->{none; NotFound} User\n\
         service Log =\n  info : String ->{} Unit\n\
         normalize_name : String -> String\nlet normalize_name name = String.trim name\n\
         load_name : Float ->{Users, Log; NotFound} String\n\
         let load_name id =\n  let user = Users.get id\n  let name = normalize_name user.name\n\n  Log.info name\n\n  name\n",
    );
    insta::assert_snapshot!(js);
    let body = js
        .split("export const load_name")
        .nth(1)
        .expect("load_name");
    assert!(body.contains("Effect.gen(function* ()"), "{body}");
    assert_eq!(body.matches("yield*").count(), 2, "{body}");
}

#[test]
fn constructor_ids_name_package_module_and_type() {
    let js = compile(
        "module App.Users exposing (..)\n\
         type Choice = Some Float | Empty\n",
    );
    // the name, and a hash of package, module, type and name
    let c = |module: &str| Constructor {
        module: module.to_owned(),
        ty: "Choice".to_owned(),
        name: "Some".to_owned(),
        arity: 1,
        rep: Rep::Tagged,
        dicts: 0,
    };
    let tag = super::tag(&c("App.Users"), Some("@acme/users"));
    assert!(
        tag.starts_with("Some#") && tag.len() == "Some#".len() + 12,
        "{tag}"
    );
    assert!(js.contains(&format!("_tag: \"{tag}\"")), "{js}");
    assert_ne!(tag, super::tag(&c("App.Users"), Some("@acme/other")));
    assert_ne!(tag, super::tag(&c("App.Other"), Some("@acme/users")));
    let native =
        compile("module App.Main exposing (..)\nnative : Option Float\nlet native = Some 1\n");
    assert!(
        native.contains("export const native = /*#__PURE__*/ $rt.some(1);"),
        "{native}"
    );
}

#[test]
fn debug_imports_a_module_by_its_own_name_unless_taken() {
    let js = compile(
        "module App.Main exposing (..)\n\n\
         type Shape = Option Float | Dot\n\n\
         line : Unit ->{} Unit\nlet line _ = Platform.stdout_line (show (Option.with_default 0 (Some 1)))\n",
    );
    assert!(js.contains("import * as Platform from"), "{js}");
    assert!(js.contains("Platform.stdout_line("), "{js}");
    // the constructor `Option` keeps the module `Option` apart
    assert!(js.contains("import * as $m_Option from"), "{js}");
    assert!(js.contains("export const line = (_) =>"), "{js}");
}

/// The top-level constant `name` in `js`, up to the next one.
fn export_of<'a>(js: &'a str, name: &str) -> &'a str {
    let start = js
        .find(&format!("const {name} "))
        .unwrap_or_else(|| panic!("no {name} in {js}"));
    let rest = &js[start + 1..];
    let end = [rest.find("\nexport "), rest.find("\nconst ")]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(rest.len());
    &js[start..start + 1 + end]
}

#[test]
fn a_tail_call_assigns_only_the_arguments_it_changes() {
    let js = compile(
        "module App.Loop exposing (..)\n\
         find : (a -> Bool) -> List a -> Option a\n\
         let rec find p xs =\n  match xs with\n  | [] -> None\n  | [x, ...rest] -> if p x then Some x else find p (List.drop 1 rest)\n",
    );
    let find = export_of(&js, "find");
    assert!(find.contains("for (;;)"), "{find}");
    assert!(!find.contains("const $t"), "{find}");
    // only the initialization sets `p`
    assert_eq!(find.matches("$l0 =").count(), 1, "{find}");
    assert!(find.contains("$l1 ="), "{find}");
}

#[test]
fn a_constrained_function_loops_on_a_call_passing_its_own_dictionaries() {
    let js = compile(
        "module App.Loop exposing (..)\n\
         elem : Eq a => a -> List a -> Bool\n\
         let rec elem y xs =\n  match xs with\n  | [] -> false\n  | [x, ...rest] -> if x == y then true else elem y rest\n\
         nest : forall a. Show a => Float -> a -> String\n\
         let rec nest n x = if n == 0 then show x else nest (n - 1) [x]\n",
    );
    let elem = export_of(&js, "elem");
    assert!(elem.contains("for (;;)"), "{elem}");
    assert!(!elem.contains("elem($d0)"), "{elem}");
    // polymorphic recursion passes another dictionary: a call, not a jump
    let nest = export_of(&js, "nest");
    assert!(!nest.contains("for (;;)"), "{nest}");
}

#[test]
fn the_right_operand_of_and_and_or_is_a_tail_position() {
    let js = compile(
        "module App.Loop exposing (..)\n\
         all : (a -> Bool) -> List a -> Bool\n\
         let rec all p xs =\n  match xs with\n  | [] -> true\n  | [x, ...rest] -> p x && all p rest\n\
         any : (a -> Bool) -> List a -> Bool\n\
         let rec any p xs =\n  match xs with\n  | [] -> false\n  | [x, ...rest] -> p x || any p rest\n",
    );
    let all = export_of(&js, "all");
    assert!(
        all.contains("if (!p(x)) {\n") && all.contains("return false;"),
        "{all}"
    );
    assert!(!all.contains("all(p)"), "{all}");
    let any = export_of(&js, "any");
    assert!(
        any.contains("if (p(x)) {\n") && any.contains("return true;"),
        "{any}"
    );
    assert!(!any.contains("any(p)"), "{any}");
}

#[test]
fn mutually_tail_recursive_functions_run_as_one_loop() {
    let text = "module App.Parity exposing (..)\n\
         rec {\n\
         \x20 is_even : Float -> Bool\n  let is_even n = if n == 0 then true else is_odd (n - 1)\n\
         \x20 is_odd : Float -> Bool\n  let is_odd n = if n == 0 then false else is_even (n - 1)\n\
         }\n\
         rec {\n\
         \x20 a : Float -> Float -> Float\n  let a acc n = if n == 0 then acc else b (acc + 1) (n - 1)\n\
         \x20 b : Float -> Float -> Float\n  let b acc n = if n == 0 then acc else c acc (n - 1)\n\
         \x20 c : Float -> Float -> Float\n  let c acc n = if n == 0 then acc else a (a 0 acc) (n - 1)\n\
         }\n";
    for release in [false, true] {
        let js = compile_with(text, release, Labels::default()).code;
        let parity = js.split("export const").next().unwrap_or_default();
        assert!(parity.contains("const is_even$rec = ($f, $l0) =>"), "{js}");
        assert!(
            parity.contains("switch ($f)") && parity.contains("$f = 1;"),
            "{js}"
        );
        assert!(!parity.contains("is_odd("), "{js}");
        let group = &js[js.find("const a$rec").expect("a group")..];
        let group = &group[..group.find("\nexport").unwrap_or(group.len())];
        // the inner `a 0 acc` is not in tail position: it calls the member
        let inner = if release { "a$2(0, acc)" } else { "a(0)(acc)" };
        assert!(
            group.contains("$f = 2;") && group.contains(inner),
            "{group}"
        );
        let calls_a = if release {
            "a$rec(0, acc, n)"
        } else {
            "(n) => a$rec(0, acc, n)"
        };
        assert!(js.contains(calls_a), "{js}");
    }
}

#[test]
fn a_loop_walks_a_list_by_offset_unless_its_rest_escapes() {
    let text = "module App.Walk exposing (..)\n\
         pairs : Float -> List Float -> Float\n\
         let rec pairs acc xs =\n  match xs with\n  | [a, b, ...rest] -> pairs (acc + a * b) rest\n  | [a] -> acc + a\n  | _ -> acc\n\
         restart : Float -> List Float -> Float\n\
         let rec restart n xs =\n  match xs with\n  | [] -> if n == 0 then 0 else restart (n - 1) [1, 2]\n  | [_, ...rest] -> restart n rest\n\
         sum : List Float -> Float\n\
         let rec sum xs =\n  match xs with\n  | [] -> 0\n  | [x, ...rest] -> x + sum rest\n\
         total : Float -> List Float -> Float\n\
         let rec total acc xs =\n  match xs with\n  | [] -> acc\n  | [x, ...rest] -> if x > 9 then acc + sum rest else total (acc + x) rest\n\
         rec {\n\
         \x20 even_len : List a -> Bool\n  let even_len xs =\n    match xs with\n    | [] -> true\n    | [_, ...rest] -> odd_len rest\n\
         \x20 odd_len : List a -> Bool\n  let odd_len xs =\n    match xs with\n    | [] -> false\n    | [_, ...rest] -> even_len rest\n\
         }\n";
    for release in [false, true] {
        let js = compile_with(text, release, Labels::default()).code;
        let pairs = export_of(&js, if release { "pairs$2" } else { "pairs" });
        assert!(!pairs.contains("slice("), "{pairs}");
        assert!(
            pairs.contains("xs[$o1 + 1]") && pairs.contains("$o1 = $o1 + 2;"),
            "{pairs}"
        );
        // another list on the walked position starts at its beginning
        let restart = export_of(&js, if release { "restart$2" } else { "restart" });
        assert!(
            restart.contains("$l1 = [1, 2];\n") && restart.contains("$o1 = 0;"),
            "{restart}"
        );
        assert!(!restart.contains("slice("), "{restart}");
        let group = &js[js.find("const even_len$rec").expect("a group")..];
        assert!(
            group.contains("let $o0 = 0;") && !group.contains("slice("),
            "{group}"
        );
        // `rest` is a value of its own outside a loop, or when it escapes one
        let total = export_of(&js, if release { "total$2" } else { "total" });
        assert!(total.contains("slice(1)"), "{total}");
        assert!(export_of(&js, "sum").contains("slice(1)"), "{js}");
    }
}

#[test]
fn an_atom_write_names_its_place() {
    let text = "module App.Counter exposing (..)\n\
         count : Atom.Writable Float\nlet count = Atom.make 0\n\
         reset : Action.Action Ui.ClickSnapshot\n\
         let reset = Action.exhaust (fun _ -> task {\n  Atom.set count 0\n  Atom.update count (fun n -> n + 1)\n})\n";
    let code = compile(text);
    // each write's `CallSite` is the record of its own place
    assert!(code.contains("place: \"App.Counter:6\""), "{code}");
    assert!(code.contains("place: \"App.Counter:7\""), "{code}");
}

#[test]
fn effect_bridge_does_not_treat_a_user_api_type_as_http_handlers() {
    let code = on_rt_with_bridge(
        "module App exposing (use)\n\ntype Api a = Api a\n\n\
         use : Api a -> a -> a\n\
foreign pure use from \"@flick-lang/effect-ts\" \"use\"\n  arity 2\n",
        EffectBridge::Available,
    );
    assert!(!code.contains("handlersToEffect"), "{code}");
}

#[test]
fn effect_bridge_adapts_the_handler_record_of_an_rpc_group() {
    let code = on_rt_with_bridge(
        "module App exposing (use)\n\nuse : Rpc.Group r hs -> hs -> Unit\n\
foreign pure use from \"@flick-lang/effect-ts\" \"use\"\n  arity 2\n",
        EffectBridge::Available,
    );
    assert!(code.contains("handlersToEffect"), "{code}");
}
