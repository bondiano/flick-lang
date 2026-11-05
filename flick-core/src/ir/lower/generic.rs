//! The methods of a `deriving Generic` instance, from its [`GenericInfo`]:
//! `to_rep` matches the type's constructors and builds each one's `Con` of
//! its `Field`s in pairs, `Inl`/`Inr` before it; `from_rep` matches the
//! representation and applies the constructor.

use crate::ir::{Arm, Expr, ExprKind, Lit, Param, Pat, Stage};
use crate::source::Span;
use crate::types::{Constructor, GenericInfo};

/// The methods of class `Generic`, as its module declares them.
const TO_REP: &str = "to_rep";
const FROM_REP: &str = "from_rep";

/// `to_rep` and `from_rep` of `info`, their code at `span`.
pub fn methods(info: &GenericInfo, span: Span) -> Vec<(String, Expr)> {
    let at = |kind| Expr::new(kind, span);
    let apply = |c: &Constructor, args: Vec<Expr>| {
        (args.into_iter()).fold(at(ExprKind::Con(c.clone())), |func, arg| {
            at(ExprKind::App {
                func: Box::new(func),
                arg: Box::new(arg),
                stage: Stage::Pure,
                arg_first: false,
            })
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
    let n = info.cases.len();
    let mut to = Vec::new();
    let mut from = Vec::new();
    for (k, (case, labels)) in info.cases.iter().enumerate() {
        let labels: &Option<Vec<String>> = labels;
        let count = labels.as_ref().map_or(case.arity, Vec::len);
        let locals: Vec<String> = (0..count).map(|i| format!("f{i}")).collect();
        // the fields in pairs ended by `()`, and the pattern of the same
        let fields = locals
            .iter()
            .rev()
            .fold(at(ExprKind::Lit(Lit::Unit)), |rest, x| {
                let one = apply(&info.field, vec![at(ExprKind::Local(x.clone()))]);
                at(ExprKind::Tuple(vec![one, rest]))
            });
        let shape = locals.iter().rev().fold(Pat::Wild, |rest, x| {
            Pat::Tuple(vec![
                Pat::Con(info.field.clone(), vec![Pat::Bind(x.clone())]),
                rest,
            ])
        });
        let (taken, rebuilt) = match labels {
            Some(labels) => {
                let bound = labels
                    .iter()
                    .zip(&locals)
                    .map(|(l, x)| (l.clone(), Pat::Bind(x.clone())));
                let record = labels.iter().zip(&locals);
                let record = record.map(|(l, x)| (l.clone(), at(ExprKind::Local(x.clone()))));
                (
                    vec![Pat::Record(bound.collect())],
                    vec![at(ExprKind::Record(record.collect()))],
                )
            }
            None => (
                locals.iter().map(|x| Pat::Bind(x.clone())).collect(),
                locals
                    .iter()
                    .map(|x| at(ExprKind::Local(x.clone())))
                    .collect(),
            ),
        };
        // `Inr` k times, and `Inl` but at the last
        let mut value = apply(&info.con, vec![fields]);
        let mut pat = Pat::Con(info.con.clone(), vec![shape]);
        if k + 1 < n {
            value = apply(&info.inl, vec![value]);
            pat = Pat::Con(info.inl.clone(), vec![pat]);
        }
        for _ in 0..k {
            value = apply(&info.inr, vec![value]);
            pat = Pat::Con(info.inr.clone(), vec![pat]);
        }
        to.push(Arm {
            pat: Pat::Con(case.clone(), taken),
            guard: None,
            body: value,
        });
        from.push(Arm {
            pat,
            guard: None,
            body: apply(case, rebuilt),
        });
    }
    let matching = |x: &str, arms| {
        at(ExprKind::Match(
            Box::new(at(ExprKind::Local(x.to_owned()))),
            arms,
        ))
    };
    vec![
        (TO_REP.to_owned(), lambda("value", matching("value", to))),
        (FROM_REP.to_owned(), lambda("rep", matching("rep", from))),
    ]
}
