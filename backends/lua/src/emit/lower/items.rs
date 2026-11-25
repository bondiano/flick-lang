//! Top-level items, each the assignment of its value to the module's table.

use flick_backend_support::names::instance;
use flick_ir::{Binding, Expr as IrExpr, InstanceInfo, ItemKind, Span, default_method};

use super::super::ast::{BinOp, Block, Expr, ExprKind, Field, Function, LastKind, Name, StmtKind};
use super::Lowering;
use super::pattern::Sink;

impl Lowering<'_> {
    /// The statements that define an item; those its value needs are in a
    /// `do` block of their own, so the chunk's locals stay few.
    pub(super) fn item_kind(&mut self, kind: &ItemKind, span: Span) {
        let block = self.within(|g| g.definition(kind, span));
        if block.stmts.len() > 1 {
            self.push(StmtKind::Do(block), span);
        } else {
            self.inline(block);
        }
    }

    fn definition(&mut self, kind: &ItemKind, span: Span) {
        match kind {
            ItemKind::Value {
                name, dicts, body, ..
            } => {
                let value = if dicts.is_empty() {
                    self.expr(body)
                } else {
                    self.dictionaries_of(dicts, body, span)
                };
                self.define(name, value, span);
            }
            ItemKind::Destructure { pattern, body, .. } => {
                let value = self.expr(body);
                let subject = self.subject(value);
                self.destructure_to(pattern, Expr::name(&subject, span), Sink::Module, span);
            }
            ItemKind::Constructor(c) => {
                let value = self.constructor(c, span);
                self.define(&c.name, value, span);
            }
            ItemKind::Instance { info, methods } => self.instance(info, methods, span),
            ItemKind::Foreign(f) => {
                let what = format!("the foreign function `{}.{}`", self.module, f.name);
                let why = match &f.binding {
                    Binding::Module { from, .. } if from.starts_with("flick:") => {
                        "the Lua kernel table refuses it: the Lua primitive kernel is not implemented"
                    }
                    _ => "foreign functions need the Lua value ABI, which is not implemented",
                };
                self.unsupported(span, &what, why);
            }
            ItemKind::Service { name, .. } => {
                let what = format!("the service `{}.{name}`", self.module);
                self.unsupported(
                    span,
                    &what,
                    "services need the Lua runtime, which is not implemented",
                );
            }
        }
    }

    /// `M.name = value`.
    fn define(&mut self, name: &str, value: Expr, span: Span) {
        let target = self.table(self.module, span).field(name);
        self.push(StmtKind::Assign(target, value), span);
    }

    /// An instance's dictionary; with a context, a function of the
    /// context's dictionaries that builds it.
    fn instance(&mut self, info: &InstanceInfo, methods: &[(String, IrExpr)], span: Span) {
        if methods.is_empty() && info.derived.is_some() {
            let what = format!("the derived instance `{}`", info.name);
            self.unsupported(
                span,
                &what,
                "the Lua runtime that derives it is not implemented",
            );
            return;
        }
        let value = if info.context.is_empty() {
            self.dictionary(info, methods, span)
        } else {
            let mark = self.givens.len();
            let mut params = Vec::with_capacity(info.context.len());
            for given in &info.context {
                let name = self.names.fresh("dict");
                self.givens.push((given.clone(), name.clone()));
                params.push(name);
            }
            let body = self.function_body(|g| {
                let dict = g.dictionary(info, methods, span);
                g.block.end(LastKind::Return(dict), span);
            });
            self.givens.truncate(mark);
            Expr::new(
                ExprKind::Function(Box::new(Function { params, body, span })),
                span,
            )
        };
        self.define(&instance(&info.name), value, span);
    }

    /// The table of an instance's methods. Its superclasses and the
    /// methods it takes from the class's defaults are read through
    /// `__index`, computed at each read: a default takes the dictionary
    /// itself, which is complete only once built.
    fn dictionary(
        &mut self,
        info: &InstanceInfo,
        methods: &[(String, IrExpr)],
        span: Span,
    ) -> Expr {
        let exprs: Vec<&IrExpr> = methods.iter().map(|(_, m)| m).collect();
        let values = self.ordered(&exprs);
        let entries = (methods.iter().zip(values))
            .map(|((name, _), v)| Field::Pair(Expr::str(name, span), v))
            .collect();
        let table = Expr::new(ExprKind::Table(entries), span);
        if info.supers.is_empty() && info.defaults.is_empty() {
            return table;
        }
        let (dict, key) = (self.names.fresh("dict"), self.names.fresh("key"));
        let body = self.function_body(|g| {
            for (class, d) in &info.supers {
                let value = g.dict(d, span);
                g.answer(&key, &format!("${class}"), value, span);
            }
            let (module, class) = &info.class;
            for (name, own, extra) in &info.defaults {
                let default = g.table(module, span).field(&default_method(class, name));
                let mut args = vec![Expr::name(&dict, span)];
                args.extend(extra.iter().map(|d| g.dict(d, span)));
                let value = if *own == 0 {
                    default.call(args)
                } else {
                    let own: Vec<Name> = (0..*own).map(|_| g.names.fresh("dict")).collect();
                    args.extend(own.iter().map(|o| Expr::name(o, span)));
                    let mut body = Block::default();
                    body.end(LastKind::Return(default.call(args)), span);
                    Expr::new(
                        ExprKind::Function(Box::new(Function {
                            params: own,
                            body,
                            span,
                        })),
                        span,
                    )
                };
                g.answer(&key, name, value, span);
            }
        });
        let index = Expr::new(
            ExprKind::Function(Box::new(Function {
                params: vec![dict, key],
                body,
                span,
            })),
            span,
        );
        let meta = Expr::new(
            ExprKind::Table(vec![Field::Pair(Expr::str("__index", span), index)]),
            span,
        );
        Expr::name(&Name::new("setmetatable".to_owned()), span).call(vec![table, meta])
    }

    /// `if key == name then return value end`.
    fn answer(&mut self, key: &Name, name: &str, value: Expr, span: Span) {
        let test = Expr::binary(
            BinOp::Eq,
            Expr::name(key, span),
            Expr::str(name, span),
            span,
        );
        let mut then = Block::default();
        then.end(LastKind::Return(value), span);
        self.push(StmtKind::If(vec![(test, then)], None), span);
    }
}
