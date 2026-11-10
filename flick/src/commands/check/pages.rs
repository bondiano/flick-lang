//! `flick check` of Markdown pages: each ```` ```flick ```` block is checked
//! on its own, as the site's corpus test does, and reported in the page;
//! `--fix` writes the safe fixes of its compiled text back into the page.

use std::fs;
use std::path::Path;

use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::markdown::{self, Block, Mode};
use flick_core::source::{FileId, SourceMap};

use super::{FIX_PASSES, chosen, taken};

/// Whether `path` names a Markdown page rather than Flick sources.
pub fn is_page(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|e| e == "md" || e == "mdx")
}

/// The diagnostics of the blocks of the page at `path`, which is added to
/// `sources`; with `fix`, the page is first rewritten with their safe fixes
/// of the codes `fix` lets through, except in `keep` blocks. An `ignore` block is not checked; a `fail` block
/// is not either, as its errors are the example and a fix could remove them.
pub fn check(sources: &mut SourceMap, path: &str, fix: Option<&[String]>) -> Vec<Diagnostic> {
    let error = |code, message: String| vec![Diagnostic::new(code, Severity::Error, message)];
    let read = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => return error(codes::P001, format!("cannot read `{path}`: {err}")),
    };
    let mut page = read.clone();
    let mut checked = check_blocks(&page);
    let passes = if fix.is_some() { FIX_PASSES } else { 0 };
    for _ in 0..passes {
        let fixed = fixed(&page, &checked, fix.unwrap_or_default());
        if fixed == page {
            break;
        }
        page = fixed;
        checked = check_blocks(&page);
    }
    if page != read
        && let Err(err) = fs::write(path, &page)
    {
        return error(codes::P004, format!("cannot write `{path}`: {err}"));
    }
    let file = sources.add(path, page);
    checked
        .into_iter()
        .flat_map(|(block, diagnostics)| {
            let in_page = move |d| in_page(&block, d, file);
            diagnostics.into_iter().map(in_page)
        })
        .collect()
}

/// Each block of `page` that must check, with its diagnostics in its text.
fn check_blocks(page: &str) -> Vec<(Block, Vec<Diagnostic>)> {
    markdown::blocks(page)
        .into_iter()
        .filter(|b| matches!(b.mode, Mode::Check | Mode::Keep))
        .map(|block| {
            let mut sources = SourceMap::default();
            let file = sources.add("", block.text.as_str());
            let diagnostics = flick_core::check(&sources, file);
            (block, diagnostics)
        })
        .collect()
}

/// `page` with the safe fixes of the diagnostics of its `checked` blocks.
fn fixed(page: &str, checked: &[(Block, Vec<Diagnostic>)], only: &[String]) -> String {
    let mut out = page.to_owned();
    // the last block first, so the ranges of those before it hold
    let fixable = checked.iter().filter(|(b, _)| b.mode == Mode::Check);
    for (block, diagnostics) in fixable.rev() {
        let fixes = diagnostics
            .iter()
            .filter(|d| chosen(d, only))
            .flat_map(|d| &d.fixes)
            .filter(|f| f.safe)
            .map(|f| f.edits.as_slice());
        let edits = taken(fixes.collect());
        if !edits.is_empty() {
            out.replace_range(block.range(), &block.restore(page, &edits));
        }
    }
    out
}

/// `d`, a diagnostic of `block`'s text, with its spans in `page`.
fn in_page(block: &Block, mut d: Diagnostic, page: FileId) -> Diagnostic {
    d.span = d.span.map(|s| block.page_span(s, page));
    for related in &mut d.related {
        related.span = block.page_span(related.span, page);
    }
    for edit in d.fixes.iter_mut().flat_map(|f| &mut f.edits) {
        edit.span = block.page_span(edit.span, page);
    }
    d
}
