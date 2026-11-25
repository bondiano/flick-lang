//! Data: constructors, records, module values and dictionaries, and the
//! `Some` that `?` takes apart.

use std::iter;

use flick_backend_support::names::{instance, tag};
use flick_ir::{
    Binding, Constructor, Dict, Expr as IrExpr, ExprKind as Ir, ItemKind, Lang, Rep, Span, Stage,
};

use super::super::ast::{Block, Expr, ExprKind, Field, Function, LastKind, Name, Stmt, StmtKind};
use super::{Lowering, RUNTIME};

impl<'p> Lowering<'p> {
    /// A constructor applied to all its fields, at pure stages in order.
    pub(super) fn constructed(&mut self, e: &IrExpr) -> Option<Expr> {
        let mut args = Vec::new();
        let mut head = e;
        while let Ir::App {
            func,
            arg,
            stage: Stage::Pure,
            arg_first: false,
        } = &head.kind
        {
            args.push(&**arg);
            head = func;
        }
        let Ir::Con(c) = &head.kind else {
            return None;
        };
        if c.dicts > 0 || args.len() != c.arity {
            return None;
        }
        args.reverse();
        let values = self.ordered(&args);
        Some(match c.rep {
            Rep::Tagged => self.tagged(c, values.into_iter().map(Field::Item), e.span),
            _ => values.into_iter().next()?,
        })
    }

    /// The table of a tagged constructor's value.
    fn tagged(&self, c: &Constructor, items: impl Iterator<Item = Field>, span: Span) -> Expr {
        let tag = Field::Pair(Expr::str("_tag", span), Expr::str(&self.tag_of(c), span));
        Expr::new(
            ExprKind::Table(iter::once(tag).chain(items).collect()),
            span,
        )
    }

    /// A constructor as a value: a function of its dictionaries, then one
    /// of each field, or the value itself without either.
    pub(super) fn constructor(&mut self, c: &Constructor, span: Span) -> Expr {
        let function = |params: Vec<Name>, value: Expr| {
            let mut body = Block::default();
            body.end(LastKind::Return(value), span);
            Expr::new(
                ExprKind::Function(Box::new(Function { params, body, span })),
                span,
            )
        };
        match &c.rep {
            Rep::Literal(literal) => Self::enum_literal(literal, span),
            Rep::Unboxed | Rep::Typeof(_) => {
                let x = self.names.fresh("x");
                function(vec![x.clone()], Expr::name(&x, span))
            }
            Rep::Tagged => {
                let fields: Vec<Name> = (0..c.arity).map(|_| self.names.fresh("f")).collect();
                let dicts: Vec<Name> = (0..c.dicts).map(|_| self.names.fresh("dict")).collect();
                let items = (fields.iter().chain(&dicts)).map(|n| Field::Item(Expr::name(n, span)));
                let mut value = self.tagged(c, items, span);
                for field in fields.into_iter().rev() {
                    value = function(vec![field], value);
                }
                if !dicts.is_empty() {
                    value = function(dicts, value);
                }
                value
            }
        }
    }

    /// The tag of a tagged constructor's values, as the JS target writes it.
    pub(super) fn tag_of(&self, c: &Constructor) -> String {
        let package = self.packages.get(c.module.as_str()).copied().flatten();
        tag(c, package)
    }

    /// A top-level value of a module, read from its table.
    pub(super) fn global(&mut self, module: &str, name: &str, span: Span) -> Expr {
        match self.items.get(&(module, name)) {
            Some(ItemKind::Foreign(f)) => {
                let what = format!("`{module}.{}`", f.source_name.as_ref().unwrap_or(&f.name));
                let why = match &f.binding {
                    Binding::Module { from, .. } if from.starts_with("flick:") => {
                        "the Lua primitive kernel has no row for it"
                    }
                    _ => "foreign functions need the Lua value ABI, which is not implemented",
                };
                return self.unsupported(span, &what, why);
            }
            Some(ItemKind::Service { .. }) => {
                return self.unsupported(span, &format!("the service `{module}.{name}`"), RUNTIME);
            }
            _ => {}
        }
        self.table(module, span).field(name)
    }

    /// The dictionary a constraint resolved to.
    pub(super) fn dict(&mut self, dict: &Dict, span: Span) -> Expr {
        match dict {
            Dict::Instance { module, name, args } => {
                let head = self.table(module, span).field(&instance(name));
                if args.is_empty() {
                    head
                } else {
                    let args = args.iter().map(|d| self.dict(d, span)).collect();
                    head.call(args)
                }
            }
            Dict::Given(shown) => match self.givens.iter().rev().find(|(g, _)| g == shown) {
                Some((_, name)) => Expr::name(name, span),
                None => self.refuse(
                    span,
                    format!("malformed IR: no dictionary `{shown}` in scope"),
                ),
            },
            Dict::Value { module, name } => self.global(module, name, span),
            Dict::Opened { binder, field } => {
                Expr::name(&self.names.local(binder), span).field(field)
            }
            Dict::Field(local) => Expr::name(&self.names.local(local), span),
            Dict::Super { dict, class } => self.dict(dict, span).field(&format!("${class}")),
            Dict::Site { place, definition } => Expr::new(
                ExprKind::Table(vec![
                    Field::Pair(Expr::str("place", span), Expr::str(place, span)),
                    Field::Pair(Expr::str("definition", span), Expr::str(definition, span)),
                ]),
                span,
            ),
            Dict::Primitive { labels, .. } => Expr::new(
                ExprKind::Table(
                    labels
                        .iter()
                        .map(|l| Field::Item(Expr::str(l, span)))
                        .collect(),
                ),
                span,
            ),
            Dict::Structural { class, .. } => self.unsupported(
                span,
                &format!("the structural `{class}` of a record or tuple"),
                "the Lua runtime that derives it is not implemented",
            ),
            Dict::Error => self.refuse(span, "malformed IR: an unresolved dictionary".to_owned()),
        }
    }

    /// A record or a dictionary: its fields in the order written.
    pub(super) fn record(&mut self, fields: &[(String, IrExpr)], span: Span) -> Expr {
        let exprs: Vec<&IrExpr> = fields.iter().map(|(_, v)| v).collect();
        let values = self.ordered(&exprs);
        let pairs = (fields.iter().zip(values))
            .map(|((label, _), v)| Field::Pair(Expr::str(label, span), v))
            .collect();
        Expr::new(ExprKind::Table(pairs), span)
    }

    /// `{ base with … }`: a copy of every field of `base`, then the new ones.
    pub(super) fn update(
        &mut self,
        base: &IrExpr,
        fields: &[(String, IrExpr)],
        span: Span,
    ) -> Expr {
        let mut exprs = vec![base];
        exprs.extend(fields.iter().map(|(_, v)| v));
        let mut values = self.ordered(&exprs).into_iter();
        let base = values.next().expect("the base");
        let copy = self.local("r", Expr::new(ExprKind::Table(Vec::new()), span));
        let (key, value) = (self.names.fresh("k"), self.names.fresh("x"));
        let mut body = Block::default();
        let target = Expr::name(&copy, span).index(Expr::name(&key, span));
        body.push(Stmt::new(
            StmtKind::Assign(target, Expr::name(&value, span)),
            span,
        ));
        self.push(StmtKind::Pairs(key, value, base, body), span);
        for ((label, _), v) in fields.iter().zip(values) {
            let target = Expr::name(&copy, span).field(label);
            self.push(StmtKind::Assign(target, v), span);
        }
        Expr::name(&copy, span)
    }

    /// `o?`: the field of a `Some`; a `None` is returned from the function.
    pub(super) fn unwrap(&mut self, option: &IrExpr, span: Span) -> Expr {
        let (none, some) = (
            self.lang_constructor(Lang::None),
            self.lang_constructor(Lang::Some),
        );
        let (Some(none), Some(some)) = (none, some) else {
            return self.refuse(
                span,
                "malformed IR: `?` without `None` and `Some`".to_owned(),
            );
        };
        if self.functions == 0 {
            return self.refuse(span, "malformed IR: `?` outside a function".to_owned());
        }
        let value = self.expr(option);
        let subject = self.subject(value);
        let test = self.is_constructor(none, &Expr::name(&subject, span), span);
        let mut early = Block::default();
        early.end(LastKind::Return(Expr::name(&subject, span)), span);
        self.push(StmtKind::If(vec![(test, early)], None), span);
        Self::field_of(some, 0, Expr::name(&subject, span))
    }

    /// The constructor of a lang item, if the program has it.
    fn lang_constructor(&self, lang: Lang) -> Option<&'p Constructor> {
        let (module, name) = self.known.get(lang)?;
        self.constructors.get(&(module, name)).copied()
    }
}
