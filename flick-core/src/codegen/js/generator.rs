//! `for`: a lazy `Seq` is one generator whose nested `for…of` loops yield
//! its elements; a strict `[for …]` runs the same loops at once and pushes
//! the elements to a list.

use std::mem;

use flick_pretty::docvec;

use super::doc::{After, Before, Js, MULTILINE, Stmt, YIELD_TEXT, YIELDS, block, call, stmt_flags};
use super::{Gen, names};
use crate::ir::{Expr, ExprKind, Lit, Pat};

impl<'d> Gen<'_, 'd> {
    /// A `Generate`, or a loop or yield of one emitted for what it does.
    pub(super) fn generated(&mut self, e: &Expr) -> Js<'d> {
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
        self.leaf("undefined")
    }

    fn generate(&mut self, body: &Expr, strict: bool, sync: bool, limit: Option<&Expr>) -> Js<'d> {
        let a = self.a;
        let tail = self.tail.take();
        let result = if strict {
            let out = self.temp();
            self.buf.push(self.line(&format!("const {out} = [];")));
            let saved = (self.collector.replace(out.clone()), self.limit.take());
            self.discard(body);
            (self.collector, self.limit) = saved;
            self.leaf(&out)
        } else {
            // the clauses run no Effect: Sync calls at most, with the
            // services of the stage here, which each step of a `seqIn` reads
            // with a limit, a count of the elements yielded ends it
            // `n` truncated, as `Seq.take` and `List.take` count it
            let limit = limit.map(|l| {
                let n = self.expr(l);
                let whole = n
                    .atom()
                    .is_some_and(|n| n.parse::<f64>().is_ok_and(|n| n.fract() == 0.0));
                let n = if whole {
                    n
                } else {
                    call(a, self.leaf("Math.trunc"), &[n])
                };
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
                let n = n.atom().unwrap_or_default();
                let start = self.line(&format!("let {count} = 0;"));
                let empty = self.line(&format!("if (!({n} > 0)) return;"));
                let doc = docvec![a, start.doc, a.line(), empty.doc];
                stmts.insert(0, Stmt::new(doc, start.flags | empty.flags | MULTILINE));
            }
            (self.collector, self.sync, self.limit) = saved;
            self.generator = generator;
            let function = self.generator_function("", &stmts);
            if !sync {
                let seq = self.rt("seq");
                call(a, seq, &[function])
            } else if self.generator && !self.kernel {
                let fiber_context = self.rt("fiberContext");
                let context = self.yield_star(fiber_context);
                let seq_in = self.rt("seqIn");
                call(a, seq_in, &[context, function])
            } else {
                let sync_context = self.rt("syncContext");
                let context = call(a, sync_context, &[]);
                let seq_in = self.rt("seqIn");
                call(a, seq_in, &[context, function])
            }
        };
        self.tail = tail;
        result
    }

    /// Emits `e` for what it does: the loops, guards and yields of a `for`.
    pub(super) fn discard(&mut self, e: &Expr) {
        let arena = self.a;
        match &e.kind {
            ExprKind::Each {
                pat,
                source,
                body,
                stop,
            } => {
                let s = self.expr(source);
                let s = if stop.is_some() { self.spill(s) } else { s };
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
                    if let Some(stop) = stop {
                        let stop = g.expr(stop);
                        g.buf.push(Stmt::new(
                            docvec![
                                arena,
                                "if (",
                                stop.at(arena, Before::Delimiter, After::Delimiter),
                                ") break;"
                            ],
                            stop.flags,
                        ));
                    }
                });
                let head = docvec![
                    arena,
                    "for (const ",
                    arena.alloc_str(&var),
                    " of ",
                    s.at(arena, Before::Other, After::Delimiter),
                    ") "
                ];
                let flags = s.flags | stmt_flags(&inner) | MULTILINE;
                let loop_ = Stmt::new(docvec![arena, head, block(arena, &inner)], flags);
                let loop_ = if let Some(stop) = stop {
                    let stop = self.expr(stop);
                    let test = Js::code(
                        docvec![
                            arena,
                            "!(",
                            stop.at(arena, Before::Delimiter, After::Delimiter),
                            ")"
                        ],
                        super::doc::Head::Other,
                        &[stop],
                    );
                    self.if_then(test, &[loop_])
                } else {
                    loop_
                };
                self.buf.push(loop_);
            }
            ExprKind::Yield(value) => self.yielded(value),
            ExprKind::If(c, t, f) => {
                let c = self.expr(c);
                let (ts, ()) = self.block(|g| g.discard(t));
                let (fs, ()) = self.block(|g| g.discard(f));
                let stmt = if fs.is_empty() {
                    self.if_then(c, &ts)
                } else {
                    self.if_else(c, &ts, &fs)
                };
                self.buf.push(stmt);
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
                if v.atom().is_none() {
                    self.buf.push(self.expr_stmt(v));
                }
            }
        }
    }
    fn yielded(&mut self, value: &Expr) {
        let arena = self.a;
        let v = self.expr(value);
        let stmt = if let Some(out) = &self.collector {
            let push = self.leaf(&format!("{out}.push"));
            self.expr_stmt(call(arena, push, &[v]))
        } else {
            let yielded = v.at(arena, Before::Other, After::Semicolon);
            let doc = docvec![arena, "yield ", yielded, ";"];
            Stmt::new(doc, v.flags | YIELD_TEXT | YIELDS)
        };
        self.buf.push(stmt);
        if let Some((n, count)) = &self.limit {
            let n = n.atom().unwrap_or_default();
            let stmt = self.line(&format!("if (++{count} >= {n}) return;"));
            self.buf.push(stmt);
        }
    }
}
