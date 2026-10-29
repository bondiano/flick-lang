//! `for`: a strict `[for …]` runs its loops at once and appends the
//! elements to a list; a lazy one is a `flickrt.Seq` whose function runs
//! the same loops on each iteration and yields the elements, stopping when
//! the consumer does.

use std::mem;

use super::{Gen, indent};
use crate::ir::{Expr, ExprKind, Lit, Pat};

/// Where the elements of the `for` being emitted go.
#[derive(Clone)]
pub(super) enum Collector {
    /// Appended to the list in this variable.
    List(String),
    /// Yielded; with a fused limit, its variable and the count's.
    Yield(Option<(String, String)>),
}

impl Gen<'_> {
    /// The list a strict `for` with the clauses `body` builds.
    pub(super) fn strict(&mut self, body: &Expr) -> String {
        let out = self.temp();
        self.buf.push(format!("{out} := []flickrt.Value{{}}"));
        let saved = self.collector.replace(Collector::List(out.clone()));
        self.discard(body);
        self.collector = saved;
        out
    }

    /// The Seq a lazy `for` with the clauses `body` makes; a fused limit is
    /// computed now, and counted on each iteration.
    pub(super) fn lazy(&mut self, body: &Expr, limit: Option<&Expr>) -> String {
        let limit = limit.map(|l| {
            let n = self.expr(l);
            let n = format!("flickrt.Count({n})");
            let n = self.spill(n);
            (n, self.temp())
        });
        let saved = (
            self.collector.replace(Collector::Yield(limit.clone())),
            self.tail.take(),
            mem::replace(&mut self.generator, true),
        );
        let (mut stmts, ()) = self.block(|g| g.discard(body));
        (self.collector, self.tail, self.generator) = saved;
        if let Some((n, count)) = &limit {
            stmts.insert(
                0,
                format!(
                    "{count} := 0
if flickrt.CountOf({n}) <= 0 {{
\treturn
}}"
                ),
            );
        }
        format!(
            "flickrt.Seq(func(yield func(flickrt.Value) bool) {{\n{}}})",
            indent(&stmts)
        )
    }

    /// Emits `e` for what it does: the loops, guards and elements of a
    /// strict `for`.
    pub(super) fn discard(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Each { pat, source, body } => {
                let s = self.expr(source);
                let var = self.temp();
                let (inner, ()) = self.block(|g| match pat {
                    Pat::Bind(name) => {
                        g.bind(name, &var);
                        g.discard(body);
                    }
                    _ => g.arm(&var, pat, None, body, Self::discard),
                });
                self.buf.push(format!(
                    "for {var} := range flickrt.Iter({s}) {{\n{}}}",
                    indent(&inner)
                ));
            }
            ExprKind::Yield(value) => {
                let v = self.expr(value);
                match self.collector.clone().expect("a yield is inside a `for`") {
                    Collector::List(out) => self.buf.push(format!("{out} = append({out}, {v})")),
                    Collector::Yield(limit) => {
                        self.buf.push(format!("if !yield({v}) {{\n\treturn\n}}"));
                        if let Some((n, count)) = limit {
                            self.buf.push(format!(
                                "if {count}++; {count} >= flickrt.CountOf({n}) {{\n\treturn\n}}"
                            ));
                        }
                    }
                }
            }
            ExprKind::If(c, t, f) => {
                let c = self.expr(c);
                let (ts, ()) = self.block(|g| g.discard(t));
                let (fs, ()) = self.block(|g| g.discard(f));
                self.buf.push(format!(
                    "if flickrt.Bool({c}) {{\n{}}} else {{\n{}}}",
                    indent(&ts),
                    indent(&fs)
                ));
            }
            ExprKind::Block(stmts, result) => {
                for stmt in stmts {
                    self.stmt(stmt);
                }
                self.discard(result);
            }
            ExprKind::Lit(Lit::Unit) => {}
            _ => {
                let v = self.expr(e);
                // a result read by nothing: Go refuses a variable nobody reads
                self.buf.push(format!("_ = {v}"));
            }
        }
    }
}
