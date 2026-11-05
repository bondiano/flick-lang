//! Expressions and statements. A Go expression is returned; statements it
//! needs go into the current statement list before it.

use std::mem;

use flick_pretty::{Document, docvec};

use super::arith::Native;
use super::call::poly;
use super::layout::{Code, call, if_else, list, statement};
use super::{Gen, names};
use crate::codegen::tail::{Callee, Target};
use crate::ir::{BinOp, Expr, ExprKind, Lit, Pat, Stage, Stmt};
use crate::types::Lift;

/// A literal as Go: numbers typed as the runtime holds them.
pub(super) fn literal(lit: &Lit) -> String {
    match lit {
        Lit::Int(n) => format!("int32({n})"),
        Lit::Float(x) if x.is_nan() => "flickrt.NaN".to_owned(),
        Lit::Float(x) if x.is_infinite() && *x > 0.0 => "flickrt.Inf".to_owned(),
        Lit::Float(x) if x.is_infinite() => "flickrt.NegInf".to_owned(),
        Lit::Float(x) if *x == 0.0 && x.is_sign_negative() => "flickrt.NegZero".to_owned(),
        // Rust's shortest digits read back to the same float in Go
        Lit::Float(x) => format!("float64({x:e})"),
        Lit::BigInt(n) => format!("flickrt.BigLit({})", names::string(n)),
        Lit::String(s) => names::string(s),
        Lit::Bool(b) => b.to_string(),
        Lit::Unit => "flickrt.Unit".to_owned(),
    }
}

/// A Go function that adapts a function of one stage where one of another
/// is expected, or the result of a task.
fn lift(how: &Lift) -> String {
    match how {
        Lift::Stage { from, to, result } => format!(
            "flickrt.LiftStage({}, {}, {})",
            *from != Stage::Pure,
            *to != Stage::Pure,
            result.as_deref().map_or_else(|| "nil".to_owned(), lift)
        ),
        Lift::Eff(inner) => format!("flickrt.LiftEff({})", lift(inner)),
    }
}

impl<'a> Gen<'a> {
    /// Compiles expressions that are evaluated in order: an operand whose
    /// code needs statements makes the operands before it variables first.
    pub(super) fn ordered(&mut self, exprs: &[&Expr]) -> Vec<Code<'a>> {
        let a = self.a;
        let mut out: Vec<Code<'a>> = Vec::new();
        for e in exprs {
            let mark = self.buf.len();
            let go = self.expr(e);
            if self.buf.len() > mark {
                let mut spills = Vec::new();
                for (prev, before) in out.iter_mut().zip(exprs) {
                    if !prev.atomic {
                        let t = self.temp();
                        let ty = self.native(before).map_or("flickrt.Value", Native::go);
                        spills.push(statement(a, docvec![a, "var ", t, " ", ty, " = ", *prev]));
                        *prev = self.code(t);
                    }
                }
                self.buf.splice(mark..mark, spills);
            }
            out.push(go);
        }
        out
    }

    /// `go` as a variable unless it is atomic.
    pub(super) fn spill(&mut self, go: Code<'a>) -> Code<'a> {
        if go.atomic {
            return go;
        }
        let t = self.temp();
        self.emit(docvec![self.a, "var ", t, " flickrt.Value = ", go]);
        self.code(t)
    }

    /// Declares the local `name` as `value`, Go code of type `native`.
    pub(super) fn bind_native(&mut self, name: &str, value: Code<'a>, native: Native) {
        let l = self.a.alloc_str(&names::local(name));
        self.emit(docvec![self.a, "var ", l, " ", native.go(), " = ", value]);
        self.emit(docvec![self.a, "_ = ", l]);
        self.natives.insert(name.to_owned(), native);
    }

    /// Declares the local `name` as `value`.
    pub(super) fn bind(&mut self, name: &str, value: Code<'a>) {
        self.natives.remove(name);
        let l = self.a.alloc_str(&names::local(name));
        self.emit(docvec![self.a, "var ", l, " flickrt.Value = ", value]);
        self.emit(docvec![self.a, "_ = ", l]);
    }

    pub(super) fn expr(&mut self, e: &Expr) -> Code<'a> {
        let a = self.a;
        match &e.kind {
            ExprKind::Lit(lit) => self.code(&literal(lit)),
            ExprKind::Local(name) => self.code(&names::local(name)),
            ExprKind::Global { module, name } => self.global_value(module, name, Vec::new()),
            ExprKind::Method { dict, name } => {
                let dict = self.dict(dict);
                call(
                    a,
                    "flickrt.Method",
                    [self.context(), dict.doc, self.string(name)],
                )
                .into()
            }
            ExprKind::Con(_) | ExprKind::App { .. } => self.app(e),
            ExprKind::Dicts(inner, dicts) => self.given(inner, dicts),
            ExprKind::Abstract(shown, inner) => self.abstraction(shown, inner),
            ExprKind::Lambda { params, body } => self.lambda(params, body, None),
            ExprKind::Block(stmts, result) => {
                for stmt in stmts {
                    self.stmt(stmt);
                }
                self.expr(result)
            }
            ExprKind::If(cond, then, other) => self.if_expr(cond, then, other),
            ExprKind::Match(scrutinee, arms) => {
                let s = self.expr(scrutinee);
                let s = self.spill(s);
                let r = self.temp();
                self.emit(docvec![a, "var ", r, " flickrt.Value"]);
                self.arms(s.doc, arms, Some(r));
                self.code(r)
            }
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                let items: Vec<&Expr> = items.iter().collect();
                let items = self.ordered(&items).into_iter().map(|item| item.doc);
                list(a, "[]flickrt.Value{", items, "}").into()
            }
            ExprKind::Record(fields) => self.record(None, fields),
            ExprKind::Update(base, fields) => self.record(Some(base), fields),
            ExprKind::Field(record, label) => {
                let r = self.expr(record);
                call(a, "flickrt.Field", [r.doc, self.string(label)]).into()
            }
            ExprKind::Binary(op, lhs, rhs) => self.binary(*op, lhs, rhs),
            ExprKind::Not(inner) => {
                let v = self.expr(inner);
                call(a, "flickrt.Not", [v.doc]).into()
            }
            ExprKind::Unwrap(inner) => self.unwrap(inner),
            ExprKind::Neg(num, inner) => self.neg(*num, inner),
            ExprKind::Ignore(inner) => {
                let v = self.expr(inner);
                // a result read by nothing: Go refuses a variable nobody reads
                self.emit(docvec![a, "_ = ", v]);
                self.unit()
            }
            ExprKind::Lift(inner, how) => {
                let v = self.expr(inner);
                call(a, lift(how), [v.doc]).into()
            }
            ExprKind::Each { .. } | ExprKind::Yield(_) => {
                self.discard(e);
                self.unit()
            }
            ExprKind::Generate {
                body,
                strict: false,
                sync: false,
                limit,
            } => self.lazy(body, limit.as_deref()),
            ExprKind::Generate { strict: false, .. } => {
                self.unsupported(e.span, "a lazy `for` that reads services")
            }
            ExprKind::Generate { body, .. } => self.strict(body),
            ExprKind::Operation {
                service,
                name,
                stages,
            } => self.operation(service, name, stages),
            ExprKind::ServiceTag(key) => self.code(&names::string(key)),
            ExprKind::Label(label) => self.code(&names::string(label)),
            // outside effectful code nothing can fail with a typed error
            ExprKind::Try(body, _) if self.fiber.is_none() => self.expr(body),
            ExprKind::Try(body, arms) => self.try_expr(body, arms),
            ExprKind::Await(task) => {
                let t = self.expr(task);
                self.run(t)
            }
            ExprKind::Task(body) => self.task(body),
            ExprKind::Scope { owner, outer, body } => {
                self.scope(owner.as_deref(), outer.as_deref(), body)
            }
            ExprKind::Within(owner, inner) => self.within(owner, inner),
            ExprKind::Provide(layer, body) => self.provide(layer, body),
        }
    }

    fn if_expr(&mut self, cond: &Expr, then: &Expr, other: &Expr) -> Code<'a> {
        let a = self.a;
        let cond = self.expr(cond);
        let (mut then_stmts, then_value) = self.block(|g| g.expr(then));
        let (mut other_stmts, other_value) = self.block(|g| g.expr(other));
        let r = self.temp();
        then_stmts.push(statement(a, docvec![a, r, " = ", then_value]));
        other_stmts.push(statement(a, docvec![a, r, " = ", other_value]));
        self.emit(docvec![a, "var ", r, " flickrt.Value"]);
        self.emit(if_else(a, cond, then_stmts, other_stmts));
        self.code(r)
    }

    /// Emits `e` in tail position: its value is returned.
    pub(super) fn tail_expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Block(stmts, result) => {
                for stmt in stmts {
                    self.stmt(stmt);
                }
                self.tail_expr(result);
            }
            ExprKind::If(cond, then, other) => {
                let cond = self.expr(cond);
                let (then, ()) = self.block(|g| g.tail_expr(then));
                let (other, ()) = self.block(|g| g.tail_expr(other));
                self.emit(if_else(self.a, cond, then, other));
            }
            ExprKind::Match(scrutinee, arms) => {
                let s = self.expr(scrutinee);
                let s = self.spill(s);
                self.arms(s.doc, arms, None);
            }
            ExprKind::Binary(op @ (BinOp::And | BinOp::Or), lhs, rhs) if self.jumps_in(rhs) => {
                self.tail_logic(*op, lhs, rhs);
            }
            _ if self.jump(e) => {}
            _ => {
                self.line(e.span);
                let v = self.expr(e);
                self.emit(docvec![self.a, "return ", v]);
            }
        }
    }

    fn record(&mut self, base: Option<&Expr>, fields: &[(String, Expr)]) -> Code<'a> {
        let a = self.a;
        let mut all: Vec<&Expr> = base.into_iter().collect();
        all.extend(fields.iter().map(|(_, v)| v));
        let mut go = self.ordered(&all);
        let base = base.map(|_| go.remove(0));
        let record = call(a, "flickrt.Rec", self.entries(fields, go));
        match base {
            Some(b) => call(a, "flickrt.Update", [b.doc, record]).into(),
            None => record.into(),
        }
    }

    /// Each label, then its value, as one item of a list.
    fn entries(&self, fields: &[(String, Expr)], values: Vec<Code<'a>>) -> Vec<Document<'a>> {
        fields
            .iter()
            .zip(values)
            .map(|((label, _), v)| docvec![self.a, self.string(label), ", ", v])
            .collect()
    }

    pub(super) fn stmt(&mut self, stmt: &Stmt) {
        let a = self.a;
        let (Stmt::Let(_, e) | Stmt::Expr(e) | Stmt::Fun { value: e, .. }) = stmt;
        self.line(e.span);
        match stmt {
            Stmt::Let(Pat::Bind(name), value) => {
                let v = self.expr(value);
                match self.native(value) {
                    Some(n) => self.bind_native(name, v, n),
                    None => self.bind(name, v),
                }
            }
            Stmt::Let(Pat::Wild, value) => {
                let v = self.expr(value);
                self.emit(docvec![a, "_ = ", v]);
            }
            Stmt::Let(pat, value) => {
                let v = self.expr(value);
                let v = self.spill(v);
                self.destructure(pat, v.doc);
            }
            Stmt::Fun {
                name,
                rec,
                dicts,
                value,
            } => {
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
                    )
                });
                let (stmts, v) = self.block(|g| match &value.kind {
                    ExprKind::Lambda { params, body } => g.lambda(params, body, target),
                    _ => g.expr(value),
                });
                if let Some(saved) = saved {
                    (self.tail, self.generator, self.fiber, self.context) = saved;
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
                let v = self.expr(e);
                // a result read by nothing: Go refuses a variable nobody reads
                self.emit(docvec![a, "_ = ", v]);
            }
        }
    }
}
