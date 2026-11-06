//! What the release profile adds to code generation: worker functions for
//! known full applications (`add$2`) and constants for the methods of
//! canonical instances that the module calls through them.

use std::collections::HashMap;

use super::names;
use crate::ir::{Expr, ExprKind, Item, ItemKind, Stage};

/// The name and arity of the worker `name$n` a top-level `item` exports in
/// release when the module exports it: a function of two or more
/// parameters, without dictionaries, whose stages before the last are pure.
/// It agrees with the item's stages in the summaries of `.iface`, so an
/// importer calls `name$n` for a full application.
pub fn exported_worker(item: &Item) -> Option<(&str, usize)> {
    let (name, arity, before_last_pure) = match &item.kind {
        ItemKind::Value { name, dicts, body } if dicts.is_empty() => match &body.kind {
            ExprKind::Lambda { params, .. } => (
                name,
                params.len(),
                params.iter().rev().skip(1).all(|p| p.stage == Stage::Pure),
            ),
            _ => return None,
        },
        ItemKind::Foreign(f) if f.dicts == 0 && f.taken() == f.stages.len() => (
            &f.name,
            f.stages.len(),
            f.stages.iter().rev().skip(1).all(|&r| !r),
        ),
        _ => return None,
    };
    (arity >= 2 && before_last_pure).then_some((name.as_str(), arity))
}

/// Functions whose stages before the last are pure, taking their parameters
/// directly, that the module applies fully somewhere or exports with a
/// worker (`exported_arity`): such a call goes to a worker with all the
/// arguments, without the closures in between. Each with its arity, and
/// whether it still needs its curried form: it does when it is exported or
/// named other than in a full application.
pub(super) fn workers(
    module: &str,
    items: &[Item],
    exported: &dyn Fn(&str) -> bool,
    exported_arity: &dyn Fn(&str) -> Option<usize>,
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
                        && params[..params.len() - 1]
                            .iter()
                            .all(|p| p.stage == Stage::Pure) =>
                {
                    let by_arity: Option<&HashMap<usize, usize>> = calls.get(name.as_str());
                    let refs = by_arity.map_or(0, |b| b.values().sum::<usize>());
                    let full = by_arity.and_then(|b| b.get(&params.len())).copied();
                    if full.is_none() && exported_arity(name) != Some(params.len()) {
                        return None;
                    }
                    let full = full.unwrap_or(0);
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
        ItemKind::Constructor(_) | ItemKind::Service { .. } => {}
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

/// The methods of `item`, an instance without a context, written in it
/// rather than derived or defaulted: in release each is a constant
/// `<instance>$<method>` the module exports, and a canonical use calls it
/// directly, in this module or an importer. A method of another dictionary
/// of the same type is never replaced.
pub fn direct_methods(item: &Item) -> Vec<(&str, &str)> {
    match &item.kind {
        ItemKind::Instance { info, methods }
            if info.context.is_empty() && info.derived.is_none() =>
        {
            methods
                .iter()
                .map(|(method, _)| (info.name.as_str(), method.as_str()))
                .collect()
        }
        _ => Vec::new(),
    }
}

/// The constant of each direct method of the module's instances (see
/// [`direct_methods`]), by instance and method.
pub(super) fn direct_constants(items: &[Item]) -> HashMap<(String, String), String> {
    items
        .iter()
        .flat_map(direct_methods)
        .map(|(instance, method)| {
            let constant = format!("{}${method}", names::instance(instance));
            ((instance.to_owned(), method.to_owned()), constant)
        })
        .collect()
}
