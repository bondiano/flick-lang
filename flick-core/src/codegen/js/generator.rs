//! `for`: a lazy `Seq` is one generator whose nested `for…of` loops yield
//! its elements; a strict `[for …]` runs the same loops at once and pushes
//! the elements to a list.

use std::mem;

use super::expr::atomic;
use super::{Gen, indent, names};
use crate::ir::{Expr, ExprKind, Lit, Pat};

impl Gen<'_> {
    /// A `Generate`, or a loop or yield of one emitted for what it does.
    pub(super) fn generated(&mut self, e: &Expr) -> String {
        if let ExprKind::Generate {
            body,
            strict,
            sync,
            limit,
        } = &e.kind
        {
            return self.generate(body, *strict, *sync, limit.as_deref());
        }
        self.discard(e);
        "undefined".to_owned()
    }

    fn generate(&mut self, body: &Expr, strict: bool, sync: bool, limit: Option<&Expr>) -> String {
        let tail = self.tail.take();
        let result = if strict {
            let out = self.temp();
            self.buf.push(format!("const {out} = [];"));
            let saved = (self.collector.replace(out.clone()), self.limit.take());
            self.discard(body);
            (self.collector, self.limit) = saved;
            out
        } else {
            // the clauses run no Effect: Sync calls at most, with the
            // services of the stage here, which each step of a `seqIn` reads
            // with a limit, a count of the elements yielded ends it
            // `n` truncated, as `Seq.take` and `List.take` count it
            let limit = limit.map(|l| {
                let n = self.expr(l);
                let whole = n.parse::<f64>().is_ok_and(|n| n.fract() == 0.0);
                let n = if whole { n } else { format!("Math.trunc({n})") };
                (self.spill(n), self.temp())
            });
            let generator = mem::replace(&mut self.generator, false);
            let saved = (
                self.collector.take(),
                mem::replace(&mut self.sync, sync),
                mem::replace(&mut self.limit, limit.clone()),
            );
            let (mut stmts, ()) = self.block(|g| g.discard(body));
            if let Some((n, count)) = &limit {
                stmts.insert(0, format!("let {count} = 0;\nif (!({n} > 0)) return;"));
            }
            (self.collector, self.sync, self.limit) = saved;
            self.generator = generator;
            let function = self.generator_function("", &stmts);
            if !sync {
                format!("{}({function})", self.rt("seq"))
            } else if self.generator && !self.kernel {
                let context = format!("(yield* {})", self.rt("fiberContext"));
                format!("{}({context}, {function})", self.rt("seqIn"))
            } else {
                let context = format!("{}()", self.rt("syncContext"));
                format!("{}({context}, {function})", self.rt("seqIn"))
            }
        };
        self.tail = tail;
        result
    }

    /// Emits `e` for what it does: the loops, guards and yields of a `for`.
    pub(super) fn discard(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Each { pat, source, body } => {
                let s = self.expr(source);
                let (var, test) = match pat {
                    Pat::Bind(name) => (names::ident(name), false),
                    _ => (self.temp(), true),
                };
                let (inner, ()) = self.block(|g| {
                    if test {
                        g.arm(&var, pat, None, body, false, Self::discard);
                    } else {
                        g.discard(body);
                    }
                });
                self.buf
                    .push(format!("for (const {var} of {s}) {{\n{}}}", indent(&inner)));
            }
            ExprKind::Yield(value) => {
                let v = self.expr(value);
                let stmt = match &self.collector {
                    Some(out) => format!("{out}.push({v});"),
                    None => format!("yield {v};"),
                };
                self.buf.push(stmt);
                if let Some((n, count)) = &self.limit {
                    self.buf.push(format!("if (++{count} >= {n}) return;"));
                }
            }
            ExprKind::If(c, t, f) => {
                let c = self.expr(c);
                let (ts, ()) = self.block(|g| g.discard(t));
                let (fs, ()) = self.block(|g| g.discard(f));
                self.buf.push(if fs.is_empty() {
                    format!("if ({c}) {{\n{}}}", indent(&ts))
                } else {
                    format!("if ({c}) {{\n{}}} else {{\n{}}}", indent(&ts), indent(&fs))
                });
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
                    self.buf.push(format!("{v};"));
                }
            }
        }
    }
}
