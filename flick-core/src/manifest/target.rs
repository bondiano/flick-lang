//! `[target.<name>.dependencies]` of a manifest read alone (for implicit
//! packages): the packages of a backend's ecosystem it needs, which that
//! backend checks and installs. The core knows nothing of their names.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::CONFIG_FILE;

/// `[target.<name>]` as read for dependencies alone: sibling backend options
/// are ignored so an implicit package may set both `[target.<name>]` options
/// and `[target.<name>.dependencies]`.
#[derive(Deserialize)]
struct RawTarget {
    #[serde(default)]
    dependencies: BTreeMap<String, String>,
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
    toml::from_str::<Manifest>(manifest).map_or_else(
        |_| Vec::new(),
        |manifest| {
            manifest
                .target
                .into_iter()
                .flatten()
                .flat_map(|(target, table)| {
                    table
                        .dependencies
                        .into_iter()
                        .map(move |(name, version)| (target.clone(), name, version))
                })
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

    #[test]
    fn sibling_target_options_do_not_drop_target_dependencies() {
        let both = "\
[project]
kind = \"lib\"

[target.js]
runtime = \"effect\"

[target.js.dependencies]
left-pad = \"^1.3.0\"
";
        assert_eq!(
            named(both),
            [("js".to_owned(), "left-pad".to_owned(), "^1.3.0".to_owned())]
        );
    }
}
