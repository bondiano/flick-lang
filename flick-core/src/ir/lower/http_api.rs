//! `deriving HttpApi`: `T$endpoints`, the record of the endpoints, and
//! `T$api`, the group, built from the `HttpApi` functions a program would
//! call, as the checker planned them (`types::http_api`).

use super::Lower;
use crate::ir::{Expr, ExprKind, Item, ItemKind, Lit, Param, Pat, Stage};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::types::Dict;
use crate::types::companion_name;
use crate::types::http_api::{Auth, Codec, EndpointPlan, HTTP_API_DERIVE, HttpApiItem};

const HTTP_API: &str = "HttpApi";

impl Lower<'_> {
    pub(super) fn http_api(&mut self, decl: &Node, span: Span, items: &mut Vec<Item>) {
        let Some(api) = self
            .typing
            .http_apis
            .iter()
            .find(|a| a.decl == decl.range())
            .cloned()
        else {
            return;
        };
        let fields = api
            .endpoints
            .iter()
            .map(|e| (e.label.clone(), endpoint(e, span)))
            .collect();
        let endpoints = Expr::new(ExprKind::Record(fields), span);
        self.fresh_definition();
        let body = self.group(&api, span);
        for (member, body) in [("endpoints", endpoints), ("api", body)] {
            items.push(Item {
                kind: ItemKind::Value {
                    name: companion_name(&api.ty, member),
                    dicts: Vec::new(),
                    body,
                },
                span,
            });
        }
    }

    /// The group; secured, a function of the scheme, the service and its
    /// checks that guards its `Authed` and `MaybeAuthed` endpoints.
    fn group(&mut self, api: &HttpApiItem, span: Span) -> Expr {
        let names = ["security", "tag", "check", "maybe"].map(|n| self.hidden(n));
        let local = |n: &str| Expr::new(ExprKind::Local(n.to_owned()), span);
        let label = Expr::new(ExprKind::Label(api.group.clone()), span);
        let mut group = apply(global(HTTP_API, "group", span), vec![label]);
        for e in &api.endpoints {
            let made = endpoint(e, span);
            let guarded = match e.auth {
                Auth::None => made,
                Auth::Required | Auth::Optional => {
                    let (with, check) = if e.auth == Auth::Required {
                        ("with_auth", &names[2])
                    } else {
                        ("with_optional_auth", &names[3])
                    };
                    let args = vec![local(&names[0]), local(&names[1]), local(check), made];
                    apply(global(HTTP_API, with, span), args)
                }
            };
            // `add` and `add_group` take the row constraints' dictionaries,
            // which carry nothing at run time
            let add = Expr::new(
                ExprKind::Dicts(
                    Box::new(global(HTTP_API, "add", span)),
                    vec![primitive("Lacks"), primitive("UnionDisjoint")],
                ),
                span,
            );
            group = apply(add, vec![guarded, group]);
        }
        if !api.secured {
            return group;
        }
        let guarded = apply(
            global(HTTP_API, "authenticated", span),
            vec![local(&names[1]), group],
        );
        let params = names
            .into_iter()
            .map(|n| Param {
                pat: Pat::Bind(n),
                stage: Stage::Pure,
                ty: None,
            })
            .collect();
        Expr::new(
            ExprKind::Lambda {
                params,
                body: Box::new(guarded),
            },
            span,
        )
    }
}

/// `HttpApi.endpoint M "/path" label |> with_<part> codec … |> with_success
/// status codec |> with_errors (errors |> problem_case …)`.
fn endpoint(e: &EndpointPlan, span: Span) -> Expr {
    let string = |s: &str| Expr::new(ExprKind::Lit(Lit::String(s.to_owned())), span);
    let mut made = apply(
        global(HTTP_API, "endpoint", span),
        vec![
            Expr::new(ExprKind::Con(e.method.clone()), span),
            string(&e.path),
            Expr::new(ExprKind::Label(e.label.clone()), span),
        ],
    );
    for (part, c) in &e.parts {
        let with = format!("with_{part}");
        made = apply(global(HTTP_API, &with, span), vec![codec(c, span), made]);
    }
    if let Some((status, c)) = &e.success {
        let status = Expr::new(ExprKind::Lit(Lit::Int(*status)), span);
        made = apply(
            global(HTTP_API, "with_success", span),
            vec![status, codec(c, span), made],
        );
    }
    if e.errors.is_empty() {
        return made;
    }
    let cases = e
        .errors
        .iter()
        .fold(global(HTTP_API, "errors", span), |cases, dict| {
            let case = Expr::new(
                ExprKind::Dicts(
                    Box::new(global(HTTP_API_DERIVE, "problem_case", span)),
                    vec![dict.clone()],
                ),
                span,
            );
            apply(case, vec![cases])
        });
    apply(global(HTTP_API, "with_errors", span), vec![cases, made])
}

fn codec(c: &Codec, span: Span) -> Expr {
    match c {
        Codec::Value { module, name } => global(module, name, span),
        Codec::Schema { module, ty } => apply(
            global("Codec", "json", span),
            vec![global(module, &companion_name(ty, "schema"), span)],
        ),
    }
}

fn primitive(constraint: &str) -> Dict {
    Dict::Primitive {
        constraint: constraint.to_owned(),
        labels: Vec::new(),
    }
}

fn global(module: &str, name: &str, span: Span) -> Expr {
    Expr::new(
        ExprKind::Global {
            module: module.to_owned(),
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
