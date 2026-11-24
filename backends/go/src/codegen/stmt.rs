//! Statements and loop expressions.

use std::mem;

use flick_backend_support::loops;
use flick_backend_support::tail::{Callee, Target};
use flick_ir::{Expr, ExprKind, Pat, Stmt};
use flick_pretty::docvec;

use super::closures::poly;
use super::layout::{Code, block, single};
use super::{Gen, names};

impl<'a> Gen<'a> {
    pub(super) fn loop_expr(&mut self, e: &Expr) -> Code<'a> {
        let a = self.a;
        match &e.kind {
            ExprKind::ControlLoop {
                id,
                condition,
                body,
                iterable,
            } => {
                let lowered = loops::lower(
                    self.input.lang,
                    id,
                    condition.as_deref(),
                    body,
                    *iterable,
                    e.span,
                );
                self.expr(&lowered)
            }
            ExprKind::LoopExit { .. } => unreachable!("loop exits are lowered with their loop"),
            ExprKind::Loop { condition, body } => {
                let (stmts, ()) = self.block(|g| {
                    let c = g.expr_bool(condition).doc;
                    g.emit(docvec![a, "if !", c, " ", single(a, "break")]);
                    g.discard(body);
                });
                self.emit(docvec![a, "for ", block(a, stmts)]);
                self.unit()
            }
            _ => unreachable!("loop expression"),
        }
    }

    pub(super) fn stmt(&mut self, stmt: &Stmt) {
        let a = self.a;
        let (Stmt::Let { value: e, .. }
        | Stmt::Var { value: e, .. }
        | Stmt::Assign(_, e)
        | Stmt::Expr(e)
        | Stmt::Fun { value: e, .. }) = stmt;
        self.line(e.span);
        match stmt {
            Stmt::Let {
                pat: Pat::Bind(name),
                value,
                ..
            } => {
                let v = self.expr_typed(value);
                self.bind_typed(name, v.code, v.ty);
            }
            Stmt::Let {
                pat: Pat::Wild,
                value,
                ..
            } => {
                let v = self.expr_typed(value).code;
                self.emit(docvec![a, "_ = ", v]);
            }
            Stmt::Var { name, ty, value } => {
                let target = self.go_type_of(ty.as_deref().or(value.ty.as_deref()));
                if let Some(gt) = target {
                    let v = self.expr_into(value, Some(&gt));
                    self.bind_typed(name, v, Some(gt));
                } else {
                    let v = self.expr_typed(value);
                    self.bind_typed(name, v.code, v.ty);
                }
            }
            Stmt::Assign(name, value) => {
                let want = self.go_types.get(name).cloned();
                let v = self.expr_into(value, want.as_ref());
                let l = a.alloc_str(&names::local(name));
                self.emit(docvec![a, l, " = ", v]);
            }
            Stmt::Let { pat, value, .. } => {
                let v = self.expr_typed(value);
                let v = self.spill_typed(v);
                self.destructure(pat, v.code.doc, v.ty);
            }
            Stmt::Fun {
                name,
                rec,
                dicts,
                value,
                ..
            } => {
                self.go_types.remove(name);
                let l = a.alloc_str(&names::local(name));
                // declared first: a recursive function refers to itself
                self.emit(docvec![a, "var ", l, " flickrt.Value"]);
                self.emit(docvec![a, "_ = ", l]);
                let mark = self.givens.len();
                let ds = self.bind_givens(dicts);
                let target = rec.then(|| Target {
                    callee: Callee::Local(name.clone()),
                    dicts: dicts.clone(),
                });
                let saved = (!ds.is_empty()).then(|| {
                    (
                        self.tail.take(),
                        mem::replace(&mut self.generator, false),
                        self.fiber.take(),
                        self.context.replace("fb"),
                        self.returns.take(),
                    )
                });
                let (stmts, v) = self.block(|g| match &value.kind {
                    ExprKind::Lambda { params, body } => g.lambda(params, body, target),
                    _ => g.expr(value),
                });
                if let Some(saved) = saved {
                    (
                        self.tail,
                        self.generator,
                        self.fiber,
                        self.context,
                        self.returns,
                    ) = saved;
                }
                self.givens.truncate(mark);
                if ds.is_empty() {
                    self.buf.extend(stmts);
                    self.emit(docvec![a, l, " = ", v]);
                } else {
                    self.emit(docvec![a, l, " = ", poly(a, &ds, stmts, v)]);
                }
            }
            Stmt::Expr(e) => {
                let v = self.expr_typed(e).code;
                // a result read by nothing: Go refuses a variable nobody reads
                self.emit(docvec![a, "_ = ", v]);
            }
        }
    }
}
