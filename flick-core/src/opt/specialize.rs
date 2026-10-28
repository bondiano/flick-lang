//! Specialization over the closed numeric classes. A function called with
//! instances of `Numeric`, `Integer` or `Floating` gets a copy in the calling
//! module for them, whether it is of this module or another of the build,
//! and a method of a known numeric instance becomes the operator of its
//! type: the copy is then as plain as code written for that type, and small
//! enough to inline. The classes are closed, so the copies are finitely many.

use std::collections::{HashMap, HashSet};

use crate::ir::{BinOp, Expr, ExprKind, Item, ItemKind, Lit, Module};
use crate::numeric::{Arith, Num};
use crate::source::Span;
use crate::types::Dict;

use super::Generics;

const NUMERIC: &str = "Numeric";

/// A generic function by module and name: its dictionary parameters and body.
type Generic = (Vec<String>, Expr);

/// A copy still to make, for `dicts`, named `copy`; one of another module
/// takes the place of its first call, `at`, as the source of its code.
struct Pending {
    generic: Generic,
    dicts: Vec<Dict>,
    copy: String,
    at: Option<Span>,
}

fn respan(e: &mut Expr, span: Span) {
    e.span = span;
    e.each_child_mut(&mut |c| respan(c, span));
}

/// Copies the generic functions `module` calls with numeric instances, its
/// own and those `generics` offers, until no call is left to copy for.
pub fn specialize(module: &mut Module, generics: Generics) {
    let own: HashMap<String, Generic> = module
        .items
        .iter()
        .filter_map(|item| match &item.kind {
            ItemKind::Value { name, dicts, body } if !dicts.is_empty() => {
                Some((name.clone(), (dicts.clone(), body.clone())))
            }
            _ => None,
        })
        .collect();
    let mut found: HashMap<(String, String), Option<Generic>> = HashMap::new();
    let mut find = |m: &str, name: &str| -> Option<Generic> {
        if m == module.name {
            return own.get(name).cloned();
        }
        found
            .entry((m.to_owned(), name.to_owned()))
            .or_insert_with(|| generics(m, name))
            .clone()
    };
    let mut made: HashSet<String> = HashSet::new();
    let mut pending: Vec<Pending> = Vec::new();
    let this = module.name.clone();
    for item in &mut module.items {
        for body in super::bodies(item) {
            calls(body, &this, &mut find, &mut made, &mut pending);
        }
    }
    // a copy may call another generic function, or itself, with instances
    while let Some(Pending {
        generic: (params, body),
        dicts,
        copy,
        at,
    }) = pending.pop()
    {
        let given: Vec<(&str, &Dict)> = params.iter().map(String::as_str).zip(&dicts).collect();
        let mut body = body.clone();
        if let Some(span) = at {
            respan(&mut body, span);
        }
        instantiate(&mut body, &given);
        calls(&mut body, &this, &mut find, &mut made, &mut pending);
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

/// The parameters and body of the function `name` of `module` when it
/// takes dictionaries and its body names only what `exported` allows of
/// `module`, so a copy in another module can reach all it uses.
pub fn names_only(module: &Module, name: &str, exported: &dyn Fn(&str) -> bool) -> Option<Generic> {
    let (dicts, body) = module.items.iter().find_map(|item| match &item.kind {
        ItemKind::Value {
            name: n,
            dicts,
            body,
        } if n == name && !dicts.is_empty() => Some((dicts, body)),
        _ => None,
    })?;
    let mut reachable = true;
    visit(body, &mut |e| {
        if let ExprKind::Global { module: m, name } = &e.kind {
            reachable &= *m != module.name || exported(name);
        }
    });
    reachable.then(|| (dicts.clone(), body.clone()))
}

fn visit<'a>(e: &'a Expr, f: &mut dyn FnMut(&'a Expr)) {
    f(e);
    e.each_child(&mut |c| visit(c, f));
}

/// Points each call of a generic function with numeric instances at its
/// copy for them in `module`, recording a copy still to make.
fn calls(
    e: &mut Expr,
    module: &str,
    find: &mut dyn FnMut(&str, &str) -> Option<Generic>,
    made: &mut HashSet<String>,
    pending: &mut Vec<Pending>,
) {
    e.each_child_mut(&mut |c| calls(c, module, find, made, pending));
    let ExprKind::Dicts(inner, dicts) = &e.kind else {
        return;
    };
    let ExprKind::Global { module: m, name } = &inner.kind else {
        return;
    };
    let Some(dicts) = dicts.iter().map(known).collect::<Option<Vec<Dict>>>() else {
        return;
    };
    let Some(generic) = find(m, name) else {
        return;
    };
    let suffix: Vec<String> = dicts.iter().map(instance_suffix).collect();
    let owner = if m == module {
        String::new()
    } else {
        format!("{}$", m.replace('.', "$"))
    };
    let copy = format!("{owner}{name}${}", suffix.join("$"));
    let span = inner.span;
    if made.insert(copy.clone()) {
        let at = (m != module).then_some(span);
        pending.push(Pending {
            generic,
            dicts,
            copy: copy.clone(),
            at,
        });
    }
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
                Some((num, name)) => {
                    binary(name, num).map(|op| ExprKind::Binary(op, lhs.clone(), arg.clone()))
                }
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

/// The operator a binary method of `Numeric` is on `num`.
fn binary(method: &str, num: Num) -> Option<BinOp> {
    let arith = |a| BinOp::Arith(a, num);
    Some(match method {
        "add" => arith(Arith::Add),
        "sub" => arith(Arith::Sub),
        "mul" => arith(Arith::Mul),
        "div" => arith(Arith::Div),
        "rem" => arith(Arith::Rem),
        "pow" => arith(Arith::Pow),
        "lt" => BinOp::Lt,
        "le" => BinOp::Le,
        "gt" => BinOp::Gt,
        "ge" => BinOp::Ge,
        _ => return None,
    })
}

/// `from_int` or `from_float` of a literal, as a literal of the type.
fn literal(num: Num, arg: &Expr) -> Option<ExprKind> {
    let ExprKind::Lit(lit) = &arg.kind else {
        return None;
    };
    Some(ExprKind::Lit(match (num, lit) {
        (Num::Int, Lit::Int(n)) => Lit::Int(*n),
        (Num::Float, Lit::Int(n)) => Lit::Float((*n).into()),
        (Num::BigInt, Lit::Int(n)) => Lit::BigInt(n.to_string()),
        (Num::Float, Lit::Float(x)) => Lit::Float(*x),
        _ => return None,
    }))
}
