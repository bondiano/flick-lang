//! Expressions and statements. A Go expression is returned; statements it
//! needs go into the current statement list before it.

use std::mem;

use crate::codegen::loops;

use flick_pretty::{Document, docvec};

use super::arith::Native;
use super::closures::poly;
use super::layout::{Code, call, if_else, list, statement};
use super::{Gen, names};
use crate::codegen::tail::{Callee, Target};
use crate::ir::{BinOp, Expr, ExprKind, Lit, Pat, Stage, Stmt};
use flick_ir::{EnumLit, Lift};

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

/// The literal an enum's constructor is, as Go holds it.
pub(super) fn enum_literal(literal: &EnumLit) -> String {
    match literal {
        EnumLit::Int(n) => self::literal(&Lit::Int(*n)),
        EnumLit::String(s) => self::literal(&Lit::String(s.clone())),
    }
}

/// A Go function that adapts a function of one stage where one of another
/// is expected, or the result of a task.
pub(super) fn lift(how: &Lift) -> String {
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
    /// Go orders the calls of an expression but not the reads of variables
    /// between them: an operand is a variable of its own, evaluated first,
    /// when it may run code and a later one reads a `let mut` variable, or
    /// it reads one and a later one may run code.
    pub(super) fn ordered(&mut self, exprs: &[&Expr]) -> Vec<Code<'a>> {
        let a = self.a;
        let mut out: Vec<Code<'a>> = Vec::new();
        let reads: Vec<bool> = exprs.iter().map(|e| reads_variable(e)).collect();
        for (i, e) in exprs.iter().enumerate() {
            let mark = self.buf.len();
            let mut go = self.expr(e);
            let later = (i + 1..exprs.len())
                .any(|j| (!plain(e) && reads[j]) || (reads[i] && !plain(exprs[j])));
            if later && !go.atomic {
                let t = self.temp();
                let ty = self.native(e).map_or("flickrt.Value", Native::go);
                self.emit(docvec![a, "var ", t, " ", ty, " = ", go]);
                go = self.code(t);
            }
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

    fn loop_expr(&mut self, e: &Expr) -> Code<'a> {
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
                    let c = g.expr(condition);
                    let c = g.unboxed(condition, c, Native::Bool).doc;
                    g.emit(docvec![
                        a,
                        "if !",
                        c,
                        " ",
                        super::layout::single(a, "break")
                    ]);
                    g.discard(body);
                });
                self.emit(docvec![a, "for ", super::layout::block(a, stmts)]);
                self.unit()
            }
            _ => unreachable!("loop expression"),
        }
    }

    pub(super) fn expr(&mut self, e: &Expr) -> Code<'a> {
        let a = self.a;
        match &e.kind {
            ExprKind::ControlLoop { .. } | ExprKind::LoopExit { .. } | ExprKind::Loop { .. } => {
                self.loop_expr(e)
            }
            ExprKind::Lit(lit) => self.lit(lit),
            ExprKind::Local(name) => self.code(&names::local(name)),
            // not atomic: an operand after it that assigns it is ordered after the read
            ExprKind::Var(name) => Code::from(a.text(a.alloc_str(&names::local(name)))),
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
            ExprKind::If(cond, then, other) => self.if_expr(e, cond, then, other),
            ExprKind::Match(scrutinee, arms) => {
                let s = self.expr(scrutinee);
                let s = self.spill(s);
                let r = self.result(e);
                self.arms(s.doc, arms, Some(r));
                self.code(r)
            }
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                let items: Vec<&Expr> = items.iter().collect();
                let items = self.ordered(&items).into_iter().map(|item| item.doc);
                list(a, "[]flickrt.Value{", items, "}").into()
            }
            ExprKind::Record(fields) => self.record(None, fields),
            ExprKind::Dictionary(fields) => {
                let all: Vec<&Expr> = fields.iter().map(|(_, value)| value).collect();
                let values = self.ordered(&all);
                call(self.a, "flickrt.DictOf", self.entries(fields, values)).into()
            }
            ExprKind::Update(base, fields) => self.record(Some(base), fields),
            ExprKind::Field(record, label) => self.field(record, label),
            ExprKind::Binary(op, lhs, rhs) => self.binary(*op, lhs, rhs),
            ExprKind::Not(inner) => {
                let v = self.expr(inner);
                docvec![a, "!", self.unboxed(inner, v, Native::Bool).doc].into()
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
                call(a, self.once(&lift(how)), [v.doc]).into()
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
            ExprKind::Perform {
                effect: service,
                op: name,
                stages,
            } => self.operation(service, name, stages),
            ExprKind::ServiceTag(key) => self.code(&names::string(key)),
            ExprKind::Label(label) => self.code(&names::string(label)),
            // outside effectful code nothing can fail with a typed error
            ExprKind::Handle(h) if h.is_try() && self.fiber.is_none() => self.expr(&h.body),
            ExprKind::Handle(h) if h.is_try() => self.try_expr(&h.body, &h.errors),
            ExprKind::Await(task) => {
                let t = self.expr(task);
                self.run(t)
            }
            ExprKind::Task(body) => self.task(body),
            ExprKind::Within(owner, inner) => self.within(owner, inner),
            ExprKind::Handle(h) => self.handle(h),
        }
    }

    /// Declares the variable of the result of `e`, an `if` or a `match`:
    /// of the Go type every branch has, when they have one, so that a number
    /// is not made a value on the way from a branch to what uses it.
    fn result(&mut self, e: &Expr) -> &'a str {
        let r = self.temp();
        let ty = self.native(e).map_or("flickrt.Value", Native::go);
        self.emit(docvec![self.a, "var ", r, " ", ty]);
        r
    }

    fn if_expr(&mut self, e: &Expr, cond: &Expr, then: &Expr, other: &Expr) -> Code<'a> {
        let a = self.a;
        let test = self.expr(cond);
        let test = self.unboxed(cond, test, Native::Bool).doc;
        // the branches are compiled after the variable is declared, and
        // nothing they bind is a branch itself: the type is known before
        let r = self.result(e);
        let (mut then_stmts, then_value) = self.block(|g| g.expr(then));
        let (mut other_stmts, other_value) = self.block(|g| g.expr(other));
        then_stmts.push(statement(a, docvec![a, r, " = ", then_value]));
        other_stmts.push(statement(a, docvec![a, r, " = ", other_value]));
        self.emit(if_else(a, test, then_stmts, other_stmts));
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
                let test = self.expr(cond);
                let test = self.unboxed(cond, test, Native::Bool).doc;
                let (then, ()) = self.block(|g| g.tail_expr(then));
                let (other, ()) = self.block(|g| g.tail_expr(other));
                self.emit(if_else(self.a, test, then, other));
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
                // in the Go type the function returns, when it has one
                let v = match self.returns {
                    Some(native) => self.unboxed(e, v, native),
                    None => v,
                };
                self.emit(docvec![self.a, "return ", v]);
            }
        }
    }

    /// Each label, then its value, as one item of a list.
    pub(super) fn entries(
        &self,
        fields: &[(String, Expr)],
        values: Vec<Code<'a>>,
    ) -> Vec<Document<'a>> {
        fields
            .iter()
            .zip(values)
            .map(|((label, _), v)| docvec![self.a, self.string(label), ", ", v])
            .collect()
    }

    pub(super) fn stmt(&mut self, stmt: &Stmt) {
        let a = self.a;
        let (Stmt::Let(_, e)
        | Stmt::Var(_, e)
        | Stmt::Assign(_, e)
        | Stmt::Expr(e)
        | Stmt::Fun { value: e, .. }) = stmt;
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
            Stmt::Var(name, value) => {
                let v = self.expr(value);
                // a variable has one type, so a scalar one is unboxed where it is
                // declared and where it is assigned, not boxed on every assignment
                match self
                    .native(value)
                    .or_else(|| Native::of(self.input.lang, value.ty.as_deref()))
                {
                    Some(n) => {
                        let v = self.unboxed(value, v, n);
                        self.bind_native(name, v, n);
                    }
                    None => self.bind(name, v),
                }
            }
            Stmt::Assign(name, value) => {
                let v = self.expr(value);
                let v = match self.natives.get(name).copied() {
                    Some(n) => self.unboxed(value, v, n),
                    None => v,
                };
                let l = a.alloc_str(&names::local(name));
                self.emit(docvec![a, l, " = ", v]);
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

/// A literal or a name: evaluating it runs no code.
fn plain(e: &Expr) -> bool {
    matches!(
        e.kind,
        ExprKind::Lit(_) | ExprKind::Local(_) | ExprKind::Var(_) | ExprKind::Con(_)
    )
}

/// Whether `e` reads a `let mut` variable where it is evaluated.
fn reads_variable(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Var(_) => true,
        ExprKind::Lambda { .. } | ExprKind::Task(_) => false,
        _ => {
            let mut found = false;
            e.each_child(&mut |c| found |= reads_variable(c));
            found
        }
    }
}
