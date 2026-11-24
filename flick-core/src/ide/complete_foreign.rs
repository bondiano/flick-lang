//! Completion inside `foreign` declaration metadata blocks: offers ONLY the
//! core contract words and the active package backend's declared adapter words
//! (never another backend's adapter words or unrelated value symbols).

use super::complete::{Completion, CompletionKind, item};
use super::{Ide, tree};
use crate::source::FileId;
use crate::syntax::cst::{Node, foreign_words};
use crate::syntax::kind::SyntaxKind as K;
use crate::types::{AdapterArgKind, CORE_FOREIGN_META, ForeignBackendSpec, core_foreign_meta};

impl Ide<'_> {
    /// Completions when `offset` is inside a `foreign` declaration's metadata
    /// block or on an indented continuation line beneath a non-blocked `foreign`
    /// declaration. Returns `None` outside foreign metadata (or at the `catch`
    /// mapper expression slot, where normal expression completion applies).
    pub(super) fn foreign_meta_complete(
        &self,
        file: FileId,
        offset: u32,
    ) -> Option<Vec<Completion>> {
        let m = self.module(file)?;
        let text = self.text(file);
        let spec = &self.analysis.foreign_backend;
        let path = tree::path_to(&m.tree, offset);

        if let Some(meta_item) = path.iter().rev().find(|n| n.kind == K::MetaItem) {
            return complete_meta_item(meta_item, text, offset, spec);
        }

        if let Some(foreign_decl) = path.iter().rev().find(|n| n.kind == K::ForeignDecl) {
            if is_blocked_foreign(foreign_decl, text) {
                return None;
            }
            if path.iter().any(|n| n.kind == K::ForeignMeta) {
                return complete_indented_line(foreign_decl, text, offset, spec);
            }
            // Cursor may be at EOF or on an indented blank line right after ForeignSource.
            if let Some(source) = foreign_decl.nodes().find(|n| n.kind == K::ForeignSource)
                && offset >= source.range().1
            {
                return complete_indented_line(foreign_decl, text, offset, spec);
            }
        }

        if let Some(ov) = path.iter().rev().find(|n| n.kind == K::NativeOverride) {
            if path.iter().any(|n| n.kind == K::ForeignMeta) {
                return complete_indented_line(ov, text, offset, spec);
            }
            if let Some(when_cond) = ov.nodes().find(|n| n.kind == K::WhenCond)
                && offset >= when_cond.range().1
            {
                return complete_indented_line(ov, text, offset, spec);
            }
        }

        // Cursor on a blank indented line immediately following a non-blocked
        // `foreign` declaration or `@native` override whose CST node ended at the previous line.
        let prev = m.tree.nodes().filter(|n| n.range().1 <= offset).last()?;
        if prev.kind == K::ForeignDecl && !is_blocked_foreign(prev, text) {
            return complete_indented_line(prev, text, offset, spec);
        }
        if prev.kind == K::LetDecl
            && let Some(last_ov) = prev
                .nodes()
                .filter(|n| n.kind == K::NativeOverride && n.range().1 <= offset)
                .last()
        {
            return complete_indented_line(last_ov, text, offset, spec);
        }
        None
    }
}

fn is_blocked_foreign(decl: &Node, text: &str) -> bool {
    foreign_words(decl)
        .0
        .is_some_and(|tok| tok.text(text) == "blocked")
}

fn complete_meta_item(
    item_node: &Node,
    text: &str,
    offset: u32,
    spec: &ForeignBackendSpec,
) -> Option<Vec<Completion>> {
    let word_tok = item_node.child_tokens().find(|t| t.kind == K::Ident)?;
    if offset <= word_tok.end {
        return Some(word_completions(spec));
    }
    let word = word_tok.text(text);
    let args: Vec<&Node> = item_node.nodes().collect();
    let arg_idx = args
        .iter()
        .position(|n| {
            let (s, e) = n.range();
            s <= offset && offset <= e
        })
        .unwrap_or_else(|| args.iter().filter(|n| n.range().1 <= offset).count());
    arg_completions(word, arg_idx, spec)
}

fn complete_indented_line(
    decl: &Node,
    text: &str,
    offset: u32,
    spec: &ForeignBackendSpec,
) -> Option<Vec<Completion>> {
    let off = (offset as usize).min(text.len());
    let before = &text[..off];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let decl_start = (decl.range().0 as usize).min(text.len());
    // Must be on a subsequent line after the `foreign` header line.
    if line_start <= decl_start {
        return None;
    }
    let decl_line_start = text[..decl_start].rfind('\n').map_or(0, |i| i + 1);
    let decl_indent = decl_start - decl_line_start;
    let line_prefix = &text[line_start..off];
    let line_indent = line_prefix
        .bytes()
        .take_while(|b| matches!(b, b' ' | b'\t'))
        .count();
    if line_indent <= decl_indent {
        return None;
    }
    let trimmed = line_prefix.trim_start();
    if trimmed.is_empty() {
        return Some(word_completions(spec));
    }
    let mut parts = trimmed.split_whitespace();
    let word = parts.next()?;
    let trailing_space = line_prefix.ends_with(|c: char| c.is_ascii_whitespace());
    let arg_count = parts.count();
    if arg_count == 0 && !trailing_space {
        return Some(word_completions(spec));
    }
    let arg_idx = if trailing_space {
        arg_count
    } else {
        arg_count.saturating_sub(1)
    };
    arg_completions(word, arg_idx, spec)
}

fn word_completions(spec: &ForeignBackendSpec) -> Vec<Completion> {
    let mut out = Vec::new();
    for core in CORE_FOREIGN_META {
        out.push(Completion {
            documentation: Some(core.doc.to_owned()),
            snippet: core.snippet.map(str::to_owned),
            sort: 0,
            ..item(
                core.name,
                CompletionKind::Keyword,
                Some(core.synopsis.to_owned()),
            )
        });
    }
    for adapter in &spec.adapters {
        out.push(Completion {
            documentation: Some(format!(
                "{}\n\nBackend adapter (`{}`).",
                adapter.doc, spec.backend_id
            )),
            snippet: adapter.snippet(),
            sort: 1,
            ..item(
                &adapter.name,
                CompletionKind::Keyword,
                Some(adapter.synopsis()),
            )
        });
    }
    out
}

fn arg_completions(
    word: &str,
    arg_idx: usize,
    spec: &ForeignBackendSpec,
) -> Option<Vec<Completion>> {
    if word == "catch" {
        return None;
    }
    if let Some(core) = core_foreign_meta(word) {
        if arg_idx == 0 && !core.keyword_choices.is_empty() {
            return Some(
                core.keyword_choices
                    .iter()
                    .map(|choice| {
                        item(
                            choice,
                            CompletionKind::Keyword,
                            Some(format!("{word} {choice}")),
                        )
                    })
                    .collect(),
            );
        }
        return Some(Vec::new());
    }
    if let Some(adapter) = spec.adapter(word) {
        if let Some(arg) = adapter.args.get(arg_idx)
            && let AdapterArgKind::Keyword(choices) = &arg.kind
        {
            return Some(
                choices
                    .iter()
                    .map(|choice| {
                        item(
                            choice,
                            CompletionKind::Keyword,
                            Some(format!("{} ({})", adapter.synopsis(), spec.backend_id)),
                        )
                    })
                    .collect(),
            );
        }
        return Some(Vec::new());
    }
    Some(Vec::new())
}
