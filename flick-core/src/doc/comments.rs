//! `///` documentation comment extraction with source byte ranges for span
//! remapping back into `.fl` files.

use std::collections::HashMap;
use std::ptr;

use crate::syntax::cst::{Element, Node};
use crate::syntax::kind::SyntaxKind as K;

/// One `///` line of a doc comment, with its byte range in the original source
/// text (after `///` and optional single space, excluding line ending) and its
/// starting byte offset inside [`DocComment::text`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocLine {
    pub source_start: u32,
    pub source_end: u32,
    pub joined_start: u32,
}

/// A `///` documentation comment with its cleaned Markdown text and a line
/// table mapping byte offsets in `text` back to the original `.fl` source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocComment {
    pub text: String,
    pub lines: Vec<DocLine>,
}

impl DocComment {
    /// Maps a byte range `start..end` inside [`Self::text`] back to a byte
    /// range in the original source text.
    pub fn source_span(&self, start: u32, end: u32) -> Option<(u32, u32)> {
        if self.lines.is_empty() {
            return None;
        }
        let end = end.max(start);
        let map_point = |at: u32| -> u32 {
            let idx = self
                .lines
                .partition_point(|l| l.joined_start <= at)
                .saturating_sub(1);
            let line = self.lines[idx];
            let len = line.source_end.saturating_sub(line.source_start);
            let rel = at.saturating_sub(line.joined_start).min(len);
            line.source_start.saturating_add(rel)
        };
        let src_start = map_point(start);
        let src_end = if end > start {
            let idx = self
                .lines
                .partition_point(|l| l.joined_start <= end)
                .saturating_sub(1);
            let line = self.lines[idx];
            if idx > 0 && end == line.joined_start {
                self.lines[idx - 1].source_end
            } else {
                map_point(end)
            }
        } else {
            src_start
        };
        Some((src_start, src_end.max(src_start)))
    }
}

fn build_doc_comment(pending: &[(u32, u32, &str)]) -> DocComment {
    let mut text = String::new();
    let mut lines = Vec::with_capacity(pending.len());
    for (i, &(source_start, source_end, content)) in pending.iter().enumerate() {
        if i > 0 {
            text.push('\n');
        }
        let joined_start = u32::try_from(text.len()).expect("doc comment offset fits u32");
        text.push_str(content);
        lines.push(DocLine {
            source_start,
            source_end,
            joined_start,
        });
    }
    DocComment { text, lines }
}

/// The `///` comment right above each top-level declaration, and above
/// each constructor on a line of its own (`| Circle Float`), by name,
/// with source line offsets for span remapping. A `let` takes the lines
/// only right above it, as generated code writes it.
pub fn located_comments(text: &str) -> HashMap<String, DocComment> {
    let mut out = HashMap::new();
    let mut pending: Vec<(u32, u32, &str)> = Vec::new();
    let mut adjacent = false;
    let mut offset = 0usize;
    for raw in text.split_inclusive('\n') {
        let line = raw
            .strip_suffix('\n')
            .map_or(raw, |l| l.strip_suffix('\r').unwrap_or(l));
        let trimmed = line.trim_start();
        let leading = line.len() - trimmed.len();
        if let Some(doc) = trimmed.strip_prefix("///") {
            let space = usize::from(doc.starts_with(' '));
            let content = &doc[space..];
            let start = u32::try_from(offset + leading + 3 + space).unwrap_or(u32::MAX);
            let end = start.saturating_add(u32::try_from(content.len()).unwrap_or(u32::MAX));
            pending.push((start, end, content));
            adjacent = true;
            offset += raw.len();
            continue;
        }
        let name = declared(line).or_else(|| variant(line));
        if !pending.is_empty()
            && let Some(name) = name.or_else(|| adjacent.then(|| defined(line)).flatten())
        {
            out.entry(name)
                .or_insert_with(|| build_doc_comment(&pending));
        }
        adjacent = false;
        if !line.trim().is_empty() {
            pending.clear();
        }
        offset += raw.len();
    }
    out
}

/// The `///` comment right above each top-level declaration, and above
/// each constructor on a line of its own (`| Circle Float`), by name. A
/// `let` takes the lines only right above it, as generated code writes it.
pub fn comments(text: &str) -> HashMap<String, String> {
    located_comments(text)
        .into_iter()
        .map(|(name, comment)| (name, comment.text))
        .collect()
}

/// The value a top-level `let [rec] name …` defines.
fn defined(line: &str) -> Option<String> {
    let mut words = line.strip_prefix("let ")?.split_whitespace();
    let name = words
        .next()
        .filter(|w| *w != "rec")
        .or_else(|| words.next())?;
    Some(name.to_owned())
}

/// The `///` documentation immediately before the module header, with source
/// line offsets for span remapping.
pub fn located_module_comment(text: &str) -> Option<DocComment> {
    let mut pending: Vec<(u32, u32, &str)> = Vec::new();
    let mut offset = 0usize;
    for raw in text.split_inclusive('\n') {
        let line = raw
            .strip_suffix('\n')
            .map_or(raw, |l| l.strip_suffix('\r').unwrap_or(l));
        if line.starts_with("module ") {
            return (!pending.is_empty()).then(|| build_doc_comment(&pending));
        }
        if let Some(doc) = line.strip_prefix("///") {
            let space = usize::from(doc.starts_with(' '));
            let content = &doc[space..];
            let start = u32::try_from(offset + 3 + space).unwrap_or(u32::MAX);
            let end = start.saturating_add(u32::try_from(content.len()).unwrap_or(u32::MAX));
            pending.push((start, end, content));
        } else if !line.trim().is_empty() {
            pending.clear();
        }
        offset += raw.len();
    }
    None
}

/// The `///` documentation immediately before the module header.
pub fn module_comment(text: &str) -> Option<String> {
    located_module_comment(text).map(|comment| comment.text)
}

/// The constructor a line `| Name …` of a type declaration starts.
fn variant(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix('|')?.trim_start();
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    name.starts_with(|c: char| c.is_ascii_uppercase())
        .then_some(name)
}

/// The `///` comment right above `child` among the children of `parent`,
/// where the parser keeps the comments before a node.
pub fn leading_doc(parent: &Node, child: &Node, text: &str) -> Option<String> {
    let at = parent
        .children
        .iter()
        .position(|c| matches!(c, Element::Node(n) if ptr::eq(n, child)))?;
    let mut lines = Vec::new();
    for element in parent.children[..at].iter().rev() {
        match element {
            Element::Token(t) if t.kind == K::DocComment => {
                let doc = t
                    .text(text)
                    .trim_end()
                    .strip_prefix("///")
                    .unwrap_or_default();
                lines.push(doc.strip_prefix(' ').unwrap_or(doc));
            }
            Element::Token(t) if t.kind.is_trivia() && !t.kind.is_comment() => {}
            _ => break,
        }
    }
    lines.reverse();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// The name a top-level line declares: `name :`, `type [alias|error] N`,
/// `service N`, `class … N`, `sql table N`, `kind N`.
fn declared(line: &str) -> Option<String> {
    if line.starts_with(' ') {
        return None;
    }
    let words: Vec<&str> = line.split_whitespace().collect();
    if words.starts_with(&["type", "module"]) {
        return words
            .get(2)
            .map(|name| name.trim_end_matches('=').to_owned());
    }
    let skip = [
        "type", "alias", "error", "opaque", "refined", "unboxed", "untagged", "service", "class",
        "kind", "sql", "table",
    ];
    if words.get(1) == Some(&":") {
        return Some(words[0].to_owned());
    }
    if !words.first().is_some_and(|f| skip.contains(f)) {
        return None;
    }
    let name = words.iter().find(|w| !skip.contains(w))?;
    Some(
        name.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '_')
            .to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown;

    #[test]
    fn comments_belong_to_the_declaration_below() {
        let docs = comments(
            "/// Adds.\n/// Twice.\nadd : Float -> Float\n\n/// A user.\ntype alias User = {}\n/// lost\n\nlet x = 1\n",
        );
        assert_eq!(docs.get("add").map(String::as_str), Some("Adds.\nTwice."));
        assert_eq!(docs.get("User").map(String::as_str), Some("A user."));
        assert!(!docs.contains_key("x"));
        let docs = comments("/// Made.\nlet rec made = 1\n");
        assert_eq!(docs.get("made").map(String::as_str), Some("Made."));
        let docs = comments("/// Contract.\ntype module HashKey =\n  type Key\n");
        assert_eq!(docs.get("HashKey").map(String::as_str), Some("Contract."));
    }

    #[test]
    fn located_comments_map_markdown_and_hidden_lines_back_to_source() {
        let source = "\
/// Module doc.
///
/// ```flick
/// # let hidden = 1
/// let bad : String = hidden
/// ```
module Lib.Api exposing (answer)

/// Example with CRLF.\r\n/// ```flick\r\n/// let x : String = 1\r\n/// ```\r\nanswer : Int\r\nlet answer = 42\r\n";

        let mod_doc = located_module_comment(source).expect("module doc");
        let blocks = markdown::blocks(&mod_doc.text);
        assert_eq!(blocks.len(), 1);
        let block = &blocks[0];
        // "hidden" on line 2 of block.text ("let hidden = 1\nlet bad : String = hidden\n")
        let needle = "hidden";
        let at = block.text.rfind(needle).expect("needle");
        let page_start = u32::try_from(block.page_offset(at)).expect("u32 offset");
        let page_end = u32::try_from(block.page_offset(at + needle.len())).expect("u32 offset");
        let (src_start, src_end) = mod_doc
            .source_span(page_start, page_end)
            .expect("source span");
        assert_eq!(&source[src_start as usize..src_end as usize], "hidden");
        // Verify it points at the second occurrence (on `let bad : String = hidden`), not `# let hidden`
        assert!(
            source[..src_start as usize].ends_with("/// let bad : String = "),
            "{:?}",
            &source[..src_start as usize]
        );

        let decl_docs = located_comments(source);
        let ans_doc = decl_docs.get("answer").expect("answer doc");
        let ans_blocks = markdown::blocks(&ans_doc.text);
        assert_eq!(ans_blocks.len(), 1);
        let ans_block = &ans_blocks[0];
        let one = ans_block.text.find('1').expect("1");
        let p_start = u32::try_from(ans_block.page_offset(one)).expect("u32 offset");
        let p_end = u32::try_from(ans_block.page_offset(one + 1)).expect("u32 offset");
        let (s_start, s_end) = ans_doc.source_span(p_start, p_end).expect("crlf span");
        assert_eq!(&source[s_start as usize..s_end as usize], "1");

        // Block range end at closing fence stays on the last code line, not the closing ```
        let range = ans_block.range();
        let (b_start, b_end) = ans_doc
            .source_span(
                u32::try_from(range.start).expect("u32 start"),
                u32::try_from(range.end).expect("u32 end"),
            )
            .expect("block range");
        assert_eq!(
            &source[b_start as usize..b_end as usize],
            "let x : String = 1"
        );
    }
}
