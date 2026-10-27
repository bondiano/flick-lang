//! `server fn name x = body`: the witnesses of what it sends, its body as
//! `name$impl`, the procedure `name$procedure` of its calls, the call
//! `name` every caller makes, and the implementation `name$server` that
//! `Server.handler` serves.

use super::Lower;
use crate::ir::{Expr, ExprKind, Item, ItemKind, Lit, Stage};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::types::companion_name;
use crate::types::server_fn::{IMPLEMENTATION, PAYLOAD, SUCCESS, error_member};

impl Lower<'_> {
    pub(super) fn server_fn(&mut self, decl: &Node, span: Span, items: &mut Vec<Item>) {
        let Some(info) = self
            .typing
            .server_fns
            .iter()
            .find(|f| f.decl == decl.range())
            .cloned()
        else {
            return;
        };
        let name = info.name;
        for schema in self
            .typing
            .schemas
            .iter()
            .filter(|s| s.decl == decl.range())
        {
            items.push(Item {
                kind: ItemKind::Schema(schema.clone()),
                span,
            });
        }
        self.fresh_definition();
        self.definition.clone_from(&name);
        let body = self.definition(decl);
        let implementation = companion_name(&name, "impl");
        let procedure = companion_name(&name, "procedure");
        let witness = |l: &Self, member: &str| {
            l.global(
                &companion_name(&companion_name(&name, member), "schema"),
                span,
            )
        };
        let errors = (0..info.errors)
            .map(|i| witness(self, &error_member(i)))
            .collect();
        let made = apply(
            server("procedure", span),
            vec![
                Expr::new(
                    ExprKind::Lit(Lit::String(format!("{}.{name}", self.module))),
                    span,
                ),
                witness(self, PAYLOAD),
                witness(self, SUCCESS),
                Expr::new(ExprKind::List(errors), span),
            ],
        );
        let call = apply(server("call", span), vec![self.global(&procedure, span)]);
        let served = apply(
            server("function", span),
            vec![
                self.global(&procedure, span),
                self.global(&implementation, span),
            ],
        );
        for (name, body) in [
            (implementation, body),
            (procedure, made),
            (name.clone(), call),
            (companion_name(&name, IMPLEMENTATION), served),
        ] {
            items.push(Item {
                kind: ItemKind::Value {
                    name,
                    dicts: Vec::new(),
                    body,
                },
                span,
            });
        }
    }

    fn global(&self, name: &str, span: Span) -> Expr {
        Expr::new(
            ExprKind::Global {
                module: self.module.to_owned(),
                name: name.to_owned(),
            },
            span,
        )
    }
}

fn server(name: &str, span: Span) -> Expr {
    Expr::new(
        ExprKind::Global {
            module: "Server".to_owned(),
            name: name.to_owned(),
        },
        span,
    )
}

/// `f a b …`, each application pure.
fn apply(f: Expr, args: Vec<Expr>) -> Expr {
    args.into_iter().fold(f, |func, arg| {
        let span = arg.span;
        Expr::new(
            ExprKind::App {
                func: Box::new(func),
                arg: Box::new(arg),
                stage: Stage::Pure,
                arg_first: false,
            },
            span,
        )
    })
}
