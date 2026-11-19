//! `[target.<name>.dependencies]` of a manifest, read with the whole of it
//! or alone: the packages of a backend's ecosystem it needs, which that
//! backend checks and installs. The core knows nothing of their names.

use std::collections::BTreeMap;
use std::path::Path;
use std::{fs, ops};

use serde::Deserialize;
use toml::Spanned;

use super::{CONFIG_FILE, TargetDependency};
use crate::source::{FileId, Span};

/// `[target.<name>]` as written: its only table is `dependencies`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawTarget {
    #[serde(default)]
    dependencies: BTreeMap<String, Spanned<String>>,
}

/// The dependencies of each `[target.<name>]`, by target and name.
pub(crate) fn target_dependencies(
    raw: Option<BTreeMap<String, RawTarget>>,
    span: &dyn Fn(ops::Range<usize>) -> Span,
) -> Vec<TargetDependency> {
    (raw.into_iter().flatten())
        .flat_map(|(target, table)| {
            (table.dependencies.into_iter()).map(move |(name, version)| TargetDependency {
                target: target.clone(),
                name,
                span: span(version.span()),
                version: version.into_inner(),
            })
        })
        .collect()
}

/// The `[target.<name>.dependencies]` of the manifest in `dir`, as
/// `(target, name, version)`.
pub(crate) fn target_dependencies_in(dir: &Path) -> Vec<(String, String, String)> {
    fs::read_to_string(dir.join(CONFIG_FILE)).map_or_else(|_| Vec::new(), |text| named(&text))
}

/// The `[target.<name>.dependencies]` of a manifest read alone; one that
/// does not parse gives none, as the build of its own project reports it.
fn named(manifest: &str) -> Vec<(String, String, String)> {
    #[derive(Deserialize)]
    struct Manifest {
        target: Option<BTreeMap<String, RawTarget>>,
    }
    let nowhere = |_: ops::Range<usize>| Span::new(FileId::default(), 0, 0);
    toml::from_str::<Manifest>(manifest).map_or_else(
        |_| Vec::new(),
        |manifest| {
            (target_dependencies(manifest.target, &nowhere).into_iter())
                .map(|d| (d.target, d.name, d.version))
                .collect()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::named;

    #[test]
    fn the_target_dependencies_of_a_manifest_are_read_alone() {
        let pad =
            "[project]\nkind = \"library\"\n\n[target.js.dependencies]\nleft-pad = \"^1.3.0\"\n";
        assert_eq!(
            named(pad),
            [("js".to_owned(), "left-pad".to_owned(), "^1.3.0".to_owned())]
        );
        assert!(named("[project]\nkind = \"library\"\n").is_empty());
    }
}
