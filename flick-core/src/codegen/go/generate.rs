//! A strict `[for …]`: its loops run at once and append the elements to a
//! list. A lazy `Seq` is not compiled yet.

use super::expr::atomic;
use super::{Gen, indent};
use crate::ir::{Expr, ExprKind, Lit, Pat};

impl Gen<'_> {
    /// The list a strict `for` with the clauses `body` builds.
    pub(super) fn strict(&mut self, body: &Expr) -> String {
        let out = self.temp();
        self.buf.push(format!("{out} := []flickrt.Value{{}}"));
        let saved = self.collector.replace(out.clone());
        self.discard(body);
        self.collector = saved;
        out
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
                    "for _, {var} := range flickrt.List({s}) {{\n{}}}",
                    indent(&inner)
                ));
            }
            ExprKind::Yield(value) => {
                let v = self.expr(value);
                let out = self
                    .collector
                    .clone()
                    .expect("a yield is inside a strict `for`");
                self.buf.push(format!("{out} = append({out}, {v})"));
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
                if !atomic(&v) {
                    self.buf.push(format!("_ = {v}"));
                }
            }
        }
    }
}
