//! Single-pass iterative module analysis supporting materialization visibility
//! and single-use private definition credit.

use std::collections::{HashMap, HashSet};

use flick_ir::{ExprKind, ItemKind, Module, Pat};

use crate::opt::eval::materialize::{ctor_key, expr_cost};
use crate::opt::eval::value::{each_expr_pat, walk_pat};

/// Combined module analysis result computed in one iterative AST walk.
pub(super) struct ModuleAnalysis {
    pub(super) visible_external_ctors: HashSet<String>,
    pub(super) single_use_private: HashMap<String, usize>,
}

/// Collects visible external constructors and single-use private top-level
/// definition node sizes in a single iterative walk over `module`.
pub(super) fn analyze_module(module: &Module, exported: &dyn Fn(&str) -> bool) -> ModuleAnalysis {
    let mut visible = HashSet::new();
    let mut refs: HashMap<String, usize> = HashMap::new();
    for item in &module.items {
        item.each_body(&mut |expr| {
            let mut stack = vec![expr];
            while let Some(e) = stack.pop() {
                match &e.kind {
                    ExprKind::Global {
                        module: m, name, ..
                    } if m == &module.name => {
                        if let Some(count) = refs.get_mut(name.as_str()) {
                            *count += 1;
                        } else {
                            refs.insert(name.clone(), 1);
                        }
                    }
                    ExprKind::Con(c) if c.module != module.name => {
                        visible.insert(ctor_key(&c.module, &c.ty, &c.name));
                    }
                    _ => {}
                }
                each_expr_pat(e, true, &mut |pat| {
                    collect_pat_ctors(pat, &module.name, &mut visible);
                });
                e.each_child(&mut |child| stack.push(child));
            }
        });
    }
    let mut sizes: HashMap<String, usize> = HashMap::new();
    for item in &module.items {
        if let ItemKind::Value {
            name, dicts, body, ..
        } = &item.kind
            && dicts.is_empty()
            && !exported(name)
            && !name.contains('$')
            && refs.get(name.as_str()) == Some(&1)
        {
            sizes.insert(name.clone(), expr_cost(body).nodes as usize);
        }
    }
    ModuleAnalysis {
        visible_external_ctors: visible,
        single_use_private: sizes,
    }
}

fn collect_pat_ctors(root: &Pat, current_module: &str, out: &mut HashSet<String>) {
    walk_pat(root, &mut |pat| {
        if let Pat::Con(c, _) = pat
            && c.module != current_module
        {
            out.insert(ctor_key(&c.module, &c.ty, &c.name));
        }
    });
}
