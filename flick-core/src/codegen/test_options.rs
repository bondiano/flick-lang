//! What a build gives the JS backend, for tests that compile a module
//! alone.

use std::collections::BTreeMap;

use crate::ir::{ItemKind, Module};

/// The specifier `spec` gives each foreign module `module` names, by module
/// and specifier, as a build resolves it.
pub(crate) fn resolved_by(
    module: &Module,
    spec: &dyn Fn(&str) -> String,
) -> BTreeMap<(String, String), String> {
    (module.items.iter())
        .filter_map(|item| match &item.kind {
            ItemKind::Foreign(f) => f.binding.from.as_deref(),
            _ => None,
        })
        .map(|from| ((module.name.clone(), from.to_owned()), spec(from)))
        .collect()
}
