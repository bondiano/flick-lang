//! Differential property tests verifying that every FLN document rendered under
//! `RenderMode::StrictFlick` parses cleanly in `flick-syntax` with zero diagnostics.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use flick_diagnostics::source::FileId;
use flick_syntax::syntax::parse;
use fln::{
    BigInt, Constructor, Float, LayoutOptions, PageWidth, RenderMode, Value, parse_text,
    try_pretty_text, try_render_text,
};
use fln_literals::{RESERVED_KEYWORDS, is_record_label_keyword};
use proptest::prelude::*;

fn arb_field_name() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("alpha".to_owned()),
        Just("beta_2".to_owned()),
        Just("orelse".to_owned()),
        Just("catch".to_owned()),
        Just("echo".to_owned()),
        Just("todo".to_owned()),
        Just("Config".to_owned()),
        Just("_priv".to_owned()),
    ]
}

fn arb_con_name() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("Some".to_owned()),
        Just("None".to_owned()),
        Just("Config".to_owned()),
        Just("Point".to_owned()),
        Just("Entry_1".to_owned()),
    ]
}

fn arb_mod_path() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(
        prop_oneof![
            Just("App".to_owned()),
            Just("Net".to_owned()),
            Just("Core".to_owned()),
        ],
        0..=2,
    )
}

fn arb_string() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just("hello".to_owned()),
        Just("line1\nline2\t\"quoted\"\\slash".to_owned()),
        Just("\0".to_owned()),
        Just("\u{0}5".to_owned()),
        Just("λ 😀 \x07 \x1b".to_owned()),
        "[a-zA-Z0-9 _\\-\n\t\"\\\\]{0,16}",
    ]
}

fn arb_strict_int() -> impl Strategy<Value = i128> {
    prop_oneof![
        Just(0_i128),
        Just(-1_i128),
        Just(1_i128),
        Just(i128::from(i64::MIN)),
        Just(i128::from(i64::MAX)),
        Just(i128::from(u64::MAX)),
        any::<i64>().prop_map(i128::from),
        any::<u64>().prop_map(i128::from),
    ]
}

fn arb_bigint() -> impl Strategy<Value = BigInt> {
    prop_oneof![
        any::<i128>().prop_map(BigInt::from),
        Just(BigInt::parse("-99999999999999999999999999999999999999n").expect("valid")),
        Just(BigInt::parse("99999999999999999999999999999999999999n").expect("valid")),
    ]
}

fn arb_finite_float() -> impl Strategy<Value = Float> {
    prop_oneof![
        Just(Float::new(0.0)),
        Just(Float::new(-1.5)),
        Just(Float::new(42.0)),
        Just(Float::new(1.0e20)),
        Just(Float::new(-1.0e-12)),
        (-1.0e9_f64..1.0e9_f64).prop_map(Float::new),
    ]
}

fn arb_value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Unit),
        any::<bool>().prop_map(Value::Bool),
        arb_strict_int().prop_map(Value::Int),
        arb_bigint().prop_map(Value::BigInt),
        arb_finite_float().prop_map(Value::Float),
        arb_string().prop_map(Value::String),
    ];
    leaf.prop_recursive(3, 24, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..=4).prop_map(Value::List),
            prop::collection::vec(inner.clone(), 1..=3).prop_map(Value::Tuple),
            prop::collection::btree_map(arb_field_name(), inner.clone(), 0..=4)
                .prop_map(Value::Record),
            (
                arb_mod_path(),
                arb_con_name(),
                prop::collection::vec(inner, 0..=3),
            )
                .prop_map(|(path, name, args)| {
                    Value::Constructor(Constructor { path, name, args })
                }),
        ]
    })
}

fn wrap_in_let(expr: &str) -> String {
    let mut out = String::from("let x =\n");
    for line in expr.lines() {
        out.push_str("  ");
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn assert_flick_parses_cleanly(expr: &str) {
    let src = wrap_in_let(expr);
    let parsed = parse(&src, FileId::default());
    assert!(
        parsed.diagnostics.is_empty(),
        "flick-syntax rejected FLN expression:\n{src}\ndiagnostics: {:?}",
        parsed.diagnostics
    );
}

#[test]
fn flick_syntax_accepts_fln_edge_cases() {
    for src in [
        "()",
        "(1,)",
        "(1, 2, 3)",
        "App.Point (-1) (-2.5) (-3n)",
        r#"{ orelse = 1; catch = 2; echo = 3; todo = 4; nul = "\u{0}5" }"#,
        "App.Config {\n  host = \"localhost\"\n  tags = [\n    \"a\",\n    \"b\",\n  ]\n}",
    ] {
        let ast = parse_text("edge.fln", src).expect("fln parses edge case");
        let single = try_render_text(&ast, RenderMode::StrictFlick).expect("strict single");
        let pretty = try_pretty_text(&ast, LayoutOptions::flick(), RenderMode::StrictFlick)
            .expect("strict pretty");
        assert_flick_parses_cleanly(&single);
        assert_flick_parses_cleanly(&pretty);
    }

    let rec = Value::Record(BTreeMap::from([
        ("echo".to_owned(), Value::Int(-42)),
        ("todo".to_owned(), Value::Tuple(vec![Value::Bool(true)])),
    ]));
    let rendered = try_render_text(&rec, RenderMode::StrictFlick).expect("record");
    assert_flick_parses_cleanly(&rendered);
}

#[test]
fn fln_std_reserved_keywords_match_fln_literals() {
    let fln_src = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages/std/src/Fln.fl"),
    )
    .expect("read packages/std/src/Fln.fl");
    let start = fln_src
        .find("let reserved_keywords = [")
        .expect("reserved_keywords list in Fln.fl");
    let rest = &fln_src[start..];
    let end = rest
        .find(']')
        .expect("closing bracket of reserved_keywords");
    let block = &rest[..end];
    let actual: Vec<String> = block
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim().trim_end_matches(',');
            trimmed
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .map(str::to_owned)
        })
        .collect();

    let mut expected: Vec<String> = RESERVED_KEYWORDS
        .iter()
        .copied()
        .filter(|w| !is_record_label_keyword(w))
        .map(str::to_owned)
        .collect();
    expected.push("_".to_owned());
    expected.sort();

    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 45);
    for word in &actual {
        let doc = format!("{{ {word} = 1 }}");
        assert!(
            parse_text("<test>", &doc).is_err(),
            "fln crate should reject reserved record field `{word}`"
        );
    }
    for allowed in [
        "orelse",
        "catch",
        "echo",
        "todo",
        "alias",
        "and",
        "or",
        "handle",
        "effect",
        "pure",
        "ref",
        "while",
        "loop",
        "return",
        "pub",
        "private",
        "unboxed",
        "untagged",
        "enum",
        "refined",
        "of",
        "on",
        "arena",
        "var",
        "proof",
        "law",
        "component",
        "plugin",
    ] {
        let doc = format!("{{ {allowed} = 1 }}");
        assert!(
            parse_text("<test>", &doc).is_ok(),
            "fln crate should accept record field `{allowed}`"
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn strict_fln_renders_parse_in_flick_syntax(value in arb_value()) {
        let single = try_render_text(&value, RenderMode::StrictFlick)
            .expect("generated value is strict-Flick renderable");
        assert_flick_parses_cleanly(&single);

        let pretty = try_pretty_text(&value, LayoutOptions::flick(), RenderMode::StrictFlick)
            .expect("pretty strict-Flick renderable");
        assert_flick_parses_cleanly(&pretty);

        let narrow = try_pretty_text(
            &value,
            LayoutOptions {
                page_width: PageWidth::AvailablePerLine {
                    line_width: 28,
                    ribbon_fraction: 1.0,
                },
            },
            RenderMode::StrictFlick,
        )
        .expect("narrow strict-Flick renderable");
        assert_flick_parses_cleanly(&narrow);
    }
}
