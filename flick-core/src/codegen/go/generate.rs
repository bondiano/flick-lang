//! `for`: a strict `[for …]` runs its loops at once and appends the
//! elements to a list; a lazy one is a `flickrt.Seq` whose function runs
//! the same loops on each iteration and yields the elements, stopping when
//! the consumer does.

use std::{iter, mem};

use flick_pretty::docvec;

use super::Gen;
use super::layout::{Code, block, call, if_else, single, statement};
use crate::ir::{Expr, ExprKind, Lit, Pat};

/// Where the elements of the `for` being emitted go.
#[derive(Clone, Copy)]
pub(super) enum Collector<'a> {
    /// Appended to the list in this variable.
    List(&'a str),
    /// Yielded; with a fused limit, its variable and the count's.
    Yield(Option<(Code<'a>, &'a str)>),
}

impl<'a> Gen<'a> {
    /// The list a strict `for` with the clauses `body` builds.
    pub(super) fn strict(&mut self, body: &Expr) -> Code<'a> {
        let out = self.temp();
        self.emit(docvec![self.a, out, " := []flickrt.Value{}"]);
        let saved = self.collector.replace(Collector::List(out));
        self.discard(body);
        self.collector = saved;
        self.code(out)
    }

    /// The Seq a lazy `for` with the clauses `body` makes; a fused limit is
    /// computed now, and counted on each iteration.
    pub(super) fn lazy(&mut self, body: &Expr, limit: Option<&Expr>) -> Code<'a> {
        let a = self.a;
        let limit = limit.map(|l| {
            let n = self.expr(l);
            let n = call(a, "flickrt.Count", [n.doc]).into();
            let n = self.spill(n);
            (n, self.temp())
        });
        let saved = (
            self.collector.replace(Collector::Yield(limit)),
            self.tail.take(),
            mem::replace(&mut self.generator, true),
            self.fiber.take(),
            self.context.replace("fb"),
        );
        let (mut stmts, ()) = self.block(|g| g.discard(body));
        (
            self.collector,
            self.tail,
            self.generator,
            self.fiber,
            self.context,
        ) = saved;
        if let Some((n, count)) = limit {
            let none = call(a, "flickrt.CountOf", [n.doc]);
            let start = [
                statement(a, docvec![a, count, " := 0"]),
                statement(a, docvec![a, "if ", none, " <= 0 ", single(a, "return")]),
            ];
            stmts.splice(0..0, start);
        }
        let f = docvec![
            a,
            "func(fb *flickrt.Fiber, yield func(flickrt.Value) bool) ",
            block(a, stmts)
        ];
        call(a, "flickrt.Seq", [f]).into()
    }

    /// Emits `e` for what it does: the loops, guards and elements of a
    /// strict `for`.
    pub(super) fn discard(&mut self, e: &Expr) {
        let a = self.a;
        match &e.kind {
            ExprKind::Each { pat, source, body } => {
                let s = self.expr(source);
                let var = self.temp();
                let (inner, ()) = self.block(|g| match pat {
                    Pat::Bind(name) => {
                        g.bind(name, g.code(var));
                        g.discard(body);
                    }
                    _ => g.arm(a.text(var), pat, None, body, Self::discard),
                });
                // a pattern that ignores the element leaves the variable unused
                let used = statement(a, docvec![a, "_ = ", var]);
                let body = block(a, iter::once(used).chain(inner));
                let range = call(a, "flickrt.Iter", [self.context(), s.doc]);
                self.emit(docvec![a, "for ", var, " := range ", range, " ", body]);
            }
            ExprKind::Yield(value) => {
                let v = self.expr(value);
                match self.collector.expect("a yield is inside a `for`") {
                    Collector::List(out) => {
                        let append = call(a, "append", [a.text(out), v.doc]);
                        self.emit(docvec![a, out, " = ", append]);
                    }
                    Collector::Yield(limit) => {
                        let go = call(a, "yield", [v.doc]);
                        self.emit(docvec![a, "if !", go, " ", single(a, "return")]);
                        if let Some((n, count)) = limit {
                            let limit = call(a, "flickrt.CountOf", [n.doc]);
                            let test = docvec![a, "if ", count, "++; ", count, " >= ", limit];
                            self.emit(docvec![a, test, " ", single(a, "return")]);
                        }
                    }
                }
            }
            ExprKind::If(cond, then, other) => {
                let cond = self.expr(cond);
                let (then, ()) = self.block(|g| g.discard(then));
                let (other, ()) = self.block(|g| g.discard(other));
                self.emit(if_else(a, cond, then, other));
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
                self.emit(docvec![a, "_ = ", v]);
            }
        }
    }
}
