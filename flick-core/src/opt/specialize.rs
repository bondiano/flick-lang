//! Specialization over the closed numeric classes. A function called with
//! instances of `Numeric`, `Integer` or `Floating` gets a copy in the calling
//! module for them, whether it is of this module or another of the build,
//! and a method of a known numeric instance becomes the operator of its
//! type: the copy is then as plain as code written for that type, and small
//! enough to inline. The classes are closed, so the copies are finitely many.

use std::collections::{HashMap, HashSet};

use super::walk::respan;
use crate::ir::{BinOp, Expr, ExprKind, Item, ItemKind, Lit, Module};
use crate::numeric::{Arith, Num};
use crate::source::Span;
use crate::types::Dict;

use super::Generics;
use crate::types::lang::{self, Lang};

/// The module of the numeric classes and their instances.
fn numeric() -> &'static str {
    &lang::item(Lang::Numeric).module
}

/// The methods of the numeric classes the specializer turns into operators.
const METHODS: [Lang; 13] = [
    Lang::Add,
    Lang::Sub,
    Lang::Mul,
    Lang::Div,
    Lang::Rem,
    Lang::Pow,
    Lang::Lt,
    Lang::Le,
    Lang::Gt,
    Lang::Ge,
    Lang::Negate,
    Lang::FromInt,
    Lang::FromFloat,
];

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
            .or_insert_with(|| generics(m, name).filter(|(dicts, _)| !dicts.is_empty()))
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

/// The dictionary parameters and body of the value `name` of `module`, or
/// of the method `<instance>#<method>` and the dictionaries of its
/// instance's context, when its body names only what `exported` allows of
/// `module`, so a copy in another module can reach all it uses.
pub fn names_only(module: &Module, name: &str, exported: &dyn Fn(&str) -> bool) -> Option<Generic> {
    // `<instance>#<method>`: a method of an instance, over its context
    let method = name.split_once('#');
    let (dicts, body) = module
        .items
        .iter()
        .find_map(|item| match (&item.kind, method) {
            (
                ItemKind::Value {
                    name: n,
                    dicts,
                    body,
                },
                None,
            ) if n == name => Some((dicts, body)),
            (ItemKind::Instance { info, methods }, Some((instance, method)))
                if info.name == instance =>
            {
                let (_, body) = methods.iter().find(|(m, _)| m == method)?;
                Some((&info.context, body))
            }
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
        Dict::Instance { module, args, .. } if module == numeric() && args.is_empty() => {
            Some(d.clone())
        }
        Dict::Super { dict, class } => {
            let Dict::Instance { name, .. } = known(dict)? else {
                return None;
            };
            let num = name.rsplit(' ').next()?;
            let class = class.rsplit('.').next().unwrap_or(class);
            Some(Dict::Instance {
                module: numeric().to_owned(),
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
pub(super) fn instantiate(e: &mut Expr, given: &[(&str, &Dict)]) {
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
                Some((num, Lang::Negate)) => Some(ExprKind::Neg(num, arg.clone())),
                Some((num, Lang::FromInt | Lang::FromFloat)) => literal(num, arg),
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
fn method_of(e: &Expr) -> Option<(Num, Lang)> {
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
    if module != numeric() || !args.is_empty() {
        return None;
    }
    let method = METHODS.into_iter().find(|&m| lang::item(m).name == *name)?;
    Some((Num::of(instance.rsplit(' ').next()?)?, method))
}

/// The operator a binary method of `Numeric` is on `num`.
fn binary(method: Lang, num: Num) -> Option<BinOp> {
    let arith = |a| BinOp::Arith(a, num);
    Some(match method {
        Lang::Add => arith(Arith::Add),
        Lang::Sub => arith(Arith::Sub),
        Lang::Mul => arith(Arith::Mul),
        Lang::Div => arith(Arith::Div),
        Lang::Rem => arith(Arith::Rem),
        Lang::Pow => arith(Arith::Pow),
        Lang::Lt => BinOp::Lt,
        Lang::Le => BinOp::Le,
        Lang::Gt => BinOp::Gt,
        Lang::Ge => BinOp::Ge,
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
