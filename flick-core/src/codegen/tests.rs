use std::collections::{BTreeMap, HashMap, HashSet};

use super::test_options::resolved_by;
use super::{EffectBridge, Known, Labels, Options, js};
use crate::effects::effect_specifier;
use crate::ir::lang_items;
use crate::ir::tests::lower_text;
use crate::types::{Constructor, Rep};

mod layout;

fn compile(text: &str) -> String {
    compile_with(text, false, Labels::default()).code
}

pub(super) fn compile_with(text: &str, release: bool, labels: Labels) -> super::Output {
    let ir = lower_text("App", text).expect("lowers");
    let packages = BTreeMap::from([("App.Users".to_owned(), "@acme/users".to_owned())]);
    // as a build on Effect has it: a key is the module of the adapter
    let resolved = resolved_by(&ir, &effect_specifier);
    let lang = lang_items();
    let known = Known::of(&lang);
    let options = Options {
        builtins: &[],
        paths: None,
        packages: &packages,
        resolved: &resolved,
        typescript: None,
        release,
        workers: &HashMap::new(),
        direct: &HashSet::new(),
        labels,
        rt: false,
        effect_bridge: EffectBridge::Absent,
        lang: &known,
    };
    js(&ir, &options)
}

#[test]
fn a_while_nothing_leaves_is_a_plain_loop_in_javascript() {
    let text = "module App exposing (sum)\n\nsum : Int -> Int\nlet sum n =\n  let mut acc = 0\n  \
                let mut i = n\n  while i > 0 do\n    acc = acc + i\n    i = i - 1\n  acc\n";
    let plain = compile(text);
    assert!(plain.contains("if (!(i > 0)) break;"), "{plain}");
    assert!(!plain.contains("$loop"), "{plain}");
    // a local variable and a loop need nothing of the runtime
    let rt = on_rt(text);
    assert!(rt.contains("let acc = 0") && !rt.contains("import"), "{rt}");
    // an exit still needs the flags that carry it out of nested blocks
    let left = compile(&text.replace(
        "    i = i - 1\n",
        "    i = i - 1\n    if acc > 100 then break\n",
    ));
    assert!(left.contains("$loop"), "{left}");
}

/// The code of `text` on Flick's own runtime, in debug.
fn on_rt(text: &str) -> String {
    on_rt_with_bridge(text, EffectBridge::Absent)
}

fn on_rt_with_bridge(text: &str, effect_bridge: EffectBridge) -> String {
    let ir = lower_text("App", text).expect("lowers");
    // as a build on Effect has it: a key is the module of the adapter
    let resolved = resolved_by(&ir, &effect_specifier);
    let lang = lang_items();
    let known = Known::of(&lang);
    let options = Options {
        builtins: &[],
        paths: None,
        packages: &BTreeMap::new(),
        resolved: &resolved,
        typescript: None,
        release: false,
        workers: &HashMap::new(),
        direct: &HashSet::new(),
        labels: Labels::default(),
        rt: true,
        effect_bridge,
        lang: &known,
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
         let counter = Ui.view (fun () -> ui { button { disabled = false; on_click = reset } (text (show (Atom.value count))) })\n\
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
    #[cfg(feature = "native-project")]
    {
        let named: Vec<_> = compile_with(text, false, Labels::default())
            .mappings
            .into_iter()
            .filter_map(|(.., name)| name)
            .collect();
        assert_eq!(named, ["count", "reset", "counter", "start"]);
    }
}

/// A label is a mark of the type, which any module may give its own.
#[test]
fn a_value_of_a_type_marked_label_is_labelled_by_its_name() {
    let text = "module App.Gauges exposing (..)\n\
         @label gauge\ntype Gauge = Gauge Float\n\
         type Plain = Plain Float\n\
         level : Gauge\nlet level = Gauge 1\n\
         plain : Plain\nlet plain = Plain 1\n\
         raise : Gauge -> Gauge\nlet raise g = g\n";
    let labelled = |release, labels| {
        let code = compile_with(text, release, labels).code;
        ["level", "plain", "raise"]
            .into_iter()
            .filter(|name| code.contains(&format!("\"App.Gauges.{name}\"")))
            .collect::<Vec<_>>()
    };
    assert_eq!(labelled(false, Labels::None), ["level"]);
    assert_eq!(labelled(true, Labels::All), ["level"]);
    // a release build that keeps the components' alone drops another kind
    assert!(labelled(true, Labels::Components).is_empty());
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

include!("tests/tail_calls_and_bridge.rs");
