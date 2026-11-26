//! The `@native` clauses of a module checked for every environment, the
//! built-in ones: each clause lowered beside the Flick body, so that a build
//! selects among them as the environment it targets does. The module's own
//! IR lowers the clause the default environment selects.

use flick_ir::{Item, ItemKind, Module};
#[cfg(not(target_arch = "wasm32"))]
use serde::{Deserialize, Serialize};

use super::Lower;
use crate::diagnostics::Diagnostic;
use crate::resolve;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::when;
use crate::types::{
    NativeOverrideEnv, NativeOverrideInfo, OverrideSelection, Range, Typing, let_parts,
};

/// The lowered clauses of one top-level `let` with `@native` overrides.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Serialize, Deserialize))]
pub struct NativeAlternative {
    /// The `when` condition of each clause, in source order.
    pub conditions: Vec<String>,
    /// The items of each clause.
    pub clauses: Vec<Vec<Item>>,
    /// The items of the Flick body.
    pub fallback: Vec<Item>,
    /// The clause the module's IR lowers; `None` when it lowers the body.
    pub lowered: Option<usize>,
}

/// The name a top-level item defines, as a clause or a body replaces it.
fn defined(item: &Item) -> Option<&str> {
    match &item.kind {
        ItemKind::Value { name, .. } => Some(name),
        ItemKind::Foreign(foreign) => Some(&foreign.name),
        _ => None,
    }
}

impl NativeAlternative {
    /// Calls `f` on the span of everything in the clauses and the body.
    pub fn each_span_mut(&mut self, f: &mut dyn FnMut(&mut flick_ir::Span)) {
        for item in self.clauses.iter_mut().flatten().chain(&mut self.fallback) {
            item.each_span_mut(f);
        }
    }

    /// The clause `env` selects; `None` for the Flick body, also when the
    /// clauses are ambiguous there or `--no-native-overrides` is given.
    fn selected(&self, env: &NativeOverrideEnv) -> Option<usize> {
        if env.disable_native_overrides {
            return None;
        }
        let conditions: Option<Vec<when::Expr>> = (self.conditions.iter())
            .map(|c| when::parse(c).ok())
            .collect();
        match env.select(&conditions?) {
            OverrideSelection::Selected(i) => Some(i),
            OverrideSelection::None | OverrideSelection::Ambiguous(..) => None,
        }
    }
}

/// `module`, lowered with the default environment's clauses, as `env`
/// selects among the clauses of `alternatives`: each `let` whose selection
/// differs gets the items of the clause or of the body in place of those it
/// had.
pub fn select_native(
    module: &mut Module,
    alternatives: &[NativeAlternative],
    env: &NativeOverrideEnv,
) {
    for alternative in alternatives {
        let selected = alternative.selected(env);
        if selected == alternative.lowered {
            continue;
        }
        let items = |clause: Option<usize>| {
            clause.map_or(&alternative.fallback, |i| &alternative.clauses[i])
        };
        let names: Vec<&str> = items(alternative.lowered)
            .iter()
            .filter_map(defined)
            .collect();
        let replaced = |item: &Item| defined(item).is_some_and(|n| names.contains(&n));
        let Some(at) = module.items.iter().position(replaced) else {
            continue;
        };
        module.items.retain(|item| !replaced(item));
        module.items.splice(at..at, items(selected).iter().cloned());
    }
}

/// The alternatives of each top-level `let` of `module` with `@native`
/// clauses, which its check recorded each of.
pub fn native_alternatives(
    module: &resolve::Module,
    typing: &Typing,
) -> Result<Vec<NativeAlternative>, Vec<Diagnostic>> {
    let mut decls = Vec::new();
    for decl in module.tree.nodes() {
        match decl.kind {
            K::LetDecl => decls.push(decl),
            K::RecGroup => decls.extend(
                (decl.nodes())
                    .filter(|n| n.kind == K::Members)
                    .flat_map(Node::nodes)
                    .filter(|n| n.kind == K::LetDecl),
            ),
            _ => {}
        }
    }
    let mut alternatives = Vec::new();
    for decl in decls {
        // each with its condition, by index, in the order the check recorded them
        let clauses: Vec<_> = (typing.native_overrides.iter())
            .filter(|(range, _)| *range == decl.range())
            .map(|(_, info)| info.clone())
            .collect();
        let Some(name) = let_parts(decl, module.text).name.clone() else {
            continue;
        };
        if clauses.is_empty() {
            continue;
        }
        // each lowering is fresh, so it is the one the module's own IR has
        let lowered = |native: Option<&NativeOverrideInfo>| {
            let mut lower = Lower::new(module, typing);
            let span = lower.span(decl);
            let mut items = Vec::new();
            if let Some(info) = native {
                lower.fresh_definition();
                lower.native_items(decl, &name, span, info.clone(), &mut items);
            } else {
                lower.no_native = true;
                lower.let_item(decl, span, &mut items);
            }
            if lower.diagnostics.is_empty() {
                Ok(items)
            } else {
                Err(lower.diagnostics)
            }
        };
        let conditions = (clauses.iter())
            .map(|info| condition(decl, info.override_range, module.text))
            .collect();
        alternatives.push(NativeAlternative {
            conditions,
            clauses: clauses
                .iter()
                .map(|info| lowered(Some(info)))
                .collect::<Result<_, _>>()?,
            fallback: lowered(None)?,
            lowered: clauses.iter().position(|info| info.enabled),
        });
    }
    Ok(alternatives)
}

/// The `when` condition of the clause at `range` of `decl`.
fn condition(decl: &Node, range: Range, text: &str) -> String {
    (decl.nodes())
        .find(|n| n.kind == K::NativeOverride && n.range() == range)
        .and_then(|ov| ov.nodes().find(|n| n.kind == K::WhenCond))
        .map(|cond| {
            let (start, end) = cond.range();
            text[start as usize..end as usize].trim().to_owned()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use flick_ir::ItemKind;

    use crate::project::builtins::builtin_bundle_of;
    use crate::types::NativeOverrideEnv;

    /// The kind of `List.map` in the built-in `List` as `env` selects it.
    fn map_in(env: &NativeOverrideEnv) -> &'static str {
        let bundle = builtin_bundle_of(false);
        let at = (bundle.modules.iter())
            .position(|m| m.name == "List")
            .expect("List");
        let module = bundle.module_for(at, Some(env));
        let item = (module.items.iter())
            .find(|item| match &item.kind {
                ItemKind::Value { name, .. } => name == "map",
                ItemKind::Foreign(f) => f.name == "map",
                _ => false,
            })
            .expect("List.map");
        match item.kind {
            ItemKind::Foreign(_) => "foreign",
            _ => "body",
        }
    }

    #[test]
    fn a_build_selects_the_clauses_of_a_built_in_module_again() {
        let js = NativeOverrideEnv::official_js();
        assert_eq!(map_in(&js), "foreign", "`when js || go` binds it on js");
        let go = NativeOverrideEnv {
            active_tags: Some(BTreeSet::from(["go".to_owned(), "native".to_owned()])),
            active_backend_id: Some("flick/go".to_owned()),
            ..NativeOverrideEnv::official_js()
        };
        assert_eq!(map_in(&go), "foreign", "and on go");
        let other = NativeOverrideEnv {
            active_tags: Some(BTreeSet::from(["lua".to_owned()])),
            active_backend_id: Some("flick/lua".to_owned()),
            ..NativeOverrideEnv::official_js()
        };
        assert_eq!(
            map_in(&other),
            "body",
            "another backend runs the Flick body"
        );
        let disabled = NativeOverrideEnv {
            disable_native_overrides: true,
            ..NativeOverrideEnv::official_js()
        };
        assert_eq!(map_in(&disabled), "body", "as `--no-native-overrides` does");
    }
}
