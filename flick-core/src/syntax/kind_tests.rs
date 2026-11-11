//! The reserved words of `kind.rs` against the editors' grammars and the
//! keyword reference: a word added or dropped in one place fails here.

use std::collections::BTreeSet;
use std::fs;
use std::mem;
use std::path::Path;

use super::kind::{FOREIGN_MODES, KEYWORDS, SyntaxKind, describe};

/// Words a grammar or the reference may show as keywords besides the
/// reserved ones: the parser's contextual words and plugin heads.
const CONTEXTUAL: [&str; 11] = [
    "loop",
    "while",
    "and",
    "private",
    "plugin",
    "component",
    "server",
    "fn",
    "global",
    "sql",
    "ui",
];

fn read(path: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(path))
        .unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The lowercase words of two letters or more, `\x` escapes left out.
fn words(text: &str, out: &mut BTreeSet<String>) {
    let mut word = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c.is_ascii_lowercase() {
            word.push(c);
            continue;
        }
        if word.len() > 1 {
            out.insert(mem::take(&mut word));
        }
        word.clear();
        if c == '\\' {
            chars.next();
        }
    }
    if word.len() > 1 {
        out.insert(word);
    }
}

/// Every `'word'` of `grammar.js` but the grammar's own `name`.
fn tree_sitter() -> BTreeSet<String> {
    let text = read("editors/tree-sitter-flick/grammar.js");
    let mut out = BTreeSet::new();
    for (i, _) in text.match_indices('\'') {
        let rest = &text[i + 1..];
        let len = rest.bytes().take_while(u8::is_ascii_lowercase).count();
        if len > 0 && rest[len..].starts_with('\'') && !text[..i].ends_with("name: ") {
            out.insert(rest[..len].to_owned());
        }
    }
    out
}

/// The words of every `match` the `TextMate` grammar names a keyword or a
/// boolean.
fn text_mate() -> BTreeSet<String> {
    fn walk(value: &serde_json::Value, out: &mut BTreeSet<String>) {
        match value {
            serde_json::Value::Object(rule) => {
                let name = rule.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let shown = ["keyword.control", "keyword.other", "keyword.operator.word"]
                    .iter()
                    .any(|p| name.starts_with(p))
                    || name.starts_with("constant.language.boolean");
                if let (true, Some(pattern)) = (shown, rule.get("match").and_then(|m| m.as_str())) {
                    words(pattern, out);
                }
                rule.values().for_each(|v| walk(v, out));
            }
            serde_json::Value::Array(items) => items.iter().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let text = read("editors/vscode/syntaxes/flick.tmLanguage.json");
    let mut out = BTreeSet::new();
    walk(
        &serde_json::from_str(&text).expect("the grammar is JSON"),
        &mut out,
    );
    out
}

/// The `` `word` ``s of the first column of the reference's tables.
fn reference(path: &str) -> BTreeSet<String> {
    let text = read(path);
    text.lines()
        .filter(|l| l.starts_with("| `"))
        .filter_map(|l| l.split('|').nth(1))
        .flat_map(|cell| cell.split('`').skip(1).step_by(2))
        .map(str::to_owned)
        .collect()
}

/// What `shown` gets wrong about the reserved words, if anything.
fn disagreement(source: &str, shown: &BTreeSet<String>) -> Option<String> {
    let missing: Vec<_> = KEYWORDS.iter().filter(|w| !shown.contains(**w)).collect();
    let unknown: Vec<_> = shown
        .iter()
        .map(String::as_str)
        .filter(|w| !KEYWORDS.contains(w) && !CONTEXTUAL.contains(w) && !FOREIGN_MODES.contains(w))
        .collect();
    (!missing.is_empty() || !unknown.is_empty()).then(|| {
        format!("{source}: reserved but not shown {missing:?}; shown but not a keyword {unknown:?}")
    })
}

#[test]
fn the_editors_and_the_reference_show_the_reserved_words() {
    let reference_en = "docs/src/content/docs/reference/keywords.md";
    let reference_ru = "docs/src/content/docs/ru/reference/keywords.md";
    let wrong: Vec<_> = [
        disagreement("tree-sitter", &tree_sitter()),
        disagreement("TextMate", &text_mate()),
        disagreement(reference_en, &reference(reference_en)),
        disagreement(reference_ru, &reference(reference_ru)),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn an_expected_keyword_is_shown_by_its_word() {
    assert_eq!(describe(SyntaxKind::DoKw), "`do`");
    assert_eq!(describe(SyntaxKind::AsKw), "`as`");
    assert_eq!(describe(SyntaxKind::LoopKw), "a token");
}
