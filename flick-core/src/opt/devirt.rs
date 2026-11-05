//! Devirtualization: the code of known instances in place of their
//! dictionaries. A default an instance takes becomes a method of its own
//! when, with the instance and its default signature's dictionaries known,
//! its code is a function or data; a method of a known instance is its code
//! where that is small data, or a function that takes a known constructor
//! apart; and a `match` of a known constructor is the arm it takes. So the
//! instance `deriving` gives through `Generic` is the one written by hand.

use std::collections::HashMap;
use std::mem;

use super::Generics;
use super::specialize::instantiate;
use super::walk::{applied, rename, size};
use crate::ir::{Arm, Expr, ExprKind, ItemKind, Lit, Module, Pat, Stmt};
use crate::types::{Dict, default_method};

/// The size budget of data copied where a method of a known instance is.
const DATA: usize = 32;

/// How deep methods of known instances are followed into each other.
const DEPTH: usize = 16;

/// The code of methods of known instances: of this module, as it is, and of
/// the others through `generics` (`"<instance>#<method>"`).
pub struct Lookup<'a> {
    module: String,
    own: HashMap<(String, String), (Vec<String>, Expr)>,
    values: HashMap<String, (Vec<String>, Expr)>,
    generics: Generics<'a>,
}

impl<'a> Lookup<'a> {
    pub fn new(module: &Module, generics: Generics<'a>) -> Self {
        let mut own = HashMap::new();
        let mut values = HashMap::new();
        for item in &module.items {
            match &item.kind {
                ItemKind::Instance { info, methods } => {
                    for (m, code) in methods {
                        let key = (info.name.clone(), m.clone());
                        own.insert(key, (info.context.clone(), code.clone()));
                    }
                }
                ItemKind::Value { name, dicts, body } if !dicts.is_empty() => {
                    values.insert(name.clone(), (dicts.clone(), body.clone()));
                }
                _ => {}
            }
        }
        Lookup {
            module: module.name.clone(),
            own,
            values,
            generics,
        }
    }

    /// The value `name` of `module` that takes dictionaries.
    fn value(&self, module: &str, name: &str) -> Option<(Vec<String>, Expr)> {
        if module == self.module {
            return self.values.get(name).cloned();
        }
        (self.generics)(module, name)
    }

    /// The code of `method` of the instance `dict`, its context given.
    fn method(&self, dict: &Dict, method: &str) -> Option<Expr> {
        let Dict::Instance { module, name, args } = dict else {
            return None;
        };
        let (context, mut code) = if *module == self.module {
            self.own.get(&(name.clone(), method.to_owned())).cloned()?
        } else {
            (self.generics)(module, &format!("{name}#{method}"))?
        };
        // a method with constraints of its own takes their dictionaries
        if matches!(code.kind, ExprKind::Abstract(..)) || context.len() != args.len() {
            return None;
        }
        let given: Vec<(&str, &Dict)> = context.iter().map(String::as_str).zip(args).collect();
        instantiate(&mut code, &given);
        Some(code)
    }
}

/// Makes each default an instance of `module` takes its own method when its
/// code, instantiated, is a function or data: a lazy default that is
/// neither stays one, or it would be an eager use of its own dictionary.
pub fn defaults(module: &mut Module, lookup: &Lookup, fresh: &mut usize) {
    for item in &mut module.items {
        let ItemKind::Instance { info, methods: own } = &mut item.kind else {
            continue;
        };
        let (class_module, class) = info.class.clone();
        let this = Dict::Instance {
            module: info.module.clone(),
            name: info.name.clone(),
            args: info
                .context
                .iter()
                .map(|c| Dict::Given(c.clone()))
                .collect(),
        };
        let mut kept = Vec::new();
        for (name, n, extra) in mem::take(&mut info.defaults) {
            let made = (n == 0)
                .then(|| lookup.value(&class_module, &default_method(&class, &name)))
                .flatten()
                .and_then(|(params, code)| {
                    let dicts: Vec<Dict> =
                        [this.clone()].into_iter().chain(extra.clone()).collect();
                    (params.len() == dicts.len()).then_some((params, code, dicts))
                })
                .and_then(|(params, mut code, dicts)| {
                    *fresh += 1;
                    rename(&mut code, &format!("$v{fresh}"));
                    let given: Vec<(&str, &Dict)> =
                        params.iter().map(String::as_str).zip(&dicts).collect();
                    instantiate(&mut code, &given);
                    methods(&mut code, lookup, fresh, 0);
                    super::simplify(&mut code);
                    (matches!(code.kind, ExprKind::Lambda { .. }) || data(&code)).then_some(code)
                });
            match made {
                Some(code) => own.push((name, code)),
                None => kept.push((name, n, extra)),
            }
        }
        info.defaults = kept;
    }
}

/// Puts the code of methods of known instances in `e` where it is small
/// data, or a function of one parameter matching it applied to a known
/// constructor; whether anything changed.
pub fn methods(e: &mut Expr, lookup: &Lookup, fresh: &mut usize, depth: usize) -> bool {
    let mut changed = false;
    e.each_child_mut(&mut |c| changed |= methods(c, lookup, fresh, depth));
    if depth > DEPTH {
        return changed;
    }
    if let ExprKind::Method { dict, name } = &e.kind
        && let Some(mut code) = lookup.method(dict, name)
    {
        methods(&mut code, lookup, fresh, depth + 1);
        if data(&code) && size(&code) <= DATA {
            code.span = e.span;
            *e = code;
            return true;
        }
    }
    // a value with a shape to take apart, not a name of one
    if let ExprKind::App { func, arg, .. } = &mut e.kind
        && data(arg)
        && matches!(
            arg.kind,
            ExprKind::App { .. } | ExprKind::Tuple(_) | ExprKind::Record(_)
        )
        && let ExprKind::Method { dict, name } = &func.kind
        && let Some(mut code) = lookup.method(dict, name)
        && takes_apart(&code)
    {
        *fresh += 1;
        rename(&mut code, &format!("$m{fresh}"));
        **func = code;
        return true;
    }
    changed
}

/// `fun x -> match x with …`.
fn takes_apart(code: &Expr) -> bool {
    let ExprKind::Lambda { params, body } = &code.kind else {
        return false;
    };
    let ([param], ExprKind::Match(scrutinee, _)) = (&params[..], &body.kind) else {
        return false;
    };
    matches!((&param.pat, &scrutinee.kind), (Pat::Bind(x), ExprKind::Local(y)) if x == y)
}

/// Whether `e` is a value built of constructors, literals and locals: pure,
/// and known where it is built.
fn data(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Lit(_) | ExprKind::Con(_) | ExprKind::Local(_) => true,
        ExprKind::Tuple(items) | ExprKind::List(items) => items.iter().all(data),
        ExprKind::Record(fields) => fields.iter().all(|(_, v)| data(v)),
        ExprKind::App { .. } => applied(e).is_some_and(|(_, args)| args.iter().all(|a| data(a))),
        _ => false,
    }
}

/// A `match` of data: the arm it takes, its names bound to the parts;
/// whether it was one.
pub fn known_match(e: &mut Expr) -> bool {
    let ExprKind::Match(scrutinee, arms) = &e.kind else {
        return false;
    };
    if !data(scrutinee) {
        return false;
    }
    for Arm { pat, guard, body } in arms {
        if guard.is_some() {
            return false;
        }
        let mut binds = Vec::new();
        match matches(pat, scrutinee, &mut binds) {
            Some(true) => {
                let stmts = (binds.into_iter())
                    .map(|(x, v)| Stmt::Let(Pat::Bind(x), v))
                    .collect();
                *e = Expr::new(ExprKind::Block(stmts, Box::new(body.clone())), e.span);
                return true;
            }
            Some(false) => {}
            None => return false,
        }
    }
    false
}

/// Whether `pat` matches the data `e`, binding its names; `None` when that
/// is not known here.
fn matches(pat: &Pat, e: &Expr, binds: &mut Vec<(String, Expr)>) -> Option<bool> {
    match (pat, &e.kind) {
        (Pat::Wild, _) => Some(true),
        (Pat::Bind(x), _) => {
            binds.push((x.clone(), e.clone()));
            Some(true)
        }
        (Pat::Tuple(pats), ExprKind::Tuple(items)) if pats.len() == items.len() => {
            all(pats.iter().zip(items), binds)
        }
        (Pat::Record(fields), ExprKind::Record(values)) => {
            let pairs: Option<Vec<(&Pat, &Expr)>> = (fields.iter())
                .map(|(l, p)| values.iter().find(|(m, _)| m == l).map(|(_, v)| (p, v)))
                .collect();
            all(pairs?.into_iter(), binds)
        }
        (Pat::Con(c, pats), _) => {
            let (d, args) = applied(e)?;
            if (d.module.as_str(), d.ty.as_str()) != (c.module.as_str(), c.ty.as_str()) {
                return None;
            }
            if d.name != c.name {
                return Some(false);
            }
            (pats.len() == args.len()).then_some(())?;
            all(pats.iter().zip(args), binds)
        }
        (Pat::Lit(Lit::Bool(b)), ExprKind::Lit(Lit::Bool(v))) => Some(b == v),
        _ => None,
    }
}

fn all<'p>(
    pairs: impl Iterator<Item = (&'p Pat, &'p Expr)>,
    binds: &mut Vec<(String, Expr)>,
) -> Option<bool> {
    for (p, v) in pairs {
        if !matches(p, v, binds)? {
            return Some(false);
        }
    }
    Some(true)
}

#[cfg(test)]
#[path = "devirt_tests.rs"]
mod tests;
