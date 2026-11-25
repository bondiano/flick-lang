//! What environment preflight reads of the built-in modules, apart from their
//! IR: for each item, the names it defines, its foreign binding, and the uses
//! its bodies make, in their order. Preflight reports a use reached through a
//! built-in item at the user's span that reached it, and checks no gated
//! service of one, so these are all it needs. An editor keeps this summary
//! instead of the whole lowered standard library.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::gated::needs_adapter_of;
use super::refs::{Reference, collect_item_references};
use crate::ir::{ForeignMode, ItemKind, Module, reach};
use crate::source::Span;

/// The built-in modules as preflight reads them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuiltinPreflight {
    items: Vec<BuiltinItem>,
    #[serde(skip)]
    index: OnceLock<HashMap<reach::Key, usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BuiltinItem {
    module: String,
    names: Vec<String>,
    foreign: Option<BuiltinForeign>,
    uses: Vec<BuiltinUse>,
}

/// A foreign binding: the name it shows, and the export it binds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BuiltinForeign {
    shown: String,
    export: Option<(String, String)>,
    effect: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum BuiltinUse {
    Value(reach::Key),
    Module { module: String, symbol: String },
}

impl BuiltinPreflight {
    /// The summary of the lowered built-in `modules`.
    pub fn of(modules: &[Module]) -> Self {
        let mut items = Vec::new();
        for module in modules {
            for item in &module.items {
                let mut refs = Vec::new();
                collect_item_references(item, false, None, 0, &mut refs);
                let foreign = match &item.kind {
                    ItemKind::Foreign(f) => Some(BuiltinForeign {
                        shown: f.source_name.clone().unwrap_or_else(|| f.name.clone()),
                        export: (f.binding.module_export())
                            .map(|(from, export)| (from.to_owned(), export.to_owned())),
                        effect: f.mode == ForeignMode::Effect,
                    }),
                    _ => None,
                };
                let uses = (refs.into_iter())
                    .map(|r| match r {
                        Reference::Value { key, .. } => BuiltinUse::Value(key),
                        Reference::ModuleOnly { module, symbol, .. } => {
                            BuiltinUse::Module { module, symbol }
                        }
                    })
                    .collect();
                items.push(BuiltinItem {
                    module: module.name.clone(),
                    names: reach::defined(item),
                    foreign,
                    uses,
                });
            }
        }
        Self {
            items,
            index: OnceLock::new(),
        }
    }

    /// The built-in item that defines `key`.
    pub(super) fn find(&self, key: &reach::Key) -> Option<usize> {
        let index = self.index.get_or_init(|| {
            let mut out = HashMap::new();
            for (at, item) in self.items.iter().enumerate() {
                for name in &item.names {
                    out.insert((item.module.clone(), name.clone()), at);
                }
            }
            out
        });
        index.get(key).copied()
    }

    /// The name the foreign binding of item `at` shows, if it is one.
    pub(super) fn foreign_name(&self, at: usize) -> Option<&str> {
        self.items[at].foreign.as_ref().map(|f| f.shown.as_str())
    }

    /// Whether item `at` is a foreign binding only the Effect adapter
    /// implements, with the UI renderer `ui`.
    pub(super) fn needs_adapter(&self, at: usize, ui: Option<&str>) -> bool {
        let foreign = self.items[at].foreign.as_ref();
        (foreign.and_then(|f| Some((f.export.as_ref()?, f.effect))))
            .is_some_and(|((from, export), effect)| needs_adapter_of(from, export, effect, ui))
    }

    /// The uses of item `at`, each at `origin`, the span that reached it. The
    /// services its handlers cover do not matter: no built-in item reaches a
    /// user item, the only kind whose gated services preflight checks.
    pub(super) fn uses(&self, at: usize, origin: Option<Span>, covered: u8) -> Vec<Reference> {
        (self.items[at].uses.iter())
            .map(|u| match u {
                BuiltinUse::Value(key) => Reference::Value {
                    key: key.clone(),
                    span: origin,
                    covered,
                },
                BuiltinUse::Module { module, symbol } => Reference::ModuleOnly {
                    module: module.clone(),
                    symbol: symbol.clone(),
                    span: origin,
                },
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "builtin_tests.rs"]
mod tests;
