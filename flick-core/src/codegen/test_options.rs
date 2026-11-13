//! What a build gives the JS backend, for tests that compile a module
//! alone.

use super::Resolved;
use crate::ir::{ItemKind, Module};

/// The specifier `spec` gives each foreign module `module` names, by module
/// and specifier, as a build resolves it.
pub(crate) fn resolved_by(module: &Module, spec: &dyn Fn(&str) -> String) -> Vec<Resolved> {
    (module.items.iter())
        .filter_map(|item| match &item.kind {
            ItemKind::Foreign(f) => f.binding.from.as_deref(),
            _ => None,
        })
        .map(|from| Resolved {
            module: module.name.clone(),
            spec: from.to_owned(),
            specifier: spec(from),
        })
        .collect()
}
