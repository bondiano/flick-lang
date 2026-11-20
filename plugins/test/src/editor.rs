//! What an editor shows of a block: the words of the plugin, as its
//! structure reads them, as keywords, and what each means.

use flick_plugin::{Block, Highlight, HighlightKind, Hover, Span};

use crate::read::{Item, Kind, structure};

/// What each word of the plugin means, as an editor shows it over the word.
const HELP: [(&str, &str); 14] = [
    (
        "suite",
        "```flick\nsuite \"name\" { … }\n```\n\nTests under a common name: each case inside is `Suite > case`.",
    ),
    (
        "test",
        "```flick\ntest \"name\" { body }\n```\n\nAn effectful test, which runs in a scope of its own.",
    ),
    (
        "skip",
        "```flick\ntest.skip \"name\" { … }\n```\n\nReported as skipped and never run, with every case inside.",
    ),
    (
        "only",
        "```flick\ntest.only \"name\" { … }\n```\n\nWhile a case of the run is marked `only`, the run keeps only such cases.",
    ),
    (
        "timeout",
        "```flick\ntest \"name\" timeout 500 { … }\n```\n\nThe milliseconds the case has for its `before_each` hooks and its body, as has each case inside a suite without its own; then it is interrupted and reported as `timeout`.",
    ),
    (
        "before_all",
        "```flick\nbefore_all { body }\n```\n\nRuns once in the suite's scope, before the first case of the suite that the run selects; when it fails, each selected case is reported as `setup` and not run.",
    ),
    (
        "before_each",
        "```flick\nbefore_each { body }\n```\n\nRuns in the scope of each selected case of the suite, before its body; when it fails, the case is reported as `setup` and its body does not run.",
    ),
    (
        "after_each",
        "```flick\nafter_each { body }\n```\n\nRuns after each selected case of the suite, as a finalizer of its scope, also when it failed, timed out or was interrupted.",
    ),
    (
        "after_all",
        "```flick\nafter_all { body }\n```\n\nRuns once after the last selected case of the suite, as a finalizer of the suite's scope, also when a `before_all` failed or the run was interrupted.",
    ),
    (
        "fixture",
        "```flick\nfixture name = expression\n```\n\nA `Lazy` the code of the suite reads with `await name`: made the first time a case uses it, once for that case, in a scope that closes after its `after_each` hooks. A fixture may use those declared before it.",
    ),
    (
        "shared",
        "```flick\nfixture shared name = expression\n```\n\nMade once for the run of the suite, the first time one of its cases uses it, and released after its `after_all` hooks. The cases of a suite with a shared fixture run one at a time, unless it is `parallel`.",
    ),
    (
        "concurrent",
        "```flick\nfixture shared concurrent name = expression\n```\n\nA shared fixture that cases may use at once, as a pool or read-only data: the only kind a `parallel` suite shares.",
    ),
    (
        "each",
        "```flick\ntest.each rows \"name {0}\" (pattern) { … }\n```\n\nA case for each row of `rows`, which the pattern matches for the body: named `name [i]`, or with `{0}` to `{n}` replaced by the parts of the row as `show` writes them. The rows are built once, when the module's tests are.",
    ),
    (
        "parallel",
        "```flick\nsuite parallel \"name\" { … }\n```\n\nThe cases of the suite may run at once under `--jobs`, also when it shares fixtures, which must then be `concurrent`.",
    ),
];

/// The plugin's words under `item`: the word of each case inside the block,
/// the block's own being its head, and each modifier.
fn words(item: &Item, out: &mut Vec<Span>) {
    out.extend(item.word);
    out.extend(item.modifier);
    out.extend(item.timeout.map(|(word, _)| word));
    out.extend(item.parallel);
    match &item.kind {
        Kind::Suite(items) => {
            for inner in items {
                words(inner, out);
            }
        }
        Kind::Fixture(f) => out.extend(f.shared.into_iter().chain(f.concurrent)),
        Kind::Test(_) | Kind::Hook(_) | Kind::Each(_) => {}
    }
}

/// The plugin's words of `block`; none for a block it cannot read.
fn spans(block: &Block) -> Vec<Span> {
    let mut out = Vec::new();
    if let Ok(item) = structure(block) {
        words(&item, &mut out);
    }
    out
}

pub fn highlight(block: &Block) -> Vec<Highlight> {
    (spans(block).into_iter())
        .map(|span| Highlight {
            span,
            kind: HighlightKind::Keyword,
        })
        .collect()
}

pub fn hover(block: &Block, offset: u32) -> Option<Hover> {
    let span = (spans(block).into_iter()).find(|s| s.start <= offset && offset < s.end)?;
    let (_, markdown) = HELP.iter().find(|(w, _)| *w == block.text_of(span))?;
    Some(Hover {
        markdown: (*markdown).to_owned(),
        span,
    })
}
