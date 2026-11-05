//! The companions of an enum, as IR every backend runs: `T$value` matches
//! a value to its literal, `T$parse` a literal to its value or an error,
//! `T$all` lists the values; and the methods of its instance of `Enum`,
//! which name them.

use crate::ir::{Arm, Expr, ExprKind, Item, ItemKind, Lit, Param, Pat, Stage};
use crate::source::Span;
use crate::types::lang::{self, Lang};
use crate::types::{Constructor, EnumLit, Rep, companion_name};

/// The methods of class `Enum`, as its module declares them.
const METHODS: [&str; 3] = ["value", "parse", "all"];

/// `T$value`, `T$parse` and `T$all` of the enum `ty`, whose constructors
/// are `constructors`, at `span`.
pub fn companions(ty: &str, constructors: &[Constructor], span: Span) -> Vec<Item> {
    let at = |kind| Expr::new(kind, span);
    let apply = |c: Constructor, arg: Expr| {
        at(ExprKind::App {
            func: Box::new(at(ExprKind::Con(c))),
            arg: Box::new(arg),
            stage: Stage::Pure,
            arg_first: false,
        })
    };
    let lambda = |param: &str, body: Expr| {
        let pat = Pat::Bind(param.to_owned());
        let params = vec![Param {
            pat,
            stage: Stage::Pure,
            ty: None,
        }];
        at(ExprKind::Lambda {
            params,
            body: Box::new(body),
        })
    };
    let local = |x: &str| at(ExprKind::Local(x.to_owned()));
    let literals: Vec<(&Constructor, Lit)> = (constructors.iter())
        .filter_map(|c| match &c.rep {
            Rep::Literal(EnumLit::String(s)) => Some((c, Lit::String(s.clone()))),
            Rep::Literal(EnumLit::Int(n)) => Some((c, Lit::Int(*n))),
            _ => None,
        })
        .collect();
    let value = literals.iter().map(|(c, lit)| Arm {
        pat: Pat::Con((*c).clone(), Vec::new()),
        guard: None,
        body: at(ExprKind::Lit(lit.clone())),
    });
    let ok = native(Lang::Ok, Lang::Result);
    let err = native(Lang::Err, Lang::Result);
    let parsed = literals.iter().map(|(c, lit)| Arm {
        pat: Pat::Lit(lit.clone()),
        guard: None,
        body: apply(ok.clone(), at(ExprKind::Con((*c).clone()))),
    });
    // the issue names no value, as every decoding error
    let issue = at(ExprKind::Record(vec![
        (
            "message".to_owned(),
            at(ExprKind::Lit(Lit::String(format!("not a valid {ty}")))),
        ),
        (
            "path".to_owned(),
            at(ExprKind::Lit(Lit::String(String::new()))),
        ),
    ]));
    let error = apply(err, apply(schema_error(), at(ExprKind::List(vec![issue]))));
    let otherwise = Arm {
        pat: Pat::Wild,
        guard: None,
        body: error,
    };
    let matching = |x: &str, arms: Vec<Arm>| at(ExprKind::Match(Box::new(local(x)), arms));
    let all = (literals.iter())
        .map(|(c, _)| at(ExprKind::Con((*c).clone())))
        .collect();
    let item = |member: &str, body: Expr| Item {
        kind: ItemKind::Value {
            name: companion_name(ty, member),
            dicts: Vec::new(),
            body,
        },
        span,
    };
    vec![
        item("value", lambda("value", matching("value", value.collect()))),
        item(
            "parse",
            lambda(
                "literal",
                matching("literal", parsed.chain([otherwise]).collect()),
            ),
        ),
        item("all", at(ExprKind::List(all))),
    ]
}

/// The methods of the instance of `Enum` of the enum `ty` of `module`: its
/// companions.
pub fn methods(module: &str, ty: &str, span: Span) -> Vec<(String, Expr)> {
    let global = |member: &str| ExprKind::Global {
        module: module.to_owned(),
        name: companion_name(ty, member),
    };
    (METHODS.iter())
        .map(|m| ((*m).to_owned(), Expr::new(global(m), span)))
        .collect()
}

/// The constructor `lang` of the type `ty`, both lang items, of one field.
fn native(lang: Lang, ty: Lang) -> Constructor {
    let item = lang::item(lang);
    Constructor {
        module: item.module.clone(),
        ty: lang::item(ty).name.clone(),
        name: item.name.clone(),
        arity: 1,
        rep: Rep::Tagged,
        dicts: 0,
    }
}

/// `SchemaError`, the error and its one constructor.
fn schema_error() -> Constructor {
    let item = lang::item(Lang::SchemaError);
    Constructor {
        module: item.module.clone(),
        ty: item.name.clone(),
        name: item.name.clone(),
        arity: 1,
        rep: Rep::Tagged,
        dicts: 0,
    }
}
