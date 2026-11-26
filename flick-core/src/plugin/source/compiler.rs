//! `Compiler` trait implementation for [`BlockSource`] plus token and doc-comment
//! helpers shared with derive target extraction.

use flick_plugin::{
    Compiler, Completion, Hole, HoleKind, Namespace, ReachError, Request, Response, TokenKind,
};

use super::BlockSource;
use crate::syntax::kind::SyntaxKind as K;

impl Compiler for BlockSource {
    fn parse_expr(&mut self, start: u32, end: u32) -> Result<Hole, flick_plugin::Diagnostic> {
        self.parse(HoleKind::Expr, start, end)
    }

    fn parse_type(&mut self, start: u32, end: u32) -> Result<Hole, flick_plugin::Diagnostic> {
        self.parse(HoleKind::Ty, start, end)
    }

    fn parse_pat(&mut self, start: u32, end: u32) -> Result<Hole, flick_plugin::Diagnostic> {
        self.parse(HoleKind::Pat, start, end)
    }

    fn parse_decl(&mut self, start: u32, end: u32) -> Result<Hole, flick_plugin::Diagnostic> {
        self.parse(HoleKind::Decl, start, end)
    }

    fn parse_body(&mut self, start: u32, end: u32) -> Result<Hole, flick_plugin::Diagnostic> {
        self.parse_as(HoleKind::Expr, true, start, end)
    }

    fn parse_lines(&mut self, start: u32, end: u32) -> Result<Hole, flick_plugin::Diagnostic> {
        self.parse(HoleKind::Lines, start, end)
    }

    fn read_file(&mut self, path: &str) -> Result<Vec<u8>, ReachError> {
        self.reach.read_file(path)
    }

    fn env(&mut self, name: &str) -> Result<Option<String>, ReachError> {
        self.reach.env(name)
    }

    fn fetch(&mut self, request: &Request) -> Result<Response, ReachError> {
        self.reach.fetch(request)
    }

    fn literal_of(&mut self, name: &str) -> Option<String> {
        let answer = self.enums.get(name).cloned();
        self.reach.literal_of(name, answer)
    }

    fn published(&mut self, name: &str) -> Option<Vec<u8>> {
        let answer = self.published.get(name).cloned();
        self.reach.published(name, answer)
    }

    fn published_names(&mut self) -> Vec<String> {
        let names: Vec<String> = self.published.keys().cloned().collect();
        self.reach.published_names(names)
    }

    fn in_scope(&mut self, namespace: Namespace, fits: Option<&str>) -> Vec<Completion> {
        (self.in_scope.as_ref()).map_or_else(Vec::new, |s| s.answer(namespace, fits))
    }
}

pub(crate) fn token_kind(kind: K) -> TokenKind {
    match kind {
        K::Ident => TokenKind::Ident,
        K::Con => TokenKind::Con,
        K::Number | K::BigInt => TokenKind::Number,
        K::String => TokenKind::String,
        k if k.is_keyword() => TokenKind::Keyword,
        _ => TokenKind::Punct,
    }
}

/// The `///` lines right above the line of `offset`, skipping any intervening
/// `@attribute` lines between the doc comment and the declaration, each without
/// `///` and one space.
pub(crate) fn docs_above(text: &str, offset: u32) -> String {
    let line = text[..offset as usize].rfind('\n').map_or(0, |i| i + 1);
    let mut lines: Vec<&str> = Vec::new();
    let mut collecting = false;
    for raw in text[..line].lines().rev() {
        let trimmed = raw.trim_start();
        if let Some(doc) = trimmed.strip_prefix("///") {
            collecting = true;
            lines.push(doc.strip_prefix(' ').unwrap_or(doc));
        } else if !collecting && trimmed.starts_with('@') {
            // Skip attribute lines between the doc comment and the declaration.
        } else {
            break;
        }
    }
    lines.reverse();
    lines.join("\n")
}

/// The `///` lines directly above an item (a record field or variant) at
/// `offset`: returns empty if another token precedes `offset` on its own line
/// (other than a single leading `|` when `allow_leading_pipe` is true).
pub(crate) fn docs_above_item(text: &str, offset: u32, allow_leading_pipe: bool) -> String {
    let start = offset as usize;
    let line = text[..start].rfind('\n').map_or(0, |i| i + 1);
    let prefix = text[line..start].trim();
    let owns_line = prefix.is_empty() || (allow_leading_pipe && prefix == "|");
    if !owns_line {
        return String::new();
    }
    docs_above(text, offset)
}
