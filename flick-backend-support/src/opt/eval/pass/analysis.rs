//! Module-wide iterative analyses supporting materialization visibility and
//! single-use private definition credit.

use std::collections::{HashMap, HashSet};

use flick_ir::{ExprKind, ItemKind, Module, Pat, Stmt};

use crate::opt::eval::materialize::expr_cost;

/// Collects all external constructors already referenced in `module` (either in
/// expressions or patterns) so materialization never introduces an unexported import.
pub(super) fn collect_visible_external_ctors(module: &Module) -> HashSet<(String, String, String)> {
    let mut out = HashSet::new();
    for item in &module.items {
        item.each_body(&mut |expr| {
            let mut stack = vec![expr];
            while let Some(e) = stack.pop() {
                match &e.kind {
                    ExprKind::Con(c) if c.module != module.name => {
                        out.insert((c.module.clone(), c.ty.clone(), c.name.clone()));
                    }
                    ExprKind::Match(_, arms) => {
                        for arm in arms {
                            collect_pat_ctors(&arm.pat, &module.name, &mut out);
                        }
                    }
                    ExprKind::Block(stmts, _) => {
                        for stmt in stmts {
                            if let Stmt::Let(pat, _) = stmt {
                                collect_pat_ctors(pat, &module.name, &mut out);
                            }
                        }
                    }
                    ExprKind::Lambda { params, .. } => {
                        for p in params {
                            collect_pat_ctors(&p.pat, &module.name, &mut out);
                        }
                    }
                    ExprKind::Each { pat, .. } => {
                        collect_pat_ctors(pat, &module.name, &mut out);
                    }
                    ExprKind::Handle(h) => {
                        for arm in &h.errors {
                            collect_pat_ctors(&arm.pat, &module.name, &mut out);
                        }
                    }
                    _ => {}
                }
                e.each_child(&mut |child| stack.push(child));
            }
        });
    }
    out
}

fn collect_pat_ctors(
    root: &Pat,
    current_module: &str,
    out: &mut HashSet<(String, String, String)>,
) {
    let mut stack = vec![root];
    while let Some(pat) = stack.pop() {
        match pat {
            Pat::Con(c, args) => {
                if c.module != current_module {
                    out.insert((c.module.clone(), c.ty.clone(), c.name.clone()));
                }
                for arg in args {
                    stack.push(arg);
                }
            }
            Pat::Tuple(items) | Pat::Or(items) => {
                for item in items {
                    stack.push(item);
                }
            }
            Pat::List(items, rest) => {
                for item in items {
                    stack.push(item);
                }
                if let Some(rest) = rest {
                    stack.push(rest);
                }
            }
            Pat::Record(fields) => {
                for (_, p) in fields {
                    stack.push(p);
                }
            }
            Pat::Wild | Pat::Bind(_) | Pat::Lit(_) => {}
        }
    }
}

/// Counts references to each private top-level `ItemKind::Value` in `module` and
/// records the node size of those referenced exactly once (which eliminating the
/// call allows `unused_items` to remove).
pub(super) fn collect_single_use_private_sizes(
    module: &Module,
    exported: &dyn Fn(&str) -> bool,
) -> HashMap<String, u32> {
    let mut refs: HashMap<&str, usize> = HashMap::new();
    for item in &module.items {
        item.each_body(&mut |expr| {
            let mut stack = vec![expr];
            while let Some(e) = stack.pop() {
                if let ExprKind::Global { module: m, name } = &e.kind
                    && m == &module.name
                {
                    *refs.entry(name.as_str()).or_default() += 1;
                }
                e.each_child(&mut |child| stack.push(child));
            }
        });
    }
    let mut sizes = HashMap::new();
    for item in &module.items {
        if let ItemKind::Value { name, dicts, body } = &item.kind
            && dicts.is_empty()
            && !exported(name)
            && !name.contains('$')
            && refs.get(name.as_str()) == Some(&1)
        {
            sizes.insert(name.clone(), expr_cost(body).nodes);
        }
    }
    sizes
}
