//! An accessor written apart from the value whose field it was meant to
//! read: `user .name`, or `.name` on the line after `user`. The parser
//! takes it for an argument or for a statement of its own; the error that
//! follows gets a help naming the accessor and, after a plain value, a fix
//! that glues them: `user.name`. The fix changes the program, so it is not
//! safe.

use super::Checker;
use super::bindings::let_parts;
use crate::diagnostics::{Diagnostic, Edit, Fix, Severity, codes};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// The accessors at the start of `nodes`, as written: not in parentheses.
pub(super) fn leading_accessors<'n>(nodes: &[&'n Node]) -> Vec<&'n Node> {
    (nodes.iter())
        .take_while(|n| n.kind == K::AccessorExpr)
        .copied()
        .collect()
}

impl Checker<'_> {
    /// The help for `accessors` written apart from `value`, which is `None`
    /// for an application; with the edits that glue them to `value` when it
    /// is a name, a field read or in parentheses and nothing but whitespace
    /// stands between them.
    pub(super) fn detached_fix(&self, value: Option<&Node>, accessors: &[&Node]) -> Fix {
        let first = self.text_of(accessors[0]);
        let value = value.filter(|v| matches!(v.kind, K::NameExpr | K::FieldExpr | K::ParenExpr));
        let start = format!(
            "`{first}` here starts a separate accessor, a function; to read the field, write it right after the value"
        );
        let Some(value) = value else {
            return Fix {
                message: format!("{start}, with no space or line break"),
                edits: Vec::new(),
                safe: false,
            };
        };
        let path: String = accessors.iter().map(|a| self.text_of(a)).collect();
        let mut edits = Vec::new();
        let mut end = value.range().1;
        for accessor in accessors {
            let next = accessor.range().0;
            // a comment between them stays
            if !self.text[end as usize..next as usize].trim().is_empty() {
                edits.clear();
                break;
            }
            edits.push(Edit {
                span: Span::new(self.file, end, next),
                replacement: String::new(),
            });
            end = accessor.range().1;
        }
        Fix {
            message: format!("{start}: `{}{path}`", self.text_of(value)),
            edits,
            safe: false,
        }
    }

    /// Runs of accessors that start lines of a block right after an
    /// expression or a `let` (`user` / `.name`): the first error about them
    /// reported since `since` gets the help and the fix of
    /// [`Self::detached_fix`]. A glue fix takes the place of the `ignore` of
    /// a discarded value.
    pub(super) fn detached_statements(&mut self, statements: &[&Node], since: usize) {
        for i in 1..statements.len() {
            let before = statements[i - 1];
            // a run starts after a statement that is no accessor of its own
            if statements[i].kind != K::AccessorExpr
                || matches!(before.kind, K::Signature | K::AccessorExpr)
                || !self.starts_line(statements[i])
            {
                continue;
            }
            let run: Vec<&Node> = (statements[i..].iter())
                .take_while(|n| n.kind == K::AccessorExpr && self.starts_line(n))
                .copied()
                .collect();
            let value = if before.kind == K::LetDecl {
                let_parts(before, self.text).body.and_then(|body| {
                    let mut nodes = body.nodes();
                    match (nodes.next(), nodes.next()) {
                        (Some(value), None) => Some(value),
                        _ => None,
                    }
                })
            } else {
                Some(before)
            };
            self.detached_run(before, value, &run, since);
        }
    }

    /// [`Self::detached_statements`] for the accessors `run` after the
    /// statement `before`, of value `value`.
    fn detached_run(&mut self, before: &Node, value: Option<&Node>, run: &[&Node], since: usize) {
        let fix = self.detached_fix(value, run);
        let glued = !fix.edits.is_empty();
        let spans: Vec<(u32, u32)> = run.iter().map(|n| n.range()).collect();
        let before = before.range();
        // a discarded value or accessor, or the accessor given where its
        // block expects a value
        let about = |d: &Diagnostic, value: bool| {
            let Some(span) = d.span else {
                return false;
            };
            let at = (span.start, span.end);
            d.severity == Severity::Error
                && if d.code == codes::T011 {
                    spans.contains(&at) || value && at == before
                } else {
                    d.code == codes::T003
                        && spans.contains(&at)
                        && d.message.ends_with("found a function")
                }
        };
        let Some(first) = (self.diagnostics[since..].iter()).position(|d| about(d, true)) else {
            return;
        };
        let first = since + first;
        let diagnostic = &mut self.diagnostics[first];
        if glued && diagnostic.code == codes::T011 {
            diagnostic.fixes.clear();
        }
        diagnostic.fixes.insert(0, fix);
        // the accessors after it are glued too: discarding them is no fix
        if glued {
            for d in &mut self.diagnostics[first + 1..] {
                if d.code == codes::T011 && about(d, false) {
                    d.fixes.clear();
                }
            }
        }
    }

    /// Whether only whitespace stands before `node` on its line.
    fn starts_line(&self, node: &Node) -> bool {
        let start = node.range().0 as usize;
        (self.text[..start].rsplit('\n').next()).is_some_and(|line| line.trim().is_empty())
    }
}

#[cfg(test)]
#[path = "detached_tests.rs"]
mod tests;
