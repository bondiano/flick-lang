use super::*;
use crate::source::SourceMap;
use SyntaxKind as K;

fn kinds(text: &str) -> (Vec<(SyntaxKind, &str)>, Vec<String>) {
    let mut map = SourceMap::default();
    let file = map.add("t.fl", text);
    let (tokens, errors) = lex(text, file);
    let covered: String = tokens
        .iter()
        .map(|t| &text[t.start as usize..t.end as usize])
        .collect();
    assert_eq!(covered, text, "tokens cover the source");
    let tokens = tokens
        .into_iter()
        .filter(|t| t.kind != K::Whitespace)
        .map(|t| (t.kind, &text[t.start as usize..t.end as usize]))
        .collect();
    let errors = errors
        .iter()
        .map(|d| {
            let s = d.span.expect("span");
            format!("{} {}", d.code, &text[s.range()])
        })
        .collect();
    (tokens, errors)
}

#[test]
fn numbers_follow_js_grammar_without_dot_forms() {
    for ok in [
        "0", "42", "1_000", "1.5", "1e-3", "2E+10", "0x1F", "0o17", "0b1010", "0xFF_FF", "10n",
        "0n", "0x1Fn", "0.5",
    ] {
        let (tokens, errors) = kinds(ok);
        assert!(errors.is_empty(), "{ok}: {errors:?}");
        assert_eq!(tokens.len(), 1, "{ok}");
    }
    assert_eq!(kinds("10n").0[0].0, K::BigInt);
    for (bad, at) in [
        ("010", "010"),
        ("1__0", "_"),
        ("1_", "_"),
        ("1._5", "1."),
        ("1.5n", "1.5n"),
        ("1e", "1e"),
        ("0x", "0x"),
        ("5.", "5."),
        (".5", ".5"),
        ("12abc", "abc"),
        ("0_1", "0_1"),
    ] {
        let (tokens, errors) = kinds(bad);
        assert_eq!(errors, [format!("FLICK-S006 {at}")], "{bad}");
        assert_eq!(tokens.len(), 1, "{bad} stays one token");
    }
    assert_eq!(kinds("xs.len").0.len(), 3);
    assert_eq!(kinds("(..)").0[1].0, K::DotDot);
    assert_eq!(kinds("[...r]").0[1].0, K::DotDotDot);
    // a range: the number ends before `..`, and `1.5` stays one number
    let range = |src| kinds(src).0.iter().map(|t| t.0).collect::<Vec<_>>();
    assert_eq!(range("1..5"), [K::Number, K::DotDot, K::Number]);
    assert_eq!(range("0..<n"), [K::Number, K::DotDotLt, K::Ident]);
    assert_eq!(range("1.5..2.5"), [K::Number, K::DotDot, K::Number]);
    assert!(kinds("1..").1.is_empty());
}

#[test]
fn strings_escapes_raw_and_multiline() {
    let (tokens, errors) = kinds(r##""a\n\t\r\\\"\0\u{1F600}" r"x\q" r#"a"b"# "##);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(tokens.iter().filter(|t| t.0 == K::String).count(), 3);
    let (_, errors) = kinds("\"\"\"\n  a \\u{41} \"quoted\"\n  \"\"\"");
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        kinds(r#""\q" "\u{D800}" "\u{}" "\01""#).1,
        [
            "FLICK-S005 \\q",
            "FLICK-S005 \\u{D800}",
            "FLICK-S005 \\u{}",
            "FLICK-S005 \\0"
        ]
    );
    assert_eq!(kinds("\"abc\nx").1, ["FLICK-S004 \""]);
    assert_eq!(kinds("r#\"abc\"").1, ["FLICK-S004 r#\""]);
    assert_eq!(kinds("\"\"\"x\"\"\"").1, ["FLICK-S005 \"\"\""]);
}

#[test]
fn comments_keywords_and_operators() {
    let (tokens, errors) = kinds("/* a /* b */ c */ /// doc\n// c\nlet x |> y ->{} ** -1");
    assert!(errors.is_empty());
    let shown: Vec<_> = tokens.iter().map(|t| t.0).collect();
    assert_eq!(
        shown,
        [
            K::BlockComment,
            K::DocComment,
            K::Newline,
            K::LineComment,
            K::Newline,
            K::LetKw,
            K::Ident,
            K::PipeGt,
            K::Ident,
            K::Arrow,
            K::LBrace,
            K::RBrace,
            K::StarStar,
            K::Minus,
            K::Number
        ]
    );
    assert_eq!(kinds("/* open").1, ["FLICK-S004 /*"]);
}

#[test]
fn stray_characters_and_tabs() {
    assert_eq!(kinds("a @€ b").1, ["FLICK-S003 @€"]);
    assert_eq!(kinds("\tx").1, ["FLICK-S009 \t"]);
    assert_eq!(kinds("Foo _ _x").0[1].0, K::Underscore);
}

#[test]
fn keywords_name_members_after_a_qualifier_and_in_declarations() {
    fn ident(text: &str, i: usize) -> (K, &str) {
        kinds(text).0[i]
    }
    assert_eq!(ident("HashSet.with w", 2), (K::Ident, "with"));
    assert_eq!(ident("with : Float", 0), (K::Ident, "with"));
    assert_eq!(ident("foreign pure with from", 2), (K::Ident, "with"));
    assert_eq!(ident("let error message =", 1), (K::Ident, "error"));
    assert_eq!(ident("let rec x", 1), (K::RecKw, "rec"));
    assert_eq!(ident("let assert x", 1), (K::AssertKw, "assert"));
    assert_eq!(ident("Console.assert ok", 2), (K::Ident, "assert"));
    assert_eq!(ident("foreign effect assert from", 2), (K::Ident, "assert"));
    assert_eq!(ident("try x with", 2), (K::WithKw, "with"));
    assert_eq!(ident("x . with", 2), (K::WithKw, "with"));
}

#[test]
fn keyword_context_requires_whole_words_and_adjacent_qualifiers() {
    for name in ["with_suffix", "let2", "_echo", "foreign_pure", "assertion"] {
        assert_eq!(kinds(name), (vec![(K::Ident, name)], vec![]));
    }
    for (src, word, expected) in [
        ("with :: Float", "with", K::WithKw),
        (" with : Float", "with", K::WithKw),
        ("HashSet.with", "with", K::Ident),
        ("HashSet. with", "with", K::WithKw),
        ("let rec with = 1", "with", K::Ident),
        ("foreign async with from", "with", K::Ident),
        ("foreign asynchronous with from", "with", K::WithKw),
        ("let assert x = value", "assert", K::AssertKw),
        ("{ echo = 1 }", "echo", K::Ident),
        ("{ todo : Float }", "todo", K::Ident),
        ("echo == value", "echo", K::EchoKw),
        ("todo => value", "todo", K::TodoKw),
        ("todo :: value", "todo", K::TodoKw),
    ] {
        let (tokens, errors) = kinds(src);
        assert!(errors.is_empty(), "{src}: {errors:?}");
        let token = tokens
            .iter()
            .find(|(_, text)| *text == word)
            .expect("word token");
        assert_eq!(*token, (expected, word), "{src}");
    }
}
