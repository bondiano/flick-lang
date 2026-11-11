//! `[js.dependencies]` of a manifest, read with the whole of it or alone.

use std::collections::BTreeMap;
use std::path::Path;
use std::{fs, ops};

use serde::Deserialize;
use toml::Spanned;

use super::{CONFIG_FILE, JsDependency};
use crate::diagnostics::{Diagnostic, codes};
use crate::source::{FileId, Span};

/// `[js]` as written: its only table is `dependencies`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawJs {
    #[serde(default)]
    dependencies: BTreeMap<String, Spanned<String>>,
}

/// The npm packages of `[js.dependencies]`. What a fetched library writes
/// here reaches the project's `package.json`, so an entry names a package of
/// the registry: no path, no URL, no `workspace:` or `file:` protocol.
pub(crate) fn js_dependencies(
    raw: Option<RawJs>,
    span: &dyn Fn(ops::Range<usize>) -> Span,
) -> Result<Vec<JsDependency>, Diagnostic> {
    (raw.into_iter().flat_map(|js| js.dependencies))
        .map(|(name, version)| {
            let at = span(version.span());
            let version = version.into_inner();
            let error = |what: String| {
                Diagnostic::error(codes::P032, format!("`[js.dependencies]`: {what}"), at)
            };
            if !is_npm_name(&name) {
                return Err(error(format!("`{name}` is not the name of an npm package")));
            }
            if !is_registry_version(&version) {
                return Err(error(format!(
                    "`{name} = \"{version}\"` is not a version or range of the registry"
                )));
            }
            Ok(JsDependency {
                name,
                version,
                span: at,
            })
        })
        .collect()
}

/// `name` or `@scope/name`, which cannot leave `node_modules`.
fn is_npm_name(name: &str) -> bool {
    let part = |p: &str| {
        !p.is_empty()
            && !p.starts_with('.')
            && p.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~'))
    };
    match name.strip_prefix('@') {
        Some(scoped) => scoped
            .split_once('/')
            .is_some_and(|(scope, name)| part(scope) && part(name)),
        None => part(name),
    }
}

fn is_registry_version(version: &str) -> bool {
    !version.trim().is_empty() && !version.contains([':', '/', '\\'])
}

/// The `[js.dependencies]` of the manifest in `dir`, as `(name, version)`.
pub(crate) fn js_dependencies_in(dir: &Path) -> Vec<(String, String)> {
    fs::read_to_string(dir.join(CONFIG_FILE)).map_or_else(|_| Vec::new(), |text| named(&text))
}

/// The `[js.dependencies]` of a manifest read alone. One that does not
/// parse, or names what is no package of the registry, gives none: the
/// build of its own project reports it.
fn named(manifest: &str) -> Vec<(String, String)> {
    #[derive(Deserialize)]
    struct Manifest {
        js: Option<RawJs>,
    }
    let nowhere = |_: ops::Range<usize>| Span::new(FileId::default(), 0, 0);
    toml::from_str::<Manifest>(manifest)
        .ok()
        .and_then(|manifest| js_dependencies(manifest.js, &nowhere).ok())
        .map_or_else(Vec::new, |found| {
            found.into_iter().map(|d| (d.name, d.version)).collect()
        })
}

#[cfg(test)]
mod tests {
    use super::{is_npm_name, is_registry_version, named};

    #[test]
    fn a_js_dependency_names_a_package_of_the_registry() {
        for name in ["left-pad", "@acme/left.pad", "lodash_es"] {
            assert!(is_npm_name(name), "{name}");
        }
        for name in ["", "../up", "@acme", "@acme/", "a/b", ".hidden", "@a/../b"] {
            assert!(!is_npm_name(name), "{name}");
        }
        for version in ["1.3.0", "^1.3.0", ">=1 <3", "1.x || 2.x", "latest"] {
            assert!(is_registry_version(version), "{version}");
        }
        for version in ["", " ", "file:../x", "workspace:*", "git+ssh://x/y", "../x"] {
            assert!(!is_registry_version(version), "{version}");
        }
    }

    #[test]
    fn the_js_dependencies_of_a_manifest_are_read_alone_and_checked() {
        let pad = "[project]\nkind = \"library\"\n\n[js.dependencies]\nleft-pad = \"^1.3.0\"\n";
        assert_eq!(named(pad), [("left-pad".to_owned(), "^1.3.0".to_owned())]);
        assert!(named("[project]\nkind = \"library\"\n").is_empty());
        // what could leave `node_modules` is not passed on
        assert!(named("[js.dependencies]\nleft-pad = \"file:../x\"\n").is_empty());
    }
}
