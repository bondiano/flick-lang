//! Expressions and statements. A Go expression is returned; statements it
//! needs go into the current statement list before it.

use std::mem;

use super::{Gen, indent, names};
use crate::codegen::tail::{Callee, Target};
use crate::ir::{Arm, BinOp, Expr, ExprKind, Lit, Pat, Stage, Stmt};
use crate::numeric::{Arith, Num};
use crate::types::Lift;

/// Whether moving the evaluation of `go` later is unobservable: a name or
/// a literal.
pub(super) fn atomic(go: &str) -> bool {
    let name = go
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && go
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
    name || go.starts_with('"') && !go[1..].contains('"')
        || go.starts_with("int32(")
        || go.starts_with("float64(")
}

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

impl Gen<'_> {
    /// Compiles expressions that are evaluated in order: an operand whose
    /// code needs statements makes the operands before it variables first.
    pub(super) fn ordered(&mut self, exprs: &[&Expr]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for e in exprs {
            let mark = self.buf.len();
            let go = self.expr(e);
            if self.buf.len() > mark {
                let mut spills = Vec::new();
                for prev in &mut out {
                    if !atomic(prev) {
                        let t = self.temp();
                        spills.push(format!("var {t} flickrt.Value = {prev}"));
                        *prev = t;
                    }
                }
                self.buf.splice(mark..mark, spills);
            }
            out.push(go);
        }
        out
    }

    /// `go` as a variable unless it is atomic.
    pub(super) fn spill(&mut self, go: String) -> String {
        if atomic(&go) {
            return go;
        }
        let t = self.temp();
        self.buf.push(format!("var {t} flickrt.Value = {go}"));
        t
    }

    /// Declares the local `name` as `value`.
    pub(super) fn bind(&mut self, name: &str, value: &str) {
        let l = names::local(name);
        self.buf
            .push(format!("var {l} flickrt.Value = {value}\n_ = {l}"));
    }

    pub(super) fn expr(&mut self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Lit(lit) => literal(lit),
            ExprKind::Local(name) => names::local(name),
            ExprKind::Global { module, name } => self.global_value(module, name, Vec::new()),
            ExprKind::Method { dict, name } => {
                format!(
                    "flickrt.Method({}, {})",
                    self.dict(dict),
                    names::string(name)
                )
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
                self.buf.push(format!("var {r} flickrt.Value"));
                self.arms(&s, arms, Some(&r));
                r
            }
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                let items: Vec<&Expr> = items.iter().collect();
                format!("[]flickrt.Value{{{}}}", self.ordered(&items).join(", "))
            }
            ExprKind::Record(fields) => self.record(None, fields),
            ExprKind::Update(base, fields) => self.record(Some(base), fields),
            ExprKind::Field(record, label) => {
                let r = self.expr(record);
                format!("flickrt.Field({r}, {})", names::string(label))
            }
            ExprKind::Binary(op, lhs, rhs) => self.binary(*op, lhs, rhs),
            ExprKind::Not(inner) => format!("flickrt.Not({})", self.expr(inner)),
            ExprKind::Unwrap(inner) => self.unwrap(inner),
            ExprKind::Neg(num, inner) => {
                let v = self.expr(inner);
                let f = match num {
                    Num::Int => "NegI",
                    Num::Float => "NegF",
                    Num::BigInt => "NegB",
                };
                format!("flickrt.{f}({v})")
            }
            ExprKind::Ignore(inner) => {
                let v = self.expr(inner);
                // a result read by nothing: Go refuses a variable nobody reads
                self.buf.push(format!("_ = {v}"));
                "flickrt.Unit".to_owned()
            }
            ExprKind::Lift(inner, how) => {
                let v = self.expr(inner);
                format!("{}({v})", lift(how))
            }
            ExprKind::Each { .. } | ExprKind::Yield(_) => {
                self.discard(e);
                "flickrt.Unit".to_owned()
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
            } => operation(service, name, stages),
            ExprKind::ServiceTag(key) => names::string(key),
            ExprKind::Label(_) => self.unsupported(e.span, "a record label"),
            // outside effectful code nothing can fail with a typed error
            ExprKind::Try(body, _) if self.fiber.is_none() => self.expr(body),
            ExprKind::Try(body, arms) => self.try_expr(body, arms),
            ExprKind::Await(task) => {
                let t = self.expr(task);
                self.run(&t)
            }
            ExprKind::Task(body) => self.task(body),
            ExprKind::Scope { owner, outer, body } => {
                self.scope(owner.as_deref(), outer.as_deref(), body)
            }
            ExprKind::Within(owner, call) => self.within(owner, call),
            ExprKind::Provide(layer, body) => self.provide(layer, body),
            ExprKind::CodecBlock(..) => self.unsupported(e.span, "a `codec` block"),
        }
    }

    fn if_expr(&mut self, cond: &Expr, then: &Expr, other: &Expr) -> String {
        let cond = self.expr(cond);
        let (then_stmts, then_value) = self.block(|g| g.expr(then));
        let (other_stmts, other_value) = self.block(|g| g.expr(other));
        let r = self.temp();
        self.buf.push(format!(
            "var {r} flickrt.Value\nif flickrt.Bool({cond}) {{\n{}\t{r} = {then_value}\n}} else {{\n{}\t{r} = {other_value}\n}}",
            indent(&then_stmts),
            indent(&other_stmts)
        ));
        r
    }

    /// A task: a Go function of the fiber that runs `body` in it.
    pub(super) fn task(&mut self, body: &Expr) -> String {
        format!(
            "flickrt.Eff(func(fb *flickrt.Fiber) flickrt.Value {{\n{}}})",
            self.effectful_body(body)
        )
    }

    /// The statements of a Go function of a fiber `fb` that returns `body`.
    fn effectful_body(&mut self, body: &Expr) -> String {
        let saved = (
            self.tail.take(),
            mem::replace(&mut self.generator, false),
            self.fiber.replace("fb"),
        );
        let (stmts, ()) = self.block(|g| g.tail_expr(body));
        (self.tail, self.generator, self.fiber) = saved;
        indent(&stmts)
    }

    /// `scope [owner] { body }`: the body runs as the owner of a fresh
    /// region, given it and the owner around it.
    fn scope(&mut self, owner: Option<&str>, outer: Option<&str>, body: &Expr) -> String {
        let fb = self.fiber.expect("a `scope` runs where a fiber is in scope");
        let name = |n: Option<&str>| n.map_or_else(|| "_".to_owned(), names::local);
        format!(
            "flickrt.Scope({fb}, func(fb *flickrt.Fiber, {}, {} flickrt.Value) flickrt.Value {{\n{}}})",
            name(owner),
            name(outer),
            self.effectful_body(body)
        )
    }

    /// A call that acquires into the region of `owner`: run in a fiber
    /// whose owner it is.
    fn within(&mut self, owner: &str, call: &Expr) -> String {
        let fb = self.fiber.expect("`Within` runs where a fiber is in scope");
        let saved = self.fiber.replace("fbw");
        let (stmts, v) = self.block(|g| g.expr(call));
        self.fiber = saved;
        let r = self.temp();
        self.buf.push(format!(
            "var {r} flickrt.Value\n{{\n\tfbw := flickrt.Within({fb}, {})\n{}\t{r} = {v}\n}}",
            names::local(owner),
            indent(&stmts)
        ));
        r
    }

    /// `provide layer body`: the body as a task, run with the layer's services.
    fn provide(&mut self, layer: &Expr, body: &Expr) -> String {
        let layer = self.expr(layer);
        let layer = self.spill(layer);
        let body = self.task(body);
        let fb = self.fiber.expect("`provide` runs where a fiber is in scope");
        format!("flickrt.Provide({fb}, {layer}, {body})")
    }

    /// `try body with arms`: the body as a task, and a handler of its typed
    /// error that gives `flickrt.Unmatched` when no arm matches.
    fn try_expr(&mut self, body: &Expr, arms: &[Arm]) -> String {
        let body = self.task(body);
        let saved = (
            self.tail.take(),
            mem::replace(&mut self.generator, false),
            self.fiber.replace("fb"),
        );
        let err = self.temp();
        let (mut stmts, ()) = self.block(|g| {
            for arm in arms {
                g.arm(&err, &arm.pat, arm.guard.as_ref(), &arm.body, |g, value| {
                    let v = g.expr(value);
                    g.buf.push(format!("return {v}"));
                });
            }
        });
        (self.tail, self.generator, self.fiber) = saved;
        stmts.push("return flickrt.Unmatched".to_owned());
        let fb = self.fiber.expect("a `try` with arms runs in a fiber");
        format!(
            "flickrt.Try({fb}, {body}, func(fb *flickrt.Fiber, {err} flickrt.Value) flickrt.Value {{\n{}}})",
            indent(&stmts)
        )
    }

    /// `o?`: the value of the `Some`, after returning a `None` at once.
    fn unwrap(&mut self, option: &Expr) -> String {
        let v = self.expr(option);
        let v = self.spill(v);
        // in a lazy `for` a `None` ends the sequence, as a JS generator's return does
        let ret = if self.generator {
            String::new()
        } else {
            format!(" {v}")
        };
        self.buf.push(format!(
            "if flickrt.IsTag({v}, \"None\") {{\n\treturn{ret}\n}}"
        ));
        format!("flickrt.Get({v}, 0)")
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
                self.buf.push(format!(
                    "if flickrt.Bool({cond}) {{\n{}}} else {{\n{}}}",
                    indent(&then),
                    indent(&other)
                ));
            }
            ExprKind::Match(scrutinee, arms) => {
                let s = self.expr(scrutinee);
                let s = self.spill(s);
                self.arms(&s, arms, None);
            }
            ExprKind::Binary(op @ (BinOp::And | BinOp::Or), lhs, rhs) if self.jumps_in(rhs) => {
                self.tail_logic(*op, lhs, rhs);
            }
            _ if self.jump(e) => {}
            _ => {
                self.line(e.span);
                let v = self.expr(e);
                self.buf.push(format!("return {v}"));
            }
        }
    }

    fn record(&mut self, base: Option<&Expr>, fields: &[(String, Expr)]) -> String {
        let mut all: Vec<&Expr> = base.into_iter().collect();
        all.extend(fields.iter().map(|(_, v)| v));
        let mut go = self.ordered(&all);
        let base = base.map(|_| go.remove(0));
        let entries: Vec<String> = fields
            .iter()
            .zip(go)
            .map(|((label, _), v)| format!("{}, {v}", names::string(label)))
            .collect();
        let record = format!("flickrt.Rec({})", entries.join(", "));
        match base {
            Some(b) => format!("flickrt.Update({b}, {record})"),
            None => record,
        }
    }

    fn binary(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr) -> String {
        if matches!(op, BinOp::And | BinOp::Or) {
            let l = self.expr(lhs);
            let (rs, rv) = self.block(|g| g.expr(rhs));
            let go = if op == BinOp::And { "&&" } else { "||" };
            if rs.is_empty() {
                return format!("(flickrt.Bool({l}) {go} flickrt.Bool({rv}))");
            }
            let t = self.temp();
            let test = if op == BinOp::And {
                format!("flickrt.Bool({t})")
            } else {
                format!("!flickrt.Bool({t})")
            };
            self.buf.push(format!(
                "var {t} flickrt.Value = {l}\nif {test} {{\n{}\t{t} = {rv}\n}}",
                indent(&rs)
            ));
            return t;
        }
        let go = self.ordered(&[lhs, rhs]);
        let (l, r) = (&go[0], &go[1]);
        let f = match op {
            BinOp::Eq => "Equals",
            BinOp::NotEq => return format!("!flickrt.Equals({l}, {r})"),
            BinOp::Same => "Same",
            BinOp::NotSame => return format!("!flickrt.Same({l}, {r})"),
            BinOp::Lt => "Lt",
            BinOp::Le => "Le",
            BinOp::Gt => "Gt",
            BinOp::Ge => "Ge",
            BinOp::Concat => "Concat",
            BinOp::ListConcat => "ListConcat",
            BinOp::Arith(arith, num) => {
                let suffix = match num {
                    Num::Int => "I",
                    Num::Float => "F",
                    Num::BigInt => "B",
                };
                let name = match arith {
                    Arith::Add => "Add",
                    Arith::Sub => "Sub",
                    Arith::Mul => "Mul",
                    Arith::Div => "Div",
                    Arith::Rem => "Rem",
                    Arith::Pow => "Pow",
                };
                return format!("flickrt.{name}{suffix}({l}, {r})");
            }
            BinOp::And | BinOp::Or => unreachable!("handled above"),
        };
        format!("flickrt.{f}({l}, {r})")
    }

    pub(super) fn stmt(&mut self, stmt: &Stmt) {
        let (Stmt::Let(_, e) | Stmt::Expr(e) | Stmt::Fun { value: e, .. }) = stmt;
        self.line(e.span);
        match stmt {
            Stmt::Let(Pat::Bind(name), value) => {
                let v = self.expr(value);
                self.bind(name, &v);
            }
            Stmt::Let(Pat::Wild, value) => {
                let v = self.expr(value);
                self.buf.push(format!("_ = {v}"));
            }
            Stmt::Let(pat, value) => {
                let v = self.expr(value);
                let v = self.spill(v);
                self.destructure(pat, &v);
            }
            Stmt::Fun {
                name,
                rec,
                dicts,
                value,
            } => {
                let l = names::local(name);
                // declared first: a recursive function refers to itself
                self.buf.push(format!("var {l} flickrt.Value\n_ = {l}"));
                let mark = self.givens.len();
                let ds = self.bind_givens(dicts);
                let target = rec.then(|| Target {
                    callee: Callee::Local(name.clone()),
                    dicts: dicts.clone(),
                });
                let (mut stmts, v) = self.block(|g| match &value.kind {
                    ExprKind::Lambda { params, body } => g.lambda(params, body, target),
                    _ => g.expr(value),
                });
                self.givens.truncate(mark);
                if ds.is_empty() {
                    self.buf.extend(stmts);
                    self.buf.push(format!("{l} = {v}"));
                } else {
                    let takes: Vec<String> = ds
                        .iter()
                        .enumerate()
                        .map(|(i, d)| format!("{d} := ds[{i}]\n_ = {d}"))
                        .collect();
                    stmts.splice(0..0, takes);
                    stmts.push(format!("return {v}"));
                    self.buf.push(format!(
                        "{l} = flickrt.Poly(func(ds ...flickrt.Value) flickrt.Value {{\n{}}})",
                        indent(&stmts)
                    ));
                }
            }
            Stmt::Expr(e) => {
                let v = self.expr(e);
                // a result read by nothing: Go refuses a variable nobody reads
                self.buf.push(format!("_ = {v}"));
            }
        }
    }
}

/// `S.op`: a function of its arguments up to its first effectful stage.
fn operation(service: &str, name: &str, stages: &[Stage]) -> String {
    let arity = stages
        .iter()
        .position(|&s| s != Stage::Pure)
        .map_or(stages.len(), |i| i + 1);
    format!(
        "flickrt.Operation({}, {}, {arity})",
        names::string(service),
        names::string(name)
    )
}
