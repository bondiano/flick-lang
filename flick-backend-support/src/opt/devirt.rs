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
use super::specialize::{Generic, instantiate};
use super::types::{match_type, subst_types};
use super::walk::{applied, rename, respan, size};
use crate::ir::{Arm, Expr, ExprKind, ItemKind, Lit, Module, Pat, Stmt};
use flick_ir::{Dict, Span, TypeRef, default_method};

/// The size budget of data copied where a method of a known instance is.
const DATA: usize = 32;

/// How deep methods of known instances are followed into each other.
const DEPTH: usize = 16;

/// The code of methods of known instances: of this module, as it is, and of
/// the others through `generics` (`"<instance>#<method>"`).
pub struct Lookup<'a> {
    module: String,
    own: HashMap<(String, String), (Vec<String>, Expr)>,
    /// The defaults instances of this module take, without dictionaries of
    /// their own: the context, the class and those of the default signature.
    taken: HashMap<(String, String), Taken>,
    values: HashMap<String, Generic>,
    generics: Generics<'a>,
    lang: &'a crate::Known<'a>,
}

struct Taken {
    context: Vec<String>,
    class: (String, String),
    extra: Vec<Dict>,
}

impl<'a> Lookup<'a> {
    pub fn new(module: &Module, generics: Generics<'a>, lang: &'a crate::Known<'a>) -> Self {
        let mut own = HashMap::new();
        let mut taken = HashMap::new();
        let mut values = HashMap::new();
        for item in &module.items {
            match &item.kind {
                ItemKind::Instance { info, methods } => {
                    for (m, code) in methods {
                        let key = (info.name.clone(), m.clone());
                        own.insert(key, (info.context.clone(), code.clone()));
                    }
                    for (m, _, extra) in info.defaults.iter().filter(|(_, n, _)| *n == 0) {
                        let entry = Taken {
                            context: info.context.clone(),
                            class: info.class.clone(),
                            extra: extra.clone(),
                        };
                        taken.insert((info.name.clone(), m.clone()), entry);
                    }
                }
                ItemKind::Value {
                    name,
                    type_params,
                    dicts,
                    dict_constraints,
                    body,
                    ..
                } if !dicts.is_empty() => {
                    values.insert(
                        name.clone(),
                        Generic {
                            type_params: type_params.clone(),
                            dicts: dicts.clone(),
                            dict_constraints: dict_constraints.clone(),
                            body: body.clone(),
                        },
                    );
                }
                _ => {}
            }
        }
        Lookup {
            module: module.name.clone(),
            own,
            taken,
            values,
            generics,
            lang,
        }
    }

    /// The value `name` of `module` that takes dictionaries.
    fn value(&self, module: &str, name: &str) -> Option<Generic> {
        if module == self.module {
            return self.values.get(name).cloned();
        }
        (self.generics)(module, name)
    }

    /// The context dictionary parameters and raw body of `method` on `module.instance`.
    pub(super) fn raw_method(
        &self,
        module: &str,
        instance: &str,
        method: &str,
    ) -> Option<(Vec<String>, Expr)> {
        if module != self.module {
            let g = (self.generics)(module, &format!("{instance}#{method}"))?;
            return Some((g.dicts, g.body));
        }
        let key = (instance.to_owned(), method.to_owned());
        if let Some(own) = self.own.get(&key) {
            return Some(own.clone());
        }
        let taken = self.taken.get(&key)?;
        let dict = Dict::Instance {
            module: module.to_owned(),
            name: instance.to_owned(),
            args: Vec::new(),
        };
        Some((taken.context.clone(), self.default(&dict, method, taken)?))
    }

    /// The code of `method` of the instance `dict`, its context given: its
    /// own, or the default it takes; code of another module, the instance's
    /// or the class's of the default, at `at`, as its spans are of its file.
    fn method(&self, dict: &Dict, method: &str, at: Span) -> Option<Expr> {
        let Dict::Instance { module, name, args } = dict else {
            return None;
        };
        let key = (name.clone(), method.to_owned());
        // the code of a default is of the class's module, whatever the instance's
        let mut elsewhere = *module != self.module;
        let (context, mut code) = if elsewhere {
            let g = (self.generics)(module, &format!("{name}#{method}"))?;
            (g.dicts, g.body)
        } else if let Some(own) = self.own.get(&key) {
            own.clone()
        } else {
            let taken = self.taken.get(&key)?;
            elsewhere = taken.class.0 != self.module;
            (taken.context.clone(), self.default(dict, method, taken)?)
        };
        // a method with constraints of its own takes their dictionaries
        if matches!(code.kind, ExprKind::Abstract(..)) || context.len() != args.len() {
            return None;
        }
        let given: Vec<(&str, &Dict)> = context.iter().map(String::as_str).zip(args).collect();
        instantiate(&mut code, &given, self.lang, true);
        if elsewhere {
            respan(&mut code, at);
        }
        Some(code)
    }

    /// The code of the default `taken` of the instance `dict`, over the
    /// instance's context.
    fn default(&self, dict: &Dict, method: &str, taken: &Taken) -> Option<Expr> {
        let Dict::Instance { module, name, .. } = dict else {
            return None;
        };
        let (class_module, class) = &taken.class;
        let Generic {
            dicts: params,
            body: mut code,
            ..
        } = self.value(class_module, &default_method(class, method))?;
        let this = Dict::Instance {
            module: module.clone(),
            name: name.clone(),
            args: taken
                .context
                .iter()
                .map(|c| Dict::Given(c.clone()))
                .collect(),
        };
        let dicts: Vec<Dict> = [this].into_iter().chain(taken.extra.clone()).collect();
        if params.len() != dicts.len() {
            return None;
        }
        let given: Vec<(&str, &Dict)> = params.iter().map(String::as_str).zip(&dicts).collect();
        instantiate(&mut code, &given, self.lang, true);
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
                .then(|| lookup.method(&this, &name, item.span))
                .flatten();
            let made = made.and_then(|mut code| {
                *fresh += 1;
                rename(&mut code, &format!("$v{fresh}"));
                settle_parts(&mut code, lookup, fresh, 0);
                methods(&mut code, lookup, fresh, 0);
                super::simplify(&mut code, lookup.lang);
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

/// Puts the code of methods of known instances in `e` where it is taken
/// apart (a `match`, a field, the pattern of a `let`, the argument of a
/// method matching its parameter) and is small data, and anywhere it is a
/// literal; elsewhere the method stays, a name of one shared value. Whether
/// anything changed.
pub fn methods(e: &mut Expr, lookup: &Lookup, fresh: &mut usize, depth: usize) -> bool {
    let mut changed = false;
    e.each_child_mut(&mut |c| changed |= methods(c, lookup, fresh, depth));
    if depth > DEPTH {
        return changed;
    }
    match &mut e.kind {
        ExprKind::Match(taken, _) | ExprKind::Field(taken, _) => {
            changed |= settle(taken, lookup, fresh, depth);
        }
        ExprKind::Block(stmts, _) => {
            for stmt in stmts {
                if let Stmt::Let { pat, value, .. } = stmt
                    && !matches!(pat, Pat::Bind(_) | Pat::Wild)
                {
                    changed |= settle(value, lookup, fresh, depth);
                }
            }
        }
        ExprKind::App { func, arg, .. } => {
            if let ExprKind::Method { dict, name, .. } = &func.kind
                && let Some(mut code) = lookup.method(dict, name, func.span)
                && takes_apart(&code)
            {
                changed |= settle(arg, lookup, fresh, depth);
                // a value with a shape to take apart, not a name of one
                let shaped = matches!(
                    arg.kind,
                    ExprKind::App { .. } | ExprKind::Tuple(_) | ExprKind::Record(_)
                );
                if data(arg) && shaped {
                    specialize_method_types(&mut code, func.ty.as_ref());
                    *fresh += 1;
                    rename(&mut code, &format!("$m{fresh}"));
                    **func = code;
                    return true;
                }
            }
        }
        ExprKind::Method { .. } => {
            let mut copy = e.clone();
            if settle(&mut copy, lookup, fresh, depth) && literal(&copy) {
                *e = copy;
                return true;
            }
        }
        _ => {}
    }
    changed
}

/// Instantiates free type variables in an inlined method body against the
/// method reference's concrete type.
fn specialize_method_types(code: &mut Expr, expected: Option<&TypeRef>) {
    if let (Some(template), Some(concrete)) = (&code.ty, expected) {
        let mut subst = HashMap::new();
        match_type(template, concrete, &mut subst);
        if !subst.is_empty() {
            let pairs: Vec<(&str, &TypeRef)> = subst.iter().map(|(k, v)| (k.as_str(), v)).collect();
            subst_types(code, &pairs);
        }
    }
}

/// Replaces `e`, when it is a method of a known instance, with its code
/// where that is data no bigger than [`DATA`], the methods in it settled
/// first; and settles the parts of data. Whether anything changed.
fn settle(e: &mut Expr, lookup: &Lookup, fresh: &mut usize, depth: usize) -> bool {
    if depth > DEPTH {
        return false;
    }
    let ExprKind::Method { dict, name, .. } = &e.kind else {
        return settle_parts(e, lookup, fresh, depth);
    };
    let Some(mut code) = lookup.method(dict, name, e.span) else {
        return false;
    };
    // A function cannot become data here. Inspecting its body first would
    // recursively expand every method it calls, branching exponentially
    // for derived methods over data with multiple recursive fields.
    if matches!(code.kind, ExprKind::Lambda { .. }) {
        return false;
    }
    specialize_method_types(&mut code, e.ty.as_ref());
    *fresh += 1;
    rename(&mut code, &format!("$s{fresh}"));
    settle_parts(&mut code, lookup, fresh, depth + 1);
    methods(&mut code, lookup, fresh, depth + 1);
    super::simplify(&mut code, lookup.lang);
    if !data(&code) || size(&code) > DATA {
        return false;
    }
    code.span = e.span;
    if code.ty.is_none() {
        code.ty.clone_from(&e.ty);
    }
    *e = code;
    true
}

/// Settles the fields of `e` when it is built of constructors, tuples,
/// records and lists.
fn settle_parts(e: &mut Expr, lookup: &Lookup, fresh: &mut usize, depth: usize) -> bool {
    let mut changed = false;
    let built = constructed(e);
    match &mut e.kind {
        ExprKind::Tuple(items) | ExprKind::List(items) => {
            for item in items {
                changed |= settle(item, lookup, fresh, depth);
            }
        }
        ExprKind::Record(fields) => {
            for (_, v) in fields {
                changed |= settle(v, lookup, fresh, depth);
            }
        }
        ExprKind::App { func, arg, .. } if built => {
            changed |= settle(arg, lookup, fresh, depth);
            changed |= settle_parts(func, lookup, fresh, depth);
        }
        _ => {}
    }
    changed
}

/// Whether `e` applies a constructor.
fn constructed(e: &Expr) -> bool {
    let mut head = e;
    while let ExprKind::App { func, .. } = &head.kind {
        head = func;
    }
    matches!(head.kind, ExprKind::Con(_))
}

/// A value as cheap to copy as to name: a literal, a constructor.
fn literal(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::Lit(Lit::String(s)) => s.len() <= 32,
        ExprKind::Lit(_) | ExprKind::Con(_) => true,
        _ => false,
    }
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

/// Whether `e` is a constructor with its parts, each a name or a literal:
/// data that costs nothing to write again where it is taken apart.
pub(super) fn built(e: &Expr) -> bool {
    let part = |a: &&Expr| {
        matches!(
            a.kind,
            ExprKind::Lit(_) | ExprKind::Local(_) | ExprKind::Con(_)
        )
    };
    applied(e).is_some_and(|(_, args)| args.iter().all(|a| part(a) || built(a)))
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

include!("devirt_match.rs");
