#![allow(clippy::float_cmp, reason = "folded values must be exact")]

use super::*;
use crate::source::SourceMap;
use crate::syntax::parse;

fn value(expr: &str) -> Option<Const> {
    let src = format!("let x = {expr}\n");
    let mut map = SourceMap::default();
    let file = map.add("t.fl", src.as_str());
    let parsed = parse(&src, file);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let decl = parsed.tree.nodes().next().expect("let");
    let body = decl.nodes().find(|n| n.kind == K::Block).expect("body");
    eval(body, &src)
}

fn number(expr: &str) -> f64 {
    match value(expr) {
        Some(Const::Number(n)) => n,
        other => panic!("{expr}: {other:?}"),
    }
}

#[test]
fn folds_number_arithmetic_with_js_semantics() {
    assert_eq!(number("60 * 60 * 24"), 86_400.0);
    assert_eq!(number("0.1 + 0.2"), 0.300_000_000_000_000_04);
    assert!(number("0 / 0").is_nan());
    assert_eq!(number("1 / 0"), f64::INFINITY);
    assert!(number("-0").is_sign_negative());
    assert!(number("-5 % 3") == -2.0 && number("5.5 % 2") == 1.5);
    assert!(number("-0 % 5").is_sign_negative());
    assert_eq!(number("2 ** 10"), 1024.0);
    assert!(number("(-0) ** 3").is_sign_negative());
    assert_eq!(number("(0 / 0) ** 0"), 1.0);
    assert_eq!(number("(-1) ** 3"), -1.0);
    assert_eq!(
        value("2 ** 0.5"),
        None,
        "inexact powers are left to the runtime"
    );
    assert_eq!(value("2 ** 60"), None);
    assert_eq!(number("if 1 < 2 then 0x10 else 1_0"), 16.0);
}

#[test]
fn folds_comparisons_bools_strings_and_bigints() {
    assert_eq!(
        value("0 / 0 == 0 / 0"),
        Some(Const::Bool(true)),
        "SameValueZero"
    );
    assert_eq!(value("0 == -0"), Some(Const::Bool(true)));
    assert_eq!(value("0 / 0 < 1"), Some(Const::Bool(false)));
    assert_eq!(
        value("not (true && false) || false"),
        Some(Const::Bool(true))
    );
    assert_eq!(
        value(r#""a\n" ++ "b" ++ r"\c""#),
        Some(Const::String("a\nb\\c".into()))
    );
    // UTF-16 order: U+FF61 is above the surrogates of U+1F600 in UTF-16 only
    assert_eq!(
        value(r#""\u{FF61}" < "\u{1F600}""#),
        Some(Const::Bool(false))
    );
    assert_eq!(value("7n / -2n"), Some(Const::BigInt(-3)));
    assert_eq!(value("-7n % 2n"), Some(Const::BigInt(-1)));
    assert_eq!(value("2n ** 100n"), Some(Const::BigInt(1 << 100)));
    assert_eq!(
        value("1n / 0n"),
        Some(Const::BigInt(0)),
        "zero by the operator"
    );
    assert_eq!(value("2n ** -1n"), None, "throws at run time");
}

#[test]
fn leaves_non_literals_and_mixed_types_alone() {
    for expr in [
        "x + 1",
        "1 + 1n",
        "\"a\" ++ 1",
        "f 1",
        "true && x",
        "1 |> f",
    ] {
        assert_eq!(value(expr), None, "{expr}");
    }
}
