//! What an editor shows of a block: the words of the plugin, as its
//! structure reads them, as keywords, and what each means.

use flick_plugin::{Block, Highlight, HighlightKind, Hover, Span};

use crate::read::{Item, Kind, structure};

/// What each word of the plugin means, as an editor shows it over the word.
const HELP: [(&str, &str); 9] = [
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
];

/// The plugin's words under `item`: the word of each case inside the block,
/// the block's own being its head, and each modifier.
fn words(item: &Item, out: &mut Vec<Span>) {
    out.extend(item.word);
    out.extend(item.modifier);
    out.extend(item.timeout.map(|(word, _)| word));
    if let Kind::Suite(items) = &item.kind {
        for inner in items {
            words(inner, out);
        }
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
