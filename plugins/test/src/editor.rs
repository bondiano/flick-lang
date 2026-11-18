//! What an editor shows of a block: the words of the plugin, as its
//! structure reads them, as keywords, and what each means.

use flick_plugin::{Block, Highlight, HighlightKind, Hover, Span};

use crate::read::{Item, Kind, structure};

/// What each word of the plugin means, as an editor shows it over the word.
const HELP: [(&str, &str); 4] = [
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
];

/// The plugin's words under `item`: the word of each case inside the block,
/// the block's own being its head, and each modifier.
fn words(item: &Item, out: &mut Vec<Span>) {
    out.extend(item.word);
    out.extend(item.modifier);
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
