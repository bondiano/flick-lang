//! Release optimizer integration tests for reviewed native primitive folding
//! (#805): live built-in contract validation, foreign-metadata mutation
//! rejection, and emitted JS/Go release shapes with boundary preservation.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::tests::{builtin_arity, builtin_generic};
use crate::codegen::test_options::resolved_by;
use crate::codegen::{EffectBridge, Known, Labels, Options, js};
use crate::ir::lang_items;
use crate::ir::tests::check_text;
use crate::types;
use flick_backend_support::opt as portable;

fn emit_js_with_reviewed_primitives(
    text: &str,
    config: &portable::eval::EvalConfig,
) -> (String, portable::eval::EvalCounters) {
    use crate::build::builtin_modules;
    let (mut ir, interface) = check_text("App", text).expect("lowers");
    let table = lang_items();
    let known = Known::of(&table);
    let builtin = |module: &str| {
        types::builtin_sources()
            .iter()
            .any(|(name, _)| *name == module)
    };
    let primitives = portable::eval::ReviewedPrimitives::for_js(
        builtin_modules(),
        &|_| true,
        &known,
        false,
        EffectBridge::Absent,
        &|_, _| None,
    );
    let counters = portable::optimize_with_eval(
        &mut ir,
        &|name: &str| interface.exports_value(name),
        &builtin_generic,
        &builtin_arity,
        &known,
        &builtin,
        config,
        &primitives,
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

fn expected_live_primitives()
-> &'static [(&'static str, &'static str, portable::eval::NativePrimitive)] {
    use portable::eval::NativePrimitive;
    &[
        ("Int", "parse", NativePrimitive::IntParse),
        ("String", "length", NativePrimitive::StringLength),
        ("String", "slice", NativePrimitive::StringSlice),
        ("String", "starts_with", NativePrimitive::StringStartsWith),
        ("String", "contains", NativePrimitive::StringContains),
        ("String", "ends_with", NativePrimitive::StringEndsWith),
        (
            "Instances",
            "text_starts_with",
            NativePrimitive::StringStartsWith,
        ),
        (
            "Instances",
            "text_contains",
            NativePrimitive::StringContains,
        ),
    ]
}

#[test]
fn live_builtin_native_primitives_match_js_and_go_contracts() {
    use crate::build::builtin_modules;
    use portable::eval::{PrimitiveEval, ReviewedPrimitives};

    let table = lang_items();
    let known = Known::of(&table);
    let builtins = builtin_modules();
    let expected = expected_live_primitives();

    for rt in [false, true] {
        for bridge in [
            EffectBridge::Absent,
            EffectBridge::Available,
            EffectBridge::AvailableWithUi,
            EffectBridge::AvailableWithWeb,
            EffectBridge::AvailableWithUiAndWeb,
        ] {
            let prims =
                ReviewedPrimitives::for_js(builtins, &|_| true, &known, rt, bridge, &|_, _| None);
            for &(module, name, want) in expected {
                assert_eq!(
                    prims.lookup(module, name),
                    Some(want),
                    "rt={rt} bridge={bridge:?}: {module}.{name}"
                );
                assert_eq!(
                    prims.arity(module, name, &known),
                    Some(want.arity()),
                    "rt={rt} bridge={bridge:?}: {module}.{name}"
                );
            }
            for &(module, name) in &[
                ("Int", "unsafe_div"),
                ("Int", "to_string"),
                ("Float", "abs"),
                ("BigInt", "parse"),
                ("String", "to_upper"),
                ("String", "locale_compare"),
                ("String", "trim"),
            ] {
                assert_eq!(
                    prims.lookup(module, name),
                    None,
                    "unreviewed {module}.{name} must decline"
                );
            }
        }
    }

    let alt = ReviewedPrimitives::for_js(
        builtins,
        &|_| true,
        &known,
        true,
        EffectBridge::Absent,
        &|_, _| Some("./custom-runtime.mjs"),
    );
    assert!(alt.is_empty());

    #[cfg(feature = "native-project")]
    {
        use crate::build::go::bindings;
        use crate::codegen::go::Bindings;

        let go_bindings = bindings();
        let go_prims = ReviewedPrimitives::build(builtins, &|_| true, &known, &|m, f, p| {
            go_bindings.allows_primitive(m, f, p)
        });
        for &(module, name, want) in expected {
            assert_eq!(
                go_prims.lookup(module, name),
                Some(want),
                "Go bindings must accept {module}.{name}"
            );
        }

        let custom_go = Bindings::parse([
            "[flick:rt]\nintParse CustomParse\n[flick:String]\nlength Length(int32) int32\n",
        ]);
        let custom_prims = ReviewedPrimitives::build(builtins, &|_| true, &known, &|m, f, p| {
            custom_go.allows_primitive(m, f, p)
        });
        assert!(
            custom_prims.is_empty(),
            "alternate Go bindings must decline"
        );
    }
}

#[test]
fn live_builtin_native_primitives_reject_foreign_mutations() {
    use crate::build::builtin_modules;
    use crate::ir::{Expr, ExprKind, ItemKind, Lit};
    use crate::source::{FileId, Span};
    use flick_ir::{Foreign, ForeignMode, Shape};
    use portable::eval::ReviewedPrimitives;

    let table = lang_items();
    let known = Known::of(&table);
    let builtins = builtin_modules();
    let expected = expected_live_primitives();

    let mutations: Vec<fn(&mut Foreign)> = vec![
        |f| f.source_name = Some("shim".into()),
        |f| f.mode = ForeignMode::Sync,
        |f| f.mode = ForeignMode::Async,
        |f| f.mode = ForeignMode::Effect,
        |f| f.dicts = 1,
        |f| f.arity = 0,
        |f| f.arity += 1,
        |f| f.stages[0] = true,
        |f| {
            let _ = f.stages.pop();
        },
        |f| f.stages.push(false),
        |f| f.binding.from = None,
        |f| f.binding.from = Some("./alt.mjs".into()),
        |f| f.binding.name.push('!'),
        |f| {
            f.catch = Some(Expr::new(
                ExprKind::Lit(Lit::Unit),
                Span::new(FileId::new(0), 0, 0),
            ));
        },
        |f| {
            f.adapter.words.push(flick_ir::AdapterWord {
                name: "this".into(),
                args: vec![],
                span: Span::new(FileId::new(0), 0, 0),
            });
        },
        |f| {
            f.adapter.words.push(flick_ir::AdapterWord {
                name: "cancel".into(),
                args: vec![flick_ir::AdapterArg::Ident("signal".into())],
                span: Span::new(FileId::new(0), 0, 0),
            });
        },
        |f| f.adapter.optional.push(0),
        |f| f.adapter.owned.push(0),
        |f| {
            f.adapter.words.push(flick_ir::AdapterWord {
                name: "variadic".into(),
                args: vec![],
                span: Span::new(FileId::new(0), 0, 0),
            });
        },
        |f| f.adapter.result.nullable = true,
        |f| f.adapter.result.unit = true,
        |f| f.adapter.callbacks[0] = Some(1),
        |f| f.adapter.copies.args[0] = true,
        |f| f.adapter.copies.result = !f.adapter.copies.result,
        |f| f.adapter.copies.callbacks[0] = Some((vec![], false)),
        |f| f.adapter.shapes[0] = Shape::Layer,
        |f| f.adapter.result_shape = Shape::Layer,
    ];
    for &(module_name, decl_name, _) in expected {
        for (idx, mutate) in mutations.iter().enumerate() {
            let mut altered: Vec<_> = builtins.to_vec();
            let target_mod = altered
                .iter_mut()
                .find(|m| m.name == module_name)
                .expect("builtin module");
            let item = target_mod
                .items
                .iter_mut()
                .find(|i| matches!(&i.kind, ItemKind::Foreign(f) if f.name == decl_name))
                .expect("foreign decl");
            let ItemKind::Foreign(f) = &mut item.kind else {
                unreachable!()
            };
            mutate(f);
            let prims = ReviewedPrimitives::for_js(
                &altered,
                &|_| true,
                &known,
                true,
                EffectBridge::Absent,
                &|_, _| None,
            );
            assert_eq!(
                prims.lookup(module_name, decl_name),
                None,
                "{module_name}.{decl_name} mutation {idx} must decline"
            );
        }
    }
}

const NATIVE_PRIMITIVES_JS_SRC: &str = "\
module App exposing (
  len_earth, len_lone, lone_slice, valid_slice, rejoined,
  parsed_ok, parsed_unicode, parsed_neg_zero, parsed_min, parsed_max,
  parsed_over, parsed_under, parsed_nel, parsed_bad,
  age_ok, age_neg, age_bad, age_dyn,
  preds, homonym, own_foreign, unreviewed, dyn_len, dyn_slice
)

age : String -> Result String Int
let age text =
  match Int.parse text with
  | Some n when n >= 0 -> Ok n
  | Some _ -> Err \"an age is not negative\"
  | None -> Err \"not a number: \\{text}\"

length : String -> Int
let length _ = 999

parse : String -> Option Int
foreign pure parse from \"flick:rt\" \"intParse\"

len_earth : Int
let len_earth = String.length \"a\\u{1F30D}b\"

len_lone : Int
let len_lone = String.length (String.slice 1 2 \"a\\u{1F30D}b\")

lone_slice : String
let lone_slice = String.slice 1 2 \"a\\u{1F30D}b\"

valid_slice : String
let valid_slice = String.slice 1 3 \"a\\u{1F30D}b\"

rejoined : String
let rejoined = String.slice 0 2 \"a\\u{1F30D}b\" ++ String.slice 2 4 \"a\\u{1F30D}b\"

parsed_ok : Option Int
let parsed_ok = Int.parse \"36\"

parsed_unicode : Option Int
let parsed_unicode = Int.parse \"\\u{FEFF}36\\u{00A0}\"

parsed_neg_zero : Option Int
let parsed_neg_zero = Int.parse \"-0\"

parsed_min : Option Int
let parsed_min = Int.parse \"-2147483648\"

parsed_max : Option Int
let parsed_max = Int.parse \"2147483647\"

parsed_over : Option Int
let parsed_over = Int.parse \"2147483648\"

parsed_under : Option Int
let parsed_under = Int.parse \"-2147483649\"

parsed_nel : Option Int
let parsed_nel = Int.parse \"\\u{0085}36\"

parsed_bad : Option Int
let parsed_bad = Int.parse \"ada\"

age_ok : Result String Int
let age_ok = age \"36\"

age_neg : Result String Int
let age_neg = age \"-1\"

age_bad : Result String Int
let age_bad = age \"ada\"

age_dyn : String -> Result String Int
let age_dyn s = age s

preds : (Bool, Bool, Bool, Bool)
let preds = (
  String.starts_with \"[\" \"[1, 2]\",
  String.contains \" \" \"a b\",
  String.ends_with \"b\" \"a\\u{1F30D}b\",
  String.contains \"z\" \"a\\u{1F30D}b\"
)

homonym : Int
let homonym = length \"a\\u{1F30D}b\"

own_foreign : Option Int
let own_foreign = parse \"36\"

unreviewed : (String, Option BigInt)
let unreviewed = (String.to_upper \"abc\", BigInt.parse \"42\")

dyn_len : String -> Int
let dyn_len s = String.length s

dyn_slice : String -> String
let dyn_slice s = String.slice 1 2 s
";

#[test]
fn reviewed_native_primitives_fold_in_release_js_and_preserve_boundaries() {
    let (js, counters) = emit_js_with_reviewed_primitives(
        NATIVE_PRIMITIVES_JS_SRC,
        &portable::eval::EvalConfig::default(),
    );
    assert!(counters.success >= 18, "{counters:?}");
    assert!(counters.unmaterializable >= 1, "{counters:?}");
    assert_js_native_primitive_shapes(&js);

    // Zero budget preserves every primitive call.
    let (zero_js, zero_counters) = emit_js_with_reviewed_primitives(
        NATIVE_PRIMITIVES_JS_SRC,
        &portable::eval::EvalConfig {
            max_module_steps: 0,
            ..portable::eval::EvalConfig::default()
        },
    );
    assert_eq!(zero_counters.success, 0);
    assert!(zero_js.contains("$m_String.length(\"a🌍b\")"), "{zero_js}");
    assert!(zero_js.contains("$m_Int.parse(\"36\")"), "{zero_js}");
}

fn assert_js_native_primitive_shapes(js: &str) {
    for expected in [
        "export const len_earth = 4;",
        "export const len_lone = 1;",
        "export const valid_slice = \"🌍\";",
        "export const rejoined = \"a🌍b\";",
        "export const parsed_ok = /*#__PURE__*/ $rt.some(36);",
        "export const parsed_unicode = /*#__PURE__*/ $rt.some(36);",
        "export const parsed_neg_zero = /*#__PURE__*/ $rt.some(0);",
        "export const parsed_min = /*#__PURE__*/ $rt.some(-2147483648);",
        "export const parsed_max = /*#__PURE__*/ $rt.some(2147483647);",
        "export const parsed_over = $rt.none;",
        "export const parsed_under = $rt.none;",
        "export const parsed_nel = $rt.none;",
        "export const parsed_bad = $rt.none;",
        "export const age_ok = /*#__PURE__*/ $rt.ok(36);",
        "export const age_neg = /*#__PURE__*/ $rt.err(\"an age is not negative\");",
        "export const age_bad = /*#__PURE__*/ $rt.err(\"not a number: ada\");",
        "export const preds = [true, true, true, false];",
        "export const homonym = 999;",
    ] {
        assert!(js.contains(expected), "missing `{expected}` in:\n{js}");
    }
    let lone_line = js
        .lines()
        .find(|l| l.starts_with("export const lone_slice ="))
        .expect("lone_slice line");
    assert!(
        lone_line.contains("slice"),
        "must stay residual: {lone_line}"
    );
    assert!(
        !lone_line.contains('\u{FFFD}'),
        "must never substitute U+FFFD: {lone_line}"
    );

    let own_line = js
        .lines()
        .find(|l| l.starts_with("export const own_foreign ="))
        .expect("own_foreign line");
    assert!(
        own_line.contains("parse(\"36\")") || own_line.contains("$f_parse(\"36\")"),
        "{own_line}"
    );

    let unrev_line = js
        .lines()
        .find(|l| l.starts_with("export const unreviewed ="))
        .expect("unreviewed line");
    assert!(
        unrev_line.contains("to_upper") && unrev_line.contains("BigInt"),
        "{unrev_line}"
    );

    assert!(
        js.contains("export const dyn_len = (s) => $m_String.length(s);"),
        "{js}"
    );
    assert!(js.contains("export const dyn_slice = (s) =>"), "{js}");
    assert!(js.contains("export const age_dyn = (s) => age(s);"), "{js}");
}

#[cfg(feature = "native-project")]
#[test]
fn reviewed_native_primitives_fold_in_release_go_and_preserve_boundaries() {
    use crate::build::builtin_modules;
    use crate::build::go::bindings;
    use crate::codegen::go;
    use crate::ir::Module;

    let go_src = "\
module App exposing (main)

main : Unit ->{Host.Console; AppError} Unit
let main () =
  Platform.stdout_line (show (String.length \"a\\u{1F30D}b\"))
  Platform.stdout_line (show (String.length (String.slice 1 2 \"a\\u{1F30D}b\")))
  Platform.stdout_line (show (String.slice 1 2 \"a\\u{1F30D}b\"))
  Platform.stdout_line (show (Int.parse \"\\u{FEFF}36\\u{00A0}\"))
";
    let (mut go_ir, go_iface) = check_text("App", go_src).expect("lowers");
    let table = lang_items();
    let known = Known::of(&table);
    let go_bindings = bindings();
    let builtins_slice = builtin_modules();
    let go_prims =
        portable::eval::ReviewedPrimitives::build(builtins_slice, &|_| true, &known, &|m, f, p| {
            go_bindings.allows_primitive(m, f, p)
        });
    super::optimize_with_primitives(
        &mut go_ir,
        &|name: &str| go_iface.exports_value(name),
        &builtin_generic,
        &builtin_arity,
        &go_prims,
    );
    let builtins: Vec<&Module> = builtins_slice.iter().collect();
    let out = go::go(&go::Input {
        own: &[&go_ir],
        builtins: &builtins,
        entry: &go_ir.name,
        suites: &[],
        test: false,
        paths: &BTreeMap::new(),
        lines: &[],
        foreign: &BTreeMap::new(),
        bindings: go_bindings,
        lang: &known,
    });
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let code = out
        .files
        .into_iter()
        .find_map(|(name, code)| name.ends_with("m_App.go").then_some(code))
        .expect("m_App.go");
    assert!(
        code.contains("F_Platform__stdout_line(fb, \"4\")"),
        "{code}"
    );
    assert!(
        code.contains("F_Platform__stdout_line(fb, \"1\")"),
        "{code}"
    );
    assert!(
        code.contains("F_String__slice(fb, int32(1), int32(2), \"a\\U0001f30db\")"),
        "{code}"
    );
    assert!(
        !code.contains('\u{FFFD}'),
        "Go output must never contain U+FFFD:\n{code}"
    );
    assert!(
        code.contains("flickrt.NewData(\"Some\", int32(36))"),
        "{code}"
    );
    assert!(
        !code.contains("F_Int__parse"),
        "closed Int.parse should fold in Go:\n{code}"
    );
}
