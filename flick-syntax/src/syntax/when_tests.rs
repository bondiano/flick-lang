use super::*;
use crate::source::FileId;

#[test]
fn parses_and_evaluates_precedence_negation_and_parens() {
    // `!` binds tighter than `&&`, which binds tighter than `||`.
    let expr = parse("go || js && !browser").expect("valid");
    assert!(expr.eval(&|t| t == "go"));
    assert!(expr.eval(&|t| matches!(t, "js" | "node")));
    assert!(!expr.eval(&|t| matches!(t, "js" | "browser")));
    assert!(!expr.eval(&|_| false));

    let grouped = parse("!(go || js) && (jit || ffi)").expect("valid");
    assert!(grouped.eval(&|t| matches!(t, "lua" | "jit")));
    assert!(!grouped.eval(&|t| matches!(t, "js" | "jit")));
    assert!(!grouped.eval(&|t| t == "lua"));

    let double_neg = parse("!!effect-adapter").expect("valid");
    assert!(double_neg.eval(&|t| t == "effect-adapter"));
    assert!(!double_neg.eval(&|_| false));
}

#[test]
fn collects_tags_with_exact_byte_ranges_and_maps_subspan() {
    let src = r#"@when "js && !browzer || effect-adapter""#;
    let lit_start = u32::try_from(src.find('"').expect("quote")).expect("small");
    let lit_end = u32::try_from(src.len()).expect("small");
    let literal = Span::new(FileId::new(1), lit_start, lit_end);
    let cond = "js && !browzer || effect-adapter";
    let expr = parse(cond).expect("valid");
    let tags = expr.tags();
    assert_eq!(tags.len(), 3);
    for (name, range) in tags {
        assert_eq!(&cond[range.0..range.1], name);
        let mapped = subspan(src, literal, range);
        assert_eq!(&src[mapped.range()], name);
    }
}

#[test]
fn rejects_invalid_conditions_and_tag_names() {
    for bad in [
        "",
        "   ",
        "js & node",
        "js | go",
        "js &&",
        "&& js",
        "(js",
        "js)",
        "js node",
        "JS",
        "123",
        "-bad",
        "js/node",
        "()",
    ] {
        assert!(parse(bad).is_err(), "expected error for `{bad}`");
    }
    assert!(is_valid_tag("js"));
    assert!(is_valid_tag("lua51"));
    assert!(is_valid_tag("effect-adapter"));
    assert!(is_valid_tag("my_host"));
    assert!(!is_valid_tag(""));
    assert!(!is_valid_tag("Js"));
    assert!(!is_valid_tag("1host"));
    assert!(!is_valid_tag("a.b"));
}

#[test]
fn rejects_excessive_nesting_and_handles_long_chains_without_stack_overflow() {
    let deep_parens = format!("{}js{}", "(".repeat(2_000), ")".repeat(2_000));
    let err = parse(&deep_parens).expect_err("deep parens must be rejected");
    assert!(err.message.contains("maximum depth"), "{err}");

    let deep_not = format!("{}js", "!".repeat(2_000));
    let err = parse(&deep_not).expect_err("deep negations must be rejected");
    assert!(err.message.contains("maximum depth"), "{err}");

    let at_limit = format!("{}js{}", "(".repeat(MAX_DEPTH), ")".repeat(MAX_DEPTH));
    let expr = parse(&at_limit).expect("MAX_DEPTH parens allowed");
    assert!(expr.eval(&|t| t == "js"));

    let over_limit = format!(
        "{}js{}",
        "(".repeat(MAX_DEPTH + 1),
        ")".repeat(MAX_DEPTH + 1)
    );
    assert!(parse(&over_limit).is_err());

    let long_and = vec!["js"; 5_000].join(" && ");
    let expr = parse(&long_and).expect("long flat chain parses");
    assert!(expr.eval(&|t| t == "js"));
    assert!(!expr.eval(&|_| false));
    assert_eq!(expr.tags().len(), 5_000);
    drop(expr);
}

#[test]
fn subspan_falls_back_for_triple_quoted_toml_strings() {
    for (src, cond) in [
        ("when = \"\"\"js && browzer\"\"\"", "js && browzer"),
        ("when = '''js && browzer'''", "js && browzer"),
    ] {
        let start = u32::try_from(src.find('=').expect("eq") + 2).expect("small");
        let end = u32::try_from(src.len()).expect("small");
        let literal = Span::new(FileId::new(1), start, end);
        let expr = parse(cond).expect("valid");
        let (_, range) = expr.tags()[1];
        assert_eq!(subspan(src, literal, range), literal);
    }
}
