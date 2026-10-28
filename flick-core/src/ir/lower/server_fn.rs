//! `server fn name x = body`: the witnesses of what it sends, its body as
//! `name$impl`, the procedure `name$procedure` of its calls, the call
//! `name` every caller makes, and the implementation `name$server` that
//! `Server.handler` serves. Several parameters travel as one tuple: the
//! call gathers them and `name$impl` takes the tuple.

use super::Lower;
use crate::ir::{Expr, ExprKind, Item, ItemKind, Lit, Param, Pat, Stage};
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
        let mut body = self.definition(decl);
        if info.params > 1
            && let ExprKind::Lambda { params, .. } = &mut body.kind
        {
            let stage = params.last().map_or(Stage::Runtime, |p| p.stage);
            let pats = params.drain(..).map(|p| p.pat).collect();
            params.push(Param {
                pat: Pat::Tuple(pats),
                stage,
            });
        }
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
        let mut call = apply(server("call", span), vec![self.global(&procedure, span)]);
        if info.params > 1 {
            let names: Vec<String> = (0..info.params).map(|_| self.hidden("$p")).collect();
            let payload = names
                .iter()
                .map(|n| Expr::new(ExprKind::Local(n.clone()), span))
                .collect();
            let ran = Expr::new(
                ExprKind::App {
                    func: Box::new(call),
                    arg: Box::new(Expr::new(ExprKind::Tuple(payload), span)),
                    stage: Stage::Runtime,
                    arg_first: false,
                },
                span,
            );
            let last = names.len() - 1;
            let params = names
                .into_iter()
                .enumerate()
                .map(|(i, n)| Param {
                    pat: Pat::Bind(n),
                    stage: if i == last { Stage::Runtime } else { Stage::Pure },
                })
                .collect();
            call = Expr::new(
                ExprKind::Lambda {
                    params,
                    body: Box::new(ran),
                },
                span,
            );
        }
        let served = apply(
            server(if info.guarded { "guarded" } else { "function" }, span),
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
