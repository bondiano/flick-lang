//! Expressions, statements and functions.

use flick_backend_support::loops;
use flick_ir::{Expr as IrExpr, ExprKind as Ir, Lit, Param, Pat, Span, Stage, Stmt as IrStmt};

use super::super::ast::{Block, Expr, ExprKind, Field, Function, LastKind, Stmt, StmtKind, UnOp};
use super::super::names::Namer;
use super::{Dest, Lowering, RUNTIME};

impl Lowering<'_> {
    pub(super) fn expr(&mut self, e: &IrExpr) -> Expr {
        let span = e.span;
        match &e.kind {
            Ir::Lit(lit) => self.literal(lit, span),
            Ir::Local(local) | Ir::Var(local) => Expr::name(&self.names.local(local), span),
            Ir::Global { module, name, .. } => self.global(module, name, span),
            Ir::Method { dict, name, .. } => self.dict(dict, span).field(name),
            Ir::Con(c) => self.constructor(c, span),
            Ir::Label(label) => Expr::str(label, span),
            Ir::Dicts(inner, _) if matches!(inner.kind, Ir::Perform { .. }) => {
                self.unsupported(span, "an effect operation", RUNTIME)
            }
            Ir::Dicts(inner, dicts) => {
                let f = self.expr(inner);
                let dicts = dicts.iter().map(|d| self.dict(d, span)).collect();
                f.call(dicts)
            }
            Ir::Abstract(shown, inner) => self.dictionaries_of(shown, inner, span),
            Ir::App { .. } => self.app(e),
            Ir::Lambda { params, body } => self.lambda(params, body, span),
            Ir::Block(stmts, result) => {
                for stmt in stmts {
                    self.stmt(stmt);
                }
                self.expr(result)
            }
            Ir::If(..) | Ir::Match(..) => {
                let r = self.names.fresh("r");
                self.push(StmtKind::Local(r.clone(), None), span);
                self.to(e, &Dest::Into(r.clone()));
                Expr::name(&r, span)
            }
            Ir::Tuple(items) | Ir::List(items) => {
                let items: Vec<&IrExpr> = items.iter().collect();
                let values = self.ordered(&items);
                Expr::new(
                    ExprKind::Table(values.into_iter().map(Field::Item).collect()),
                    span,
                )
            }
            Ir::Record(fields) | Ir::Dictionary(fields) => self.record(fields, span),
            Ir::Update(base, fields) => self.update(base, fields, span),
            Ir::Field(record, label) => self.expr(record).field(label),
            Ir::Binary(op, lhs, rhs) => self.binary(*op, lhs, rhs, span),
            Ir::Not(inner) => {
                let inner = self.expr(inner);
                Expr::unary(UnOp::Not, inner, span)
            }
            Ir::Unwrap(inner) => self.unwrap(inner, span),
            Ir::Neg(num, inner) => self.neg(*num, inner, span),
            Ir::Ignore(inner) => {
                self.to(inner, &Dest::Drop);
                Self::unit(span)
            }
            Ir::Loop { condition, body } => {
                self.while_loop(condition, body, span);
                Self::unit(span)
            }
            Ir::ControlLoop {
                id,
                condition,
                body,
                iterable,
            } => {
                let lowered =
                    loops::lower(&self.known, id, condition.as_deref(), body, *iterable, span);
                self.expr(&lowered)
            }
            Ir::LoopExit { .. } => self.refuse(
                span,
                "malformed IR: a loop exit outside the loop it leaves".to_owned(),
            ),
            Ir::Perform { .. }
            | Ir::ServiceTag(_)
            | Ir::Await(_)
            | Ir::Task(_)
            | Ir::Within(..)
            | Ir::ReadOwnerEvidence
            | Ir::WithOwnerEvidence(..)
            | Ir::Handle(_)
            | Ir::Lift(..) => self.unsupported(span, "this effectful expression", RUNTIME),
            Ir::Generate { .. } | Ir::Each { .. } | Ir::Yield(_) => self.unsupported(
                span,
                "a generator (`for` in a list or a sequence)",
                "generators need the Lua runtime, which is not implemented",
            ),
        }
    }

    /// Lowers `e`, delivering its value to `dest`: branches deliver theirs
    /// directly, so an `if` in tail position returns from each branch.
    pub(super) fn to(&mut self, e: &IrExpr, dest: &Dest) {
        match &e.kind {
            Ir::Block(stmts, result) => {
                for stmt in stmts {
                    self.stmt(stmt);
                }
                self.to(result, dest);
            }
            Ir::If(c, t, f) => {
                let c = self.expr(c);
                let then = self.within(|g| g.to(t, dest));
                let otherwise = self.within(|g| g.to(f, dest));
                let otherwise = (!otherwise.is_empty()).then_some(otherwise);
                self.push(StmtKind::If(vec![(c, then)], otherwise), e.span);
            }
            Ir::Match(scrutinee, arms) => self.match_expr(scrutinee, arms, dest, e.span),
            _ => {
                let value = self.expr(e);
                self.deliver(value, dest);
            }
        }
    }

    pub(super) fn stmt(&mut self, stmt: &IrStmt) {
        match stmt {
            // a branch delivers its value into the local itself
            IrStmt::Let {
                pat: Pat::Bind(local),
                value:
                    value @ IrExpr {
                        kind: Ir::If(..) | Ir::Match(..),
                        ..
                    },
                ..
            } => {
                let name = self.names.local(local);
                self.push(StmtKind::Local(name.clone(), None), value.span);
                self.to(value, &Dest::Into(name));
            }
            IrStmt::Let {
                pat: Pat::Bind(local),
                value,
                ..
            } => {
                let value = self.expr(value);
                let name = self.names.local(local);
                let span = value.span;
                self.push(StmtKind::Local(name, Some(value)), span);
            }
            IrStmt::Let { pat, value, .. } => {
                let span = value.span;
                let value = self.expr(value);
                let subject = self.subject(value);
                self.destructure(pat, &subject, span);
            }
            IrStmt::Var { name, value, .. } => {
                let value = self.expr(value);
                let local = self.names.local(name);
                self.vars.insert(local.as_str().to_owned());
                let span = value.span;
                self.push(StmtKind::Local(local, Some(value)), span);
            }
            IrStmt::Assign(name, value) => {
                let value = self.expr(value);
                let span = value.span;
                let target = Expr::name(&self.names.local(name), span);
                self.push(StmtKind::Assign(target, value), span);
            }
            IrStmt::Fun {
                name,
                rec,
                dicts,
                value,
                ..
            } => self.local_function(name, *rec, dicts, value),
            IrStmt::Expr(e) => self.to(e, &Dest::Drop),
        }
    }

    /// `fun name … = value`: a function sees itself only when `rec`.
    fn local_function(&mut self, local: &str, rec: bool, dicts: &[String], value: &IrExpr) {
        let span = value.span;
        let name = self.names.local(local);
        let value = if dicts.is_empty() {
            self.expr(value)
        } else {
            self.dictionaries_of(dicts, value, span)
        };
        match value.kind {
            ExprKind::Function(f) if rec => self.push(StmtKind::LocalFunction(name, *f), span),
            kind => self.push(StmtKind::Local(name, Some(Expr::new(kind, span))), span),
        }
    }

    /// A function of the dictionaries `shown` around `inner`.
    pub(super) fn dictionaries_of(&mut self, shown: &[String], inner: &IrExpr, span: Span) -> Expr {
        let mark = self.givens.len();
        let mut params = Vec::with_capacity(shown.len());
        for given in shown {
            let name = self.names.fresh("dict");
            self.givens.push((given.clone(), name.clone()));
            params.push(name);
        }
        let body = self.function_body(|g| g.to(inner, &Dest::Return));
        self.givens.truncate(mark);
        Expr::new(
            ExprKind::Function(Box::new(Function { params, body, span })),
            span,
        )
    }

    /// A curried function: one Lua function per stage, the body in the last.
    fn lambda(&mut self, params: &[Param], body: &IrExpr, span: Span) -> Expr {
        if params.iter().any(|p| p.stage != Stage::Pure) {
            return self.unsupported(span, "a function of a non-pure stage", RUNTIME);
        }
        let mut heads = Vec::with_capacity(params.len());
        let mut patterns = Vec::new();
        for p in params {
            heads.push(match &p.pat {
                Pat::Bind(local) => self.names.local(local),
                Pat::Wild | Pat::Lit(Lit::Unit) => Namer::unused(),
                pat => {
                    let name = self.names.fresh("p");
                    patterns.push((pat, name.clone()));
                    name
                }
            });
        }
        let body = self.function_body(|g| {
            for (pat, name) in &patterns {
                g.destructure(pat, name, span);
            }
            g.to(body, &Dest::Return);
        });
        let mut heads = heads.into_iter().rev();
        let last = heads.next().unwrap_or_else(Namer::unused);
        let mut f = Function {
            params: vec![last],
            body,
            span,
        };
        for head in heads {
            let mut outer = Block::default();
            outer.end(
                LastKind::Return(Expr::new(ExprKind::Function(Box::new(f)), span)),
                span,
            );
            f = Function {
                params: vec![head],
                body: outer,
                span,
            };
        }
        Expr::new(ExprKind::Function(Box::new(f)), span)
    }

    /// One stage applied: `func(arg)`, or a constructor applied to all its
    /// fields as the table it builds.
    fn app(&mut self, e: &IrExpr) -> Expr {
        let Ir::App {
            func,
            arg,
            stage,
            arg_first,
        } = &e.kind
        else {
            unreachable!("an application")
        };
        if *stage != Stage::Pure {
            return self.unsupported(e.span, "a call of a non-pure stage", RUNTIME);
        }
        if let Some(built) = self.constructed(e) {
            return built;
        }
        if let Some(called) = self.host_application(e) {
            return called;
        }
        let (f, a) = if *arg_first {
            let mut values = self.ordered(&[arg, func]);
            let f = values.pop().expect("two operands");
            let mut a = values.pop().expect("two operands");
            // the call evaluates its callee first
            if !self.stable(&a) && !self.stable(&f) {
                a = Expr::name(&self.local("a", a), e.span);
            }
            (f, a)
        } else {
            let mut values = self.ordered(&[func, arg]);
            let a = values.pop().expect("two operands");
            (values.pop().expect("two operands"), a)
        };
        Expr {
            span: e.span,
            ..f.call(vec![a])
        }
    }

    /// `while condition do body end`; the condition is tested at the top of
    /// each iteration, after the statements it needs.
    fn while_loop(&mut self, condition: &IrExpr, body: &IrExpr, span: Span) {
        let (stmts, c) = self.within_value(|g| g.expr(condition));
        let body = self.within(|g| g.to(body, &Dest::Drop));
        if stmts.is_empty() {
            self.push(StmtKind::While(c, body), span);
            return;
        }
        let mut each = stmts;
        let mut stop = Block::default();
        stop.end(LastKind::Break, span);
        each.push(Stmt::new(
            StmtKind::If(vec![(Expr::unary(UnOp::Not, c, span), stop)], None),
            span,
        ));
        each.append(body);
        self.push(
            StmtKind::While(Expr::new(ExprKind::Bool(true), span), each),
            span,
        );
    }
}
