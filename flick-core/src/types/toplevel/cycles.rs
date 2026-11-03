//! Top-level recursion that is not declared: a function that uses itself
//! without `let rec`, functions that use each other outside one `rec`
//! group, and values that recurse at all.

use crate::diagnostics::{Diagnostic, Edit, codes};
use crate::resolve;
use crate::source::Span;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Checker;

impl Checker<'_> {
    /// Reports recursion that is not declared with `rec`.
    pub(super) fn recursion(&mut self) {
        let n = self.top.items.len();
        let mut next: Vec<Vec<usize>> = vec![Vec::new(); n];
        for &(from, to, _) in &self.top.edges {
            if !next[from].contains(&to) {
                next[from].push(to);
            }
        }
        let mut reported = vec![false; n];
        for start in 0..n {
            if reported[start] {
                continue;
            }
            let Some(cycle) = resolve::shortest_cycle(start, &next) else {
                continue;
            };
            let items = &self.top.items;
            let group = items[start].group;
            if group.is_some() && cycle.iter().all(|&i| items[i].group == group) {
                continue;
            }
            for &i in &cycle {
                reported[i] = true;
            }
            let after = cycle.get(1).copied().unwrap_or(start);
            let span = self
                .top
                .edges
                .iter()
                .find(|(f, t, _)| *f == start && *t == after)
                .map(|e| e.2)
                .expect("an edge of the cycle");
            let names: Vec<String> = cycle
                .iter()
                .map(|&i| format!("`{}`", items[i].names.join("`, `")))
                .collect();
            let values = cycle.iter().any(|&i| !items[i].function);
            let (code, message) = match (values, &names[..]) {
                (true, _) => (
                    codes::T012,
                    format!(
                        "{} would be evaluated while being defined; only functions can be recursive",
                        names.join(" and ")
                    ),
                ),
                (false, [one]) => (
                    codes::T017,
                    format!("{one} uses itself: write `let rec` for a recursive function"),
                ),
                (false, _) => (
                    codes::T017,
                    format!(
                        "{} use each other: define mutually recursive functions in one `rec {{ … }}` group",
                        names.join(", ")
                    ),
                ),
            };
            let mut diagnostic = Diagnostic::error(code, message, span);
            if let ([only], false) = (&cycle[..], values) {
                let let_kw = items[*only]
                    .decl
                    .child_tokens()
                    .find(|t| t.kind == K::LetKw);
                if let Some(kw) = let_kw {
                    diagnostic = diagnostic.with_fix(|| {
                        (
                            "write `let rec`",
                            vec![Edit {
                                span: Span::new(span.file, kw.end, kw.end),
                                replacement: " rec".to_owned(),
                            }],
                        )
                    });
                }
            }
            self.diagnostics.push(diagnostic);
        }
    }
}
