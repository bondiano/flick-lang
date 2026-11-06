//! What each plugin published of the states of its groups, before any
//! block expands, and what of it a module sees: its own names, and those
//! of the modules it imports by the names it writes them with, `Users` or
//! `Schema.Users`.

use std::collections::{BTreeMap, HashMap};

use flick_plugin::Published;

use super::super::Loaded;

/// What each package's plugin published, by module and name.
pub(super) type Publications = HashMap<String, BTreeMap<String, BTreeMap<String, Vec<u8>>>>;

/// Adds what the plugin of `package` published.
pub(super) fn add(all: &mut Publications, package: &str, published: Vec<Published>) {
    let modules = all.entry(package.to_owned()).or_default();
    for p in published {
        modules.entry(p.module).or_default().insert(p.name, p.data);
    }
}

/// What of `theirs`, a plugin's publications, `m` sees.
pub(super) fn seen(
    m: &Loaded,
    theirs: &BTreeMap<String, BTreeMap<String, Vec<u8>>>,
) -> BTreeMap<String, Vec<u8>> {
    let mut seen = theirs.get(&m.name).cloned().unwrap_or_default();
    for import in &m.syntax.imports {
        let Some(names) = theirs.get(&import.module) else {
            continue;
        };
        for (name, data) in names {
            seen.insert(format!("{}.{name}", import.qualifier()), data.clone());
            let exposed = (import.exposing.iter()).any(|e| e.name.is_empty() || e.name == *name);
            if exposed {
                seen.entry(name.clone()).or_insert_with(|| data.clone());
            }
        }
    }
    seen
}
