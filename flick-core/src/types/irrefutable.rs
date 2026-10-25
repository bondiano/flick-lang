//! Patterns of `let` and of parameters must match every value. When one of
//! a local `let` does not, the fixes turn it into a tuple or into a `match`
//! over the rest of its block.

use std::fmt::Write;
use std::ops::Range;

use super::Checker;
use super::bindings::let_parts;
use super::patterns::{Ctor, Pat};
use crate::diagnostics;
use crate::diagnostics::codes;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// A pattern of `let` or of a parameter must match every value. A local
    /// `let` comes with the statements after it in its block, for the fixes.
    pub fn irrefutable(&mut self, pat: &Pat, node: &Node, binding: Option<(&Node, &[&Node])>) {
        if *pat == Pat::Wild {
            return;
        }
        let Some(witness) = self.useful(&[vec![pat.clone()]], &[Pat::Wild]) else {
            return;
        };
        let shown = self.show_pat(&witness[0], false);
        let items = |n: usize| {
            if n == 1 {
                "1 item".to_owned()
            } else {
                format!("{n} items")
            }
        };
        let message = match pat {
            Pat::Ctor(Ctor::List(n), _) => format!(
                "a list can have any length, but this pattern matches only lists of {}, \
                 not `{shown}`; use `match` to handle every length, or a tuple if the length is fixed",
                items(*n)
            ),
            Pat::Ctor(Ctor::ListRest(n), _) => format!(
                "a list can have any length, but this pattern matches only lists of at least {}, \
                 not `{shown}`; use `match` to handle every length",
                items(*n)
            ),
            _ if shown == "_" => {
                "this pattern does not match every value; use `match` to handle every case"
                    .to_owned()
            }
            _ => format!("this pattern does not match `{shown}`; use `match` to handle every case"),
        };
        let mut diagnostic = diagnostics::Diagnostic::error(codes::T009, message, self.span(node));
        if let Some((decl, rest)) = binding {
            let value = let_parts(decl, self.text).body;
            if let Some(edits) = self.tuple_fix(node, value) {
                diagnostic = diagnostic.with_fix("use a tuple", edits);
            }
            if let Some(edit) = self.match_fix(pat, node, decl, value, rest) {
                diagnostic = diagnostic.with_fix("use `match`", vec![edit]);
            }
        }
        self.diagnostics.push(diagnostic);
    }

    /// `let p = e` and the rest of its block become `match e with | p -> rest`,
    /// the rest one level deeper, then an arm `-> todo` for each value `p` misses.
    fn match_fix(
        &self,
        pat: &Pat,
        pattern: &Node,
        decl: &Node,
        value: Option<&Node>,
        rest: &[&Node],
    ) -> Option<diagnostics::Edit> {
        let (start, end) = decl.range();
        let rest_end = rest.last()?.range().1;
        let line = self.text[..start as usize].rfind('\n').map_or(0, |i| i + 1);
        let indent = &self.text[line..start as usize];
        let value = &self.text[range(value?)];
        let after = &self.text[end as usize..rest_end as usize];
        // a type annotation has no place in an arm; moving a multiline string would change it
        let simple = indent.trim().is_empty()
            && let_parts(decl, self.text).header.is_none()
            && !value.contains('\n')
            && !after.contains("\"\"\"");
        if !simple {
            return None;
        }
        let mut text = format!(
            "match {value} with\n{indent}| {} ->",
            &self.text[range(pattern)]
        );
        let mut lines = after.split('\n');
        text.push_str(lines.next().unwrap_or_default());
        for l in lines {
            text.push('\n');
            if !l.trim().is_empty() {
                text.push_str("  ");
            }
            text.push_str(l);
        }
        // lists of every other length are not worth an arm each
        let missing = match self.missing_patterns(vec![vec![pat.clone()]]) {
            (missing, true) => missing,
            (_, false) => vec!["_".to_owned()],
        };
        for m in missing {
            let _ = write!(text, "\n{indent}| {m} -> todo");
        }
        Some(diagnostics::Edit {
            span: Span::new(self.file, start, rest_end),
            replacement: text,
        })
    }

    /// `let [a, b] = [x, y]` becomes `let (a, b) = (x, y)`.
    fn tuple_fix(&self, pattern: &Node, block: Option<&Node>) -> Option<Vec<diagnostics::Edit>> {
        let [value] = block?.nodes().collect::<Vec<_>>()[..] else {
            return None;
        };
        let n = pattern.nodes().count();
        let fits = pattern.kind == K::ListPat
            && value.kind == K::ListExpr
            && n >= 2
            && value.nodes().count() == n
            && pattern.nodes().all(|c| c.kind != K::RestPat);
        if !fits {
            return None;
        }
        let edit = |at: u32, text: &str| diagnostics::Edit {
            span: Span::new(self.file, at, at + 1),
            replacement: text.to_owned(),
        };
        Some(
            [pattern, value]
                .iter()
                .flat_map(|n| {
                    let (start, end) = n.range();
                    [edit(start, "("), edit(end - 1, ")")]
                })
                .collect(),
        )
    }
}

fn range(node: &Node) -> Range<usize> {
    let (start, end) = node.range();
    start as usize..end as usize
}

#[cfg(test)]
#[path = "irrefutable_tests.rs"]
mod tests;
