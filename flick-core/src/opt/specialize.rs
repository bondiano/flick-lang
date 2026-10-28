//! Specialization over the closed numeric classes. A function of the module
//! called with instances of `Numeric`, `Integer` or `Floating` gets a copy
//! for them, and a method of a known numeric instance becomes the operator
//! of its type: the copy is then as plain as code written for that type,
//! and small enough to inline. The classes are closed, so the copies are
//! finitely many.

use std::collections::{HashMap, HashSet};

use crate::ir::{BinOp, Expr, ExprKind, Item, ItemKind, Lit, Module};
use crate::numeric::{Arith, Num};
use crate::types::Dict;

const NUMERIC: &str = "Numeric";

/// Copies the generic functions of `module` for the numeric instances they
/// are called with, until no call is left to copy for.
pub fn specialize(module: &mut Module) {
    let generic: HashMap<String, (Vec<String>, Expr)> = module
        .items
        .iter()
        .filter_map(|item| match &item.kind {
            ItemKind::Value { name, dicts, body } if !dicts.is_empty() => {
                Some((name.clone(), (dicts.clone(), body.clone())))
            }
            _ => None,
        })
        .collect();
    if generic.is_empty() {
        return;
    }
    let mut made: HashSet<String> = HashSet::new();
    let mut pending: Vec<(String, Vec<Dict>, String)> = Vec::new();
    for item in &mut module.items {
        for body in super::bodies(item) {
            calls(body, &module.name, &generic, &mut made, &mut pending);
        }
    }
    // a copy may call another generic function, or itself, with instances
    while let Some((name, dicts, copy)) = pending.pop() {
        let (params, body) = &generic[&name];
        let given: Vec<(&str, &Dict)> = params.iter().map(String::as_str).zip(&dicts).collect();
        let mut body = body.clone();
        instantiate(&mut body, &given);
        calls(&mut body, &module.name, &generic, &mut made, &mut pending);
        let span = body.span;
        module.items.push(Item {
            kind: ItemKind::Value {
                name: copy,
                dicts: Vec::new(),
                body,
            },
            span,
        });
    }
}

/// Points each call of a generic function of `module` with numeric instances
/// at its copy for them, recording a copy still to make.
fn calls(
    e: &mut Expr,
    module: &str,
    generic: &HashMap<String, (Vec<String>, Expr)>,
    made: &mut HashSet<String>,
    pending: &mut Vec<(String, Vec<Dict>, String)>,
) {
    e.each_child_mut(&mut |c| calls(c, module, generic, made, pending));
    let ExprKind::Dicts(inner, dicts) = &e.kind else {
        return;
    };
    let ExprKind::Global { module: m, name } = &inner.kind else {
        return;
    };
    if m != module || !generic.contains_key(name) {
        return;
    }
    let Some(dicts) = dicts.iter().map(known).collect::<Option<Vec<Dict>>>() else {
        return;
    };
    let suffix: Vec<String> = dicts.iter().map(instance_suffix).collect();
    let copy = format!("{name}${}", suffix.join("$"));
    if made.insert(copy.clone()) {
        pending.push((name.clone(), dicts, copy.clone()));
    }
    let span = inner.span;
    *e = Expr::new(
        ExprKind::Global {
            module: module.to_owned(),
            name: copy,
        },
        span,
    );
}

/// `d` as a numeric instance without context, a superclass of one included.
fn known(d: &Dict) -> Option<Dict> {
    match d {
        Dict::Instance { module, args, .. } if module == NUMERIC && args.is_empty() => {
            Some(d.clone())
        }
        Dict::Super { dict, class } => {
            let Dict::Instance { name, .. } = known(dict)? else {
                return None;
            };
            let num = name.rsplit(' ').next()?;
            let class = class.rsplit('.').next().unwrap_or(class);
            Some(Dict::Instance {
                module: NUMERIC.to_owned(),
                name: format!("{class} {num}"),
                args: Vec::new(),
            })
        }
        _ => None,
    }
}

fn instance_suffix(d: &Dict) -> String {
    match d {
        Dict::Instance { name, .. } => name.replace(' ', "_"),
        _ => String::new(),
    }
}

/// `e` with each dictionary parameter in `given` replaced by its instance,
/// and each method of a numeric instance by the operator of its type.
fn instantiate(e: &mut Expr, given: &[(&str, &Dict)]) {
    e.each_child_mut(&mut |c| instantiate(c, given));
    match &mut e.kind {
        ExprKind::Method { dict, .. } => *dict = resolve(dict, given),
        ExprKind::Dicts(_, dicts) => {
            for d in dicts.iter_mut() {
                *d = resolve(d, given);
            }
        }
        _ => {}
    }
    operator(e);
}

fn resolve(d: &Dict, given: &[(&str, &Dict)]) -> Dict {
    let d = match d {
        Dict::Given(name) => given
            .iter()
            .find(|(n, _)| n == name)
            .map_or_else(|| d.clone(), |(_, inst)| (*inst).clone()),
        Dict::Super { dict, class } => Dict::Super {
            dict: Box::new(resolve(dict, given)),
            class: class.clone(),
        },
        Dict::Instance { module, name, args } => Dict::Instance {
            module: module.clone(),
            name: name.clone(),
            args: args.iter().map(|a| resolve(a, given)).collect(),
        },
        _ => d.clone(),
    };
    known(&d).unwrap_or(d)
}

/// A full application of a method of a numeric instance as the operator of
/// its type; whether it was one.
pub fn operator(e: &mut Expr) -> bool {
    let span = e.span;
    let replaced = match &e.kind {
        ExprKind::App { func, arg, .. } => match &func.kind {
            ExprKind::App {
                func: method,
                arg: lhs,
                ..
            } => match method_of(method) {
                Some((num, name)) => arith(name)
                    .map(|op| ExprKind::Binary(BinOp::Arith(op, num), lhs.clone(), arg.clone())),
                None => None,
            },
            _ => match method_of(func) {
                Some((num, "negate")) => Some(ExprKind::Neg(num, arg.clone())),
                Some((num, "from_int" | "from_float")) => literal(num, arg),
                _ => None,
            },
        },
        _ => None,
    };
    let changed = replaced.is_some();
    if let Some(kind) = replaced {
        *e = Expr::new(kind, span);
    }
    changed
}

/// The numeric type and method name of a method of a numeric instance.
fn method_of(e: &Expr) -> Option<(Num, &str)> {
    let ExprKind::Method {
        dict:
            Dict::Instance {
                module,
                name: instance,
                args,
            },
        name,
    } = &e.kind
    else {
        return None;
    };
    if module != NUMERIC || !args.is_empty() {
        return None;
    }
    Some((Num::of(instance.rsplit(' ').next()?)?, name.as_str()))
}

fn arith(method: &str) -> Option<Arith> {
    Some(match method {
        "add" => Arith::Add,
        "sub" => Arith::Sub,
        "mul" => Arith::Mul,
        "div" => Arith::Div,
        "rem" => Arith::Rem,
        "pow" => Arith::Pow,
        _ => return None,
    })
}

/// `from_int` or `from_float` of a literal, as a literal of the type.
fn literal(num: Num, arg: &Expr) -> Option<ExprKind> {
    let ExprKind::Lit(Lit::Number(text)) = &arg.kind else {
        return None;
    };
    Some(ExprKind::Lit(match num {
        Num::BigInt => Lit::BigInt(format!("{text}n")),
        Num::Int | Num::Float => Lit::Number(text.clone()),
    }))
}
