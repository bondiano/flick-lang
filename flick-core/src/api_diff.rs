//! The changes of a package's public API between two versions, by the
//! interfaces of its modules, and the version bump they call for, as Elm's
//! `elm diff` classifies them: a removed or changed export breaks
//! importers, an added one does not.

use std::collections::BTreeMap;
use std::fmt;

use crate::iface::{Export, Interface, Namespace};
use crate::types::show_type;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Added(String),
    Removed(String),
    /// What changed, as it was and as it is.
    Changed {
        name: String,
        was: String,
        now: String,
    },
}

/// The changes of one module; a whole module added or removed is one change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleChanges {
    pub module: String,
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Bump {
    Patch,
    Minor,
    Major,
}

impl fmt::Display for Bump {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Bump::Patch => "PATCH",
            Bump::Minor => "MINOR",
            Bump::Major => "MAJOR",
        })
    }
}

/// The changes from `old` to `new`, by module name.
pub fn diff(old: &[Interface], new: &[Interface]) -> Vec<ModuleChanges> {
    let old: BTreeMap<&str, &Interface> = old.iter().map(|i| (i.module.as_str(), i)).collect();
    let new: BTreeMap<&str, &Interface> = new.iter().map(|i| (i.module.as_str(), i)).collect();
    let mut modules: Vec<&str> = old.keys().chain(new.keys()).copied().collect();
    modules.sort_unstable();
    modules.dedup();
    modules
        .into_iter()
        .filter_map(|module| {
            let changes = match (old.get(module), new.get(module)) {
                (Some(_), None) => vec![Change::Removed(format!("module {module}"))],
                (None, Some(_)) => vec![Change::Added(format!("module {module}"))],
                (Some(o), Some(n)) => exports(o, n),
                (None, None) => Vec::new(),
            };
            (!changes.is_empty()).then(|| ModuleChanges {
                module: module.to_owned(),
                changes,
            })
        })
        .collect()
}

/// The bump a set of changes calls for.
pub fn bump(changes: &[ModuleChanges]) -> Bump {
    changes
        .iter()
        .flat_map(|m| &m.changes)
        .map(|c| match c {
            Change::Added(_) => Bump::Minor,
            Change::Removed(_) | Change::Changed { .. } => Bump::Major,
        })
        .max()
        .unwrap_or(Bump::Patch)
}

/// `version` (`major.minor.patch`) after `bump`; before 1.0 a breaking
/// change bumps the minor version, as npm's caret ranges read it.
pub fn next_version(version: &str, bump: Bump) -> Option<String> {
    let mut parts = version.split('.').map(str::parse::<u64>);
    let (major, minor, patch) = (
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    );
    Some(match (bump, major) {
        (Bump::Major, 0) | (Bump::Minor, _) => format!("{major}.{}.0", minor + 1),
        (Bump::Major, _) => format!("{}.0.0", major + 1),
        (Bump::Patch, _) => format!("{major}.{minor}.{}", patch + 1),
    })
}

fn exports(old: &Interface, new: &Interface) -> Vec<Change> {
    let key = |e: &Export| (e.entity.namespace, e.entity.name.clone());
    let public = |i: &Interface| -> BTreeMap<(Namespace, String), Export> {
        i.exports
            .iter()
            .filter(|e| !e.entity.name.contains('$'))
            .map(|e| {
                (
                    key(e),
                    Export {
                        at: None,
                        ..e.clone()
                    },
                )
            })
            .collect()
    };
    let (old, new) = (public(old), public(new));
    let mut out = Vec::new();
    for (k, o) in &old {
        match new.get(k) {
            None => out.push(Change::Removed(label(o))),
            // shortcut: renaming a type variable reads as a change; compare up to renaming when it matters
            Some(n) if o != n => out.push(Change::Changed {
                name: label(o),
                was: shape(o),
                now: shape(n),
            }),
            Some(_) => {}
        }
    }
    out.extend(
        new.iter()
            .filter(|(k, _)| !old.contains_key(*k))
            .map(|(_, n)| Change::Added(label(n))),
    );
    out
}

fn label(e: &Export) -> String {
    let what = match e.entity.namespace {
        Namespace::Value => "value",
        Namespace::Type => "type",
        Namespace::Constructor => "constructor",
    };
    format!("{what} {}", e.entity.name)
}

/// How an export reads: its type, or the JSON of its definition.
fn shape(e: &Export) -> String {
    match (&e.scheme, &e.definition) {
        (Some(t), _) => show_type(t),
        (None, Some(d)) => serde_json::to_string(d).unwrap_or_default(),
        (None, None) => format!("{:?}", e.decl),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceMap;
    use std::slice;

    use crate::{resolve, syntax, types};

    fn interface(text: &str) -> Interface {
        let mut sources = SourceMap::default();
        let file = sources.add("Lib.fl", text.to_owned());
        let parse = syntax::parse(text, file);
        let syntax = resolve::module_syntax(&parse.tree, text, file);
        let module = resolve::Module {
            name: "Lib",
            file,
            text,
            tree: &parse.tree,
            syntax: &syntax,
        };
        let builtin = |name: &str| types::builtins().iter().find(|i| i.module == name).cloned();
        let (mut interface, _) = resolve::resolve(&module, &builtin);
        let local = |n: &str| n == "Lib";
        let options = types::Options {
            local: &local,
            limits: types::Limits::default(),
            sql: Default::default(),
        };
        let _ = types::check(
            &module,
            &builtin,
            types::builtins(),
            options,
            &mut interface,
        );
        interface
    }

    #[test]
    fn removed_and_changed_exports_are_major_added_ones_minor() {
        let old = interface(
            "module Lib exposing (..)\n\nf : Number -> Number\nlet f x = x\n\ng : Number\nlet g = 1\n",
        );
        let new = interface(
            "module Lib exposing (..)\n\nf : String -> String\nlet f x = x\n\nh : Number\nlet h = 2\n",
        );
        let changes = diff(slice::from_ref(&old), &[new]);
        assert_eq!(
            changes[0].changes,
            [
                Change::Changed {
                    name: "value f".into(),
                    was: "Number -> Number".into(),
                    now: "String -> String".into(),
                },
                Change::Removed("value g".into()),
                Change::Added("value h".into()),
            ]
        );
        assert_eq!(bump(&changes), Bump::Major);
        let more = interface(
            "module Lib exposing (..)\n\nf : Number -> Number\nlet f x = x\n\ng : Number\nlet g = 1\n\nk : Number\nlet k = 3\n",
        );
        assert_eq!(bump(&diff(slice::from_ref(&old), &[more])), Bump::Minor);
        assert_eq!(
            bump(&diff(slice::from_ref(&old), slice::from_ref(&old))),
            Bump::Patch
        );
        assert_eq!(next_version("1.2.3", Bump::Major).as_deref(), Some("2.0.0"));
        assert_eq!(next_version("0.2.3", Bump::Major).as_deref(), Some("0.3.0"));
        assert_eq!(next_version("1.2.3", Bump::Patch).as_deref(), Some("1.2.4"));
    }
}
