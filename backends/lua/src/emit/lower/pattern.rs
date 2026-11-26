//! What a pattern tests and binds: the conditions on the parts of its
//! value, and the bindings of `let`, of parameters and of an arm.
//!
//! A part of the value is an expression that reads it, `s[1].x`, so a test
//! is a condition. The rest of a list is a slice of the list, which tests
//! read through and a binding copies. Alternatives that bind names are
//! tried in order with a flag, the first that matches binding them.

use flick_backend_support::decision::{Step, Test};
use flick_ir::{Constructor, JsClass, Lit, Pat, Rep, Span};

use super::super::ast::{BinOp, Block, Expr, ExprKind, Name, Stmt, StmtKind, UnOp};
use super::Lowering;
use super::data::held_key;

/// A part of the value a pattern matches.
#[derive(Debug, Clone)]
pub(super) enum Place {
    Value(Expr),
    /// The items of a list after the first `n`.
    Slice(Expr, usize),
}

/// What a pattern asks of its value, in order.
pub(super) enum Check<'p> {
    Cond(Expr),
    Bind(&'p str, Place),
    /// Alternatives that bind names: the first that matches binds them.
    Or(Vec<Vec<Check<'p>>>),
}

/// Where a binding goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Sink {
    /// A new local.
    Declare,
    /// The local declared before.
    Assign,
    /// The field of the module's table: a top-level binding.
    Module,
}

/// `conds` joined by `op`; `None` without any.
pub(super) fn all(op: BinOp, conds: impl IntoIterator<Item = Expr>, span: Span) -> Option<Expr> {
    conds
        .into_iter()
        .reduce(|l, r| Expr::binary(op, l, r, span))
}

impl Lowering<'_> {
    /// The place `step` leads to from `place`.
    pub(super) fn step(place: &Place, step: Step<'_>, span: Span) -> Place {
        let index = |e: &Expr, i: usize| e.clone().index(Expr::number(i, span));
        match (place, step) {
            (Place::Value(p), Step::Field(c, i)) if i >= c.arity => {
                Place::Value(p.clone().index(Expr::str(&held_key(i - c.arity), span)))
            }
            (Place::Value(p), Step::Field(_, i) | Step::Item(i)) => Place::Value(index(p, i + 1)),
            (Place::Slice(l, k), Step::Item(i)) => Place::Value(index(l, k + i + 1)),
            (Place::Value(p), Step::Rest(n)) => Place::Slice(p.clone(), n),
            (Place::Slice(l, k), Step::Rest(n)) => Place::Slice(l.clone(), k + n),
            (Place::Value(p), Step::Label(label)) => Place::Value(p.clone().field(label)),
            // the rest of a list has no fields: typed IR never asks for one
            (Place::Slice(l, _), Step::Field(..) | Step::Label(_)) => Place::Value(l.clone()),
        }
    }

    /// The condition that `test` holds of the value at `place`.
    pub(super) fn cond(&mut self, place: &Place, test: Test<'_>, span: Span) -> Expr {
        let eq = |l: Expr, r: Expr| Expr::binary(BinOp::Eq, l, r, span);
        let p = match (place, test) {
            (Place::Value(p) | Place::Slice(p, _), Test::Len { n, rest }) => {
                let offset = match place {
                    Place::Slice(_, k) => *k,
                    Place::Value(_) => 0,
                };
                let op = if rest { BinOp::Ge } else { BinOp::Eq };
                let len = Expr::unary(UnOp::Len, p.clone(), span);
                return Expr::binary(op, len, Expr::number(n + offset, span), span);
            }
            (Place::Value(p), _) => p.clone(),
            (Place::Slice(..), _) => {
                return self.refuse(span, "malformed IR: a test of a list's rest".to_owned());
            }
        };
        match test {
            Test::Tag(c) => eq(p.field("_tag"), Expr::str(&self.tag_of(c), span)),
            Test::Class(c) => {
                let class = match c.rep {
                    Rep::Typeof(JsClass::Number) => "number",
                    Rep::Typeof(JsClass::String) => "string",
                    Rep::Typeof(JsClass::Boolean) => "boolean",
                    Rep::Typeof(JsClass::Array) => "table",
                    _ => {
                        return self.unsupported(
                            span,
                            "a big integer constructor of an untagged type",
                            "the Lua kernel has no big integers",
                        );
                    }
                };
                let ty = Expr::name(&Name::new("type".to_owned()), span).call(vec![p]);
                eq(ty, Expr::str(class, span))
            }
            Test::Enum(c) => match &c.rep {
                Rep::Literal(literal) => {
                    let literal = Self::enum_literal(literal, span);
                    eq(p, literal)
                }
                _ => self.refuse(
                    span,
                    "malformed IR: an enum test of a constructor".to_owned(),
                ),
            },
            Test::Lit(Lit::Float(_) | Lit::Float32(_)) => self.unsupported(
                span,
                "a float literal pattern",
                "the Lua primitive kernel, which compares floats exactly, is not implemented",
            ),
            Test::Lit(lit) => {
                let literal = self.literal(lit, span);
                eq(p, literal)
            }
            Test::Len { .. } => unreachable!("tested above"),
        }
    }

    /// The checks of `pat` on the value at `place`.
    pub(super) fn checks<'q>(
        &mut self,
        pat: &'q Pat,
        place: &Place,
        span: Span,
        out: &mut Vec<Check<'q>>,
    ) {
        match pat {
            Pat::Wild | Pat::Lit(Lit::Unit) => {}
            Pat::Bind(name) => out.push(Check::Bind(name, place.clone())),
            Pat::Lit(lit) => out.push(Check::Cond(self.cond(place, Test::Lit(lit), span))),
            Pat::Tuple(items) => {
                for (i, item) in items.iter().enumerate() {
                    self.checks(item, &Self::step(place, Step::Item(i), span), span, out);
                }
            }
            Pat::List(items, rest) => {
                let n = items.len();
                let len = Test::Len {
                    n,
                    rest: rest.is_some(),
                };
                out.push(Check::Cond(self.cond(place, len, span)));
                for (i, item) in items.iter().enumerate() {
                    self.checks(item, &Self::step(place, Step::Item(i), span), span, out);
                }
                if let Some(rest) = rest {
                    self.checks(rest, &Self::step(place, Step::Rest(n), span), span, out);
                }
            }
            Pat::Record(fields) => {
                for (label, p) in fields {
                    self.checks(p, &Self::step(place, Step::Label(label), span), span, out);
                }
            }
            Pat::Con(c, args) => {
                let test = match c.rep {
                    Rep::Tagged => Some(Test::Tag(c)),
                    Rep::Typeof(_) => Some(Test::Class(c)),
                    Rep::Literal(_) => Some(Test::Enum(c)),
                    Rep::Unboxed => None,
                };
                if let Some(test) = test {
                    out.push(Check::Cond(self.cond(place, test, span)));
                }
                for (i, arg) in args.iter().enumerate() {
                    let at = match c.rep {
                        Rep::Tagged => Self::step(place, Step::Field(c, i), span),
                        _ => place.clone(),
                    };
                    self.checks(arg, &at, span, out);
                }
            }
            Pat::Or(alternatives) => {
                let mut alts = Vec::with_capacity(alternatives.len());
                for alt in alternatives {
                    let mut checks = Vec::new();
                    self.checks(alt, place, span, &mut checks);
                    alts.push(checks);
                }
                if alts.iter().flatten().any(|c| !matches!(c, Check::Cond(_))) {
                    out.push(Check::Or(alts));
                    return;
                }
                // without names, a condition: one alternative's tests hold
                let mut any = Vec::with_capacity(alts.len());
                for alt in alts {
                    let conds = alt.into_iter().filter_map(|c| match c {
                        Check::Cond(e) => Some(e),
                        _ => None,
                    });
                    match all(BinOp::And, conds, span) {
                        Some(cond) => any.push(cond),
                        // an alternative that matches anything
                        None => return,
                    }
                }
                out.extend(all(BinOp::Or, any, span).map(Check::Cond));
            }
        }
    }

    /// The condition that the value of `subject` is of the constructor `c`.
    pub(super) fn is_constructor(&mut self, c: &Constructor, subject: &Expr, span: Span) -> Expr {
        let pat = Pat::Con(c.clone(), Vec::new());
        let mut checks = Vec::new();
        self.checks(&pat, &Place::Value(subject.clone()), span, &mut checks);
        let conds = checks.into_iter().filter_map(|c| match c {
            Check::Cond(e) => Some(e),
            _ => None,
        });
        all(BinOp::And, conds, span).unwrap_or(Expr::new(ExprKind::Bool(true), span))
    }

    /// The field `i` of a value of the constructor `c`.
    pub(super) fn field_of(c: &Constructor, i: usize, value: Expr) -> Expr {
        let span = value.span;
        match c.rep {
            Rep::Tagged => value.index(Expr::number(i + 1, span)),
            _ => value,
        }
    }

    /// Binds `name` to the value at `place`.
    pub(super) fn bind(&mut self, sink: Sink, name: &str, place: &Place, span: Span) {
        let value = match place {
            Place::Value(p) => p.clone(),
            Place::Slice(list, k) => {
                let copy = match sink {
                    Sink::Declare => self.names.local(name),
                    Sink::Assign | Sink::Module => self.names.fresh("rest"),
                };
                let empty = Expr::new(ExprKind::Table(Vec::new()), span);
                self.push(StmtKind::Local(copy.clone(), Some(empty)), span);
                let i = self.names.fresh("i");
                let at = Expr::name(&i, span);
                let to = if *k == 0 {
                    at.clone()
                } else {
                    Expr::binary(BinOp::Sub, at.clone(), Expr::number(*k, span), span)
                };
                let mut body = Block::default();
                body.push(Stmt::new(
                    StmtKind::Assign(Expr::name(&copy, span).index(to), list.clone().index(at)),
                    span,
                ));
                let len = Expr::unary(UnOp::Len, list.clone(), span);
                self.push(StmtKind::For(i, Expr::number(k + 1, span), len, body), span);
                if sink == Sink::Declare {
                    return;
                }
                Expr::name(&copy, span)
            }
        };
        let kind = match sink {
            Sink::Declare => StmtKind::Local(self.names.local(name), Some(value)),
            Sink::Assign => StmtKind::Assign(Expr::name(&self.names.local(name), span), value),
            Sink::Module => StmtKind::Assign(self.table(self.module, span).field(name), value),
        };
        self.push(kind, span);
    }

    /// The names an alternative binds.
    fn binders<'q>(checks: &[Check<'q>], out: &mut Vec<&'q str>) {
        for check in checks {
            match check {
                Check::Bind(name, _) if !out.contains(name) => out.push(name),
                Check::Or(alts) => Self::binders(alts.first().map_or(&[], |a| &a[..]), out),
                Check::Cond(_) | Check::Bind(..) => {}
            }
        }
    }

    /// Tries `alts` in order, the first that matches binding its names to
    /// `sink`; the local that says whether one did.
    fn alternatives(&mut self, alts: &[Vec<Check<'_>>], sink: Sink, span: Span) -> Name {
        let sink = if sink == Sink::Declare {
            let mut names = Vec::new();
            Self::binders(alts.first().map_or(&[], |a| &a[..]), &mut names);
            for name in names {
                let local = self.names.local(name);
                self.push(StmtKind::Local(local, None), span);
            }
            Sink::Assign
        } else {
            sink
        };
        let matched = self.local("m", Expr::new(ExprKind::Bool(false), span));
        for (i, alt) in alts.iter().enumerate() {
            let body = self.within(|g| {
                g.emit_checks(alt, sink, span, &mut |g| {
                    let target = Expr::name(&matched, span);
                    g.push(
                        StmtKind::Assign(target, Expr::new(ExprKind::Bool(true), span)),
                        span,
                    );
                });
            });
            // the first alternative is tried while none has matched
            if i == 0 {
                self.inline(body);
            } else {
                let untried = Expr::unary(UnOp::Not, Expr::name(&matched, span), span);
                self.push(StmtKind::If(vec![(untried, body)], None), span);
            }
        }
        matched
    }

    /// `then` where `checks` hold, their names bound to `sink` first.
    pub(super) fn emit_checks(
        &mut self,
        checks: &[Check<'_>],
        sink: Sink,
        span: Span,
        then: &mut dyn FnMut(&mut Self),
    ) {
        let split = (checks.iter())
            .position(|c| matches!(c, Check::Or(_)))
            .unwrap_or(checks.len());
        let (head, rest) = checks.split_at(split);
        let inner = self.within(|g| {
            for check in head {
                if let Check::Bind(name, place) = check {
                    g.bind(sink, name, place, span);
                }
            }
            match rest.split_first() {
                Some((Check::Or(alts), rest)) => {
                    let matched = g.alternatives(alts, sink, span);
                    let body = g.within(|g| g.emit_checks(rest, sink, span, then));
                    g.push(
                        StmtKind::If(vec![(Expr::name(&matched, span), body)], None),
                        span,
                    );
                }
                _ => then(g),
            }
        });
        let conds = head.iter().filter_map(|c| match c {
            Check::Cond(e) => Some(e.clone()),
            _ => None,
        });
        match all(BinOp::And, conds, span) {
            Some(cond) => self.push(StmtKind::If(vec![(cond, inner)], None), span),
            None => self.inline(inner),
        }
    }

    /// Binds the names of an irrefutable `pat` to the parts of `subject`.
    pub(super) fn destructure(&mut self, pat: &Pat, subject: &Name, span: Span) {
        self.destructure_to(pat, Expr::name(subject, span), Sink::Declare, span);
    }

    /// Binds the names of an irrefutable `pat` to the parts of `subject`,
    /// into `sink`: every test of its parts holds.
    pub(super) fn destructure_to(&mut self, pat: &Pat, subject: Expr, sink: Sink, span: Span) {
        let mut checks = Vec::new();
        self.checks(pat, &Place::Value(subject), span, &mut checks);
        for check in &checks {
            match check {
                Check::Cond(_) => {}
                Check::Bind(name, place) => self.bind(sink, name, place, span),
                Check::Or(alts) => {
                    self.alternatives(alts, sink, span);
                }
            }
        }
    }
}
