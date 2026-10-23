//! What the release profile adds to code generation: worker functions for
//! known full applications (`add$2`) and constants for the methods of
//! canonical instances that the module calls through them.

use std::collections::{BTreeSet, HashMap};

use super::names;
use crate::ir::{Expr, ExprKind, Item, ItemKind};
use crate::types::Dict;

/// Functions whose stages before the last are pure, taking their parameters
/// directly, that the module applies fully somewhere: such a call goes to a
/// worker with all the arguments, without the closures in between. Each
/// with its arity, and whether it still needs its curried form: it does
/// when it is exported or named other than in a full application.
pub(super) fn workers(
    module: &str,
    items: &[Item],
    exported: &dyn Fn(&str) -> bool,
) -> HashMap<String, (usize, bool)> {
    let mut calls = HashMap::new();
    for item in items {
        item_bodies(item, &mut |e| count_calls(e, module, &mut calls));
    }
    items
        .iter()
        .filter_map(|item| match &item.kind {
            ItemKind::Value { name, dicts, body } if dicts.is_empty() => match &body.kind {
                ExprKind::Lambda { params, .. }
                    if params.len() >= 2
                        && params[..params.len() - 1].iter().all(|p| !p.runtime) =>
                {
                    let (refs, full) =
                        calls
                            .get(name.as_str())
                            .and_then(|by_arity: &HashMap<usize, usize>| {
                                Some((
                                    by_arity.values().sum::<usize>(),
                                    *by_arity.get(&params.len())?,
                                ))
                            })?;
                    Some((name.clone(), (params.len(), exported(name) || refs > full)))
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn item_bodies<'a>(item: &'a Item, f: &mut dyn FnMut(&'a Expr)) {
    match &item.kind {
        ItemKind::Value { body, .. } | ItemKind::Destructure { body, .. } => f(body),
        ItemKind::Refined { check, .. } => f(check),
        ItemKind::Instance { methods, .. } => methods.iter().for_each(|(_, m)| f(m)),
        ItemKind::Foreign(foreign) => foreign.catch.iter().for_each(&mut *f),
        ItemKind::Constructor(_) | ItemKind::Service { .. } | ItemKind::Schema(_) => {}
    }
}

/// For each value of `module`, how often it is named with each number of
/// arguments applied (0: a bare reference), evaluated left to right.
fn count_calls<'a>(e: &'a Expr, module: &str, out: &mut HashMap<&'a str, HashMap<usize, usize>>) {
    let mut head = e;
    let mut n = 0;
    while let ExprKind::App {
        func,
        arg_first: false,
        ..
    } = &head.kind
    {
        head = func;
        n += 1;
    }
    if let ExprKind::Global { module: m, name } = &head.kind
        && m == module
    {
        *out.entry(name.as_str()).or_default().entry(n).or_default() += 1;
        // the spine is counted: only its arguments remain to visit
        let mut node = e;
        while let ExprKind::App { func, arg, .. } = &node.kind {
            count_calls(arg, module, out);
            node = func;
        }
        return;
    }
    e.each_child(&mut |c| count_calls(c, module, out));
}

/// The methods of the module's instances without a context, written in the
/// instance, that the module calls through that very instance: each becomes
/// a constant `<instance>$<method>` the calls name directly. A method of
/// another dictionary of the same type is never replaced.
pub(super) fn direct_methods(module: &str, items: &[Item]) -> HashMap<(String, String), String> {
    let mut used = BTreeSet::new();
    for item in items {
        item_bodies(item, &mut |e| method_uses(e, module, &mut used));
    }
    let mut out = HashMap::new();
    for item in items {
        if let ItemKind::Instance { info, methods } = &item.kind
            && info.context.is_empty()
            && info.derived.is_none()
        {
            for (method, _) in methods {
                let key = (info.name.clone(), method.clone());
                if used.contains(&key) {
                    let constant = format!("{}${method}", names::instance(&info.name));
                    out.insert(key, constant);
                }
            }
        }
    }
    out
}

fn method_uses(e: &Expr, module: &str, out: &mut BTreeSet<(String, String)>) {
    if let ExprKind::Method {
        dict: Dict::Instance {
            module: m,
            name,
            args,
        },
        name: method,
    } = &e.kind
        && m == module
        && args.is_empty()
    {
        out.insert((name.clone(), method.clone()));
    }
    e.each_child(&mut |c| method_uses(c, module, out));
}
