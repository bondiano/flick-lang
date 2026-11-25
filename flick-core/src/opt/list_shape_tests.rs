//! `List.length`, `List.get` and `List.head` of a list literal, folded in
//! release through the reviewed list primitives: each element that is not
//! data still runs once, in order, whether it is selected or not.

use super::eval_primitives_tests::emit_js_with_reviewed_primitives;
use super::tests::{builtin_arity, builtin_generic, release};
use crate::build::builtin_modules;
use crate::codegen::{EffectBridge, Known};
use crate::ir::tests::check_text;
use crate::ir::{lang_items, print};
use flick_backend_support::opt::eval::{
    EvalConfig, NativePrimitive, PrimitiveEval, ReviewedPrimitives,
};

/// `note` writes its argument when it is evaluated; `boom` fails.
const HEAD: &str = "module App exposing (..)\n\ntype error Boom = Boom String\n\n\
note : Int ->{Host.Console} Int\nlet note n =\n  Platform.stdout_line (show n)\n  n\n\n\
boom : String ->{; Boom} Int\nlet boom label = Fail.fail (Boom label)\n\n\
spin : Int -> Int\nlet rec spin n = spin n\n\n";

fn js_prims() -> ReviewedPrimitives {
    let table = lang_items();
    let known = Known::of(&table);
    ReviewedPrimitives::for_js(
        builtin_modules(),
        &|_| true,
        &known,
        false,
        EffectBridge::Absent,
        &|_, _| None,
    )
}

/// The release JS of `HEAD` followed by `decls`.
fn js(decls: &str) -> String {
    emit_js_with_reviewed_primitives(&format!("{HEAD}{decls}"), &EvalConfig::default()).0
}

/// The optimized IR of the value `name` declared in `decls`.
fn ir(decls: &str, name: &str) -> String {
    let (mut ir, interface) = check_text("App", &format!("{HEAD}{decls}")).expect("lowers");
    super::optimize_with_primitives(
        &mut ir,
        &|name: &str| interface.exports_value(name),
        &builtin_generic,
        &builtin_arity,
        &js_prims(),
    );
    let printed = print(&ir);
    let start = format!("let {name} = ");
    let line = printed.lines().find(|l| l.starts_with(&start));
    line.unwrap_or_else(|| panic!("no {name} in\n{printed}"))
        .to_owned()
}

/// The declaration of `name` in `js`, up to the blank line after it.
pub(super) fn decl<'a>(js: &'a str, name: &str) -> &'a str {
    let start = format!("export const {name} = ");
    let at = js
        .find(&start)
        .unwrap_or_else(|| panic!("no {name} in\n{js}"));
    let rest = &js[at..];
    rest.split("\n\n").next().unwrap_or(rest).trim_end()
}

const KNOWN: &str = "\
len : Int -> Int -> Int\nlet len x y = List.length [x, y]\n\n\
zero : Int\nlet zero = List.length []\n\n\
sel : Int -> Option Int\nlet sel x = List.get 1 [10, x, 20]\n\n\
empty : Option Int\nlet empty = List.get 0 []\n\n\
neg : Int -> Option Int\nlet neg x = List.get (-1) [x]\n\n\
beyond : Int -> Option Int\nlet beyond x = List.get 3 [x, 1, 2]\n\n\
hd : String -> Option String\nlet hd s = List.head [s, \"b\"]\n\n\
dyn_index : Int -> Int -> Option Int\nlet dyn_index i x = List.get i [x]\n\n\
dyn_list : List Int -> Int\nlet dyn_list xs = List.length xs\n";

#[test]
fn lengths_and_lookups_of_list_literals_are_known() {
    let js = js(KNOWN);
    for (name, body) in [
        ("len", "(x) => (y) => 2;"),
        ("zero", "0;"),
        ("sel", "(x) => $rt.some(x);"),
        ("empty", "$rt.none;"),
        ("neg", "(x) => $rt.none;"),
        ("beyond", "(x) => $rt.none;"),
        ("hd", "(s) => $rt.some(s);"),
    ] {
        assert_eq!(decl(&js, name), format!("export const {name} = {body}"));
    }
    assert!(decl(&js, "dyn_index").contains("get("), "{js}");
    assert!(decl(&js, "dyn_list").contains("length(xs)"), "{js}");
    assert_eq!(ir(KNOWN, "sel"), "let sel = (fun x -> (Some x))");
    assert_eq!(ir(KNOWN, "len"), "let len = (fun x y -> 2)");
}

#[test]
fn without_reviewed_primitives_the_calls_stay() {
    let js = release(&format!("{HEAD}{KNOWN}"));
    assert!(decl(&js, "len").contains("length(["), "{js}");
    assert!(decl(&js, "sel").contains("get(1)"), "{js}");
}

#[test]
fn every_element_runs_once_in_order_selected_or_not() {
    let js = js("\
traced : Int ->{Host.Console} Option Int\nlet traced x = List.get 1 [note 1, x, note 3]\n\n\
missed : Int ->{Host.Console} Option Int\nlet missed x = List.get 5 [note 4, x]\n\n\
counted : Int ->{Host.Console} Int\nlet counted x = List.length [x, note 5]\n\n\
divided : Int -> Int -> Option Int\nlet divided x y = List.get 0 [x, Int.unsafe_div 1 y]\n\n\
failed : Int ->{; Boom} Option Int\nlet failed x = List.get 0 [x, boom \"late\"]\n\n\
spun : Int -> Option Int\nlet spun x = List.get 0 [x, spin 0]\n\n\
headed : Int ->{Host.Console} Option Int\nlet headed x = List.head [note 6, x]\n");
    let traced = decl(&js, "traced");
    let (one, three) = (traced.find("note(1)"), traced.find("note(3)"));
    assert!(one.is_some() && one < three, "{traced}");
    assert_eq!(traced.matches("note(").count(), 2, "{traced}");
    assert!(
        traced.contains("$rt.some(x)") && !traced.contains("get("),
        "{traced}"
    );
    let missed = decl(&js, "missed");
    assert_eq!(missed.matches("note(4)").count(), 1, "{missed}");
    assert!(
        missed.contains("$rt.none") && !missed.contains("get("),
        "{missed}"
    );
    let counted = decl(&js, "counted");
    assert_eq!(counted.matches("note(5)").count(), 1, "{counted}");
    assert!(counted.contains("return 2;"), "{counted}");
    let headed = decl(&js, "headed");
    assert_eq!(headed.matches("note(6)").count(), 1, "{headed}");
    assert!(
        headed.contains("$rt.some($p") && !headed.contains("get("),
        "{headed}"
    );
    for (name, kept) in [
        ("divided", "unsafe_div"),
        ("failed", "boom("),
        ("spun", "spin(0)"),
    ] {
        let d = decl(&js, name);
        assert_eq!(d.matches(kept).count(), 1, "{d}");
        assert!(d.contains("$rt.some(x)") && !d.contains("get("), "{d}");
    }
}

#[test]
fn a_folded_option_is_typed_by_its_element() {
    let printed = validated(&format!(
        "{HEAD}type alias Mon = {{ name : String; hp : Int }}\n\n\
         named : String -> Option String\nlet named s = List.get 1 [\"a\", s]\n\n\
         nameless : String -> Option String\nlet nameless s = List.get 2 [\"a\", s]\n\n\
         mon : String ->{{Host.Console}} Option Mon\n\
         let mon n = List.head [{{ name = n; hp = note 2 }}]\n\n\
         any : a -> Option a\nlet any x = List.get 0 [x]\n"
    ));
    assert!(!printed.contains("List.Core.get"), "{printed}");
}

/// The printed IR of `text` optimized with the reviewed JS primitives,
/// after the IR validator accepted it.
pub(super) fn validated(text: &str) -> String {
    use std::collections::BTreeSet;

    use crate::types;
    use flick_ir::Program;

    let (mut module, interface) = check_text("App", text).expect("lowers");
    super::optimize_with_primitives(
        &mut module,
        &|name: &str| interface.exports_value(name),
        &builtin_generic,
        &builtin_arity,
        &js_prims(),
    );
    let printed = print(&module);
    let builtin = |m: &str| types::builtins().iter().any(|b| b.module == m);
    let mut files = BTreeSet::new();
    module.clone().each_span_mut(&mut |s| {
        files.insert(s.file);
    });
    let program = Program {
        ir_version: flick_ir::VERSION.into(),
        compiler: String::new(),
        files: Vec::new(),
        modules: vec![module],
        lang: lang_items(),
    };
    let problems = flick_ir::validate(&program, &builtin, &files);
    assert!(problems.is_empty(), "{problems:#?}\n{printed}");
    printed
}

#[test]
fn option_constructors_stay_validated_for_get_without_int_parse() {
    let table = lang_items();
    let known = Known::of(&table);
    let without_parse =
        ReviewedPrimitives::build(builtin_modules(), &|_| true, &known, &|_, _, p| {
            p != NativePrimitive::IntParse
        });
    assert_eq!(
        without_parse.list_primitive("List.Core", "get"),
        Some(NativePrimitive::ListGet)
    );
    let (some, none) = without_parse.option_constructors().expect("Option kept");
    assert_eq!((some.name.as_str(), none.name.as_str()), ("Some", "None"));
    let neither = ReviewedPrimitives::build(builtin_modules(), &|_| true, &known, &|_, _, p| {
        !matches!(p, NativePrimitive::IntParse | NativePrimitive::ListGet)
    });
    assert!(neither.option_constructors().is_none());
    assert_eq!(
        neither.list_primitive("List.Core", "length"),
        Some(NativePrimitive::ListLength)
    );
}

/// The Go of `App`, optimized with the reviewed Go primitives, whose `main`
/// prints `shown`.
#[cfg(feature = "native-project")]
fn go_main(decls: &str, shown: &str) -> String {
    go_of(&format!(
        "{HEAD}{decls}\nmain : Unit ->{{Host.Console; AppError}} Unit\nlet main () = Platform.stdout_line ({shown})\n"
    ))
}

/// The Go of the module `App` written in `text`, optimized with the
/// reviewed Go primitives.
#[cfg(feature = "native-project")]
pub(super) fn go_of(text: &str) -> String {
    use std::collections::BTreeMap;

    use crate::build::go::bindings;
    use crate::codegen::go;
    use crate::ir::Module;

    let (mut ir, interface) = check_text("App", text).expect("lowers");
    let table = lang_items();
    let known = Known::of(&table);
    let go_bindings = bindings();
    let prims = ReviewedPrimitives::build(builtin_modules(), &|_| true, &known, &|m, f, p| {
        go_bindings.allows_primitive(m, f, p)
    });
    super::optimize_with_primitives(
        &mut ir,
        &|name: &str| interface.exports_value(name),
        &builtin_generic,
        &builtin_arity,
        &prims,
    );
    let builtins: Vec<&Module> = builtin_modules().iter().collect();
    let out = go::go(&go::Input {
        own: &[&ir],
        builtins: &builtins,
        entry: &ir.name,
        suites: &[],
        test: false,
        paths: &BTreeMap::new(),
        lines: &[],
        sources: &[],
        foreign: &BTreeMap::new(),
        bindings: go_bindings,
        lang: &known,
    });
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    (out.files.into_iter())
        .find_map(|(name, code)| name.ends_with("m_App.go").then_some(code))
        .expect("m_App.go")
}

/// The Go function of `name` in `go`, up to the next function.
#[cfg(feature = "native-project")]
pub(super) fn go_fn<'a>(go: &'a str, name: &str) -> &'a str {
    let at = go
        .find(&format!("func F_App__{name}("))
        .unwrap_or_else(|| panic!("no {name} in\n{go}"));
    let rest = &go[at..];
    rest[1..].find("\nfunc ").map_or(rest, |end| &rest[..=end])
}

#[cfg(feature = "native-project")]
#[test]
fn go_builds_the_option_of_the_element_type() {
    // recursive, so each stays a Go function with a typed `Option` result
    let go = go_main(
        "\
named : Int -> String -> Option String\n\
let rec named n s = if n > 0 then named (n - 1) s else List.get 1 [\"a\", s]\n\n\
nameless : Int -> String -> Option String\n\
let rec nameless n s = if n > 0 then nameless (n - 1) s else List.get 2 [\"a\", s]\n\n\
type alias Mon = { name : String; hp : Int }\n\n\
second : Int -> String ->{Host.Console} Option Mon\n\
let rec second k n =\n  if k > 0 then second (k - 1) n else List.get 1 [{ name = \"a\"; hp = 1 }, { name = n; hp = note 2 }]\n\n\
shown : Option Mon -> String\nlet shown m =\n  match m with\n  | Some { name; hp } -> name ++ show hp\n  | None -> \"none\"\n",
        "show (named (note 1) \"b\", nameless (note 1) \"c\") ++ shown (second 1 \"m\")",
    );
    for name in ["named", "nameless", "second"] {
        let f = go_fn(&go, name);
        assert!(
            !f.contains("ListGet") && !f.contains("List__Core__get"),
            "{f}"
        );
        assert!(!f.contains("int32]"), "{f}");
    }
    let named = go_fn(&go, "named");
    assert!(
        named.contains("return flickrt.Opt[string]{Val: l_s, Ok: true}"),
        "{named}"
    );
    let nameless = go_fn(&go, "nameless");
    assert!(
        nameless.contains("return flickrt.Opt[string]{}"),
        "{nameless}"
    );
    let second = go_fn(&go, "second");
    assert_eq!(second.matches("F_App__note(").count(), 1, "{second}");
    assert!(
        second.contains("return flickrt.SomeRef(R_hp_name_"),
        "{second}"
    );
}
