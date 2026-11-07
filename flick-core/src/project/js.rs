//! The npm packages a project needs on the js target beyond its own
//! `package.json`: what the `[js.dependencies]` of its manifest and of the
//! libraries it depends on name. `flick install` adds them to `package.json`,
//! so pnpm installs and pins them as the project's own, and a foreign module
//! of a library, which ships into the project's output, finds them.

use std::collections::BTreeMap;
use std::path::Path;

use super::config::JsDependency;
use super::packages::{Packages, manifest};
use super::{Host, Project};
use crate::diagnostics::{Diagnostic, codes};
use crate::source::{SourceMap, Span};

/// An npm package and the package of the closure that asks for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanted {
    pub name: String,
    pub version: String,
    /// The library that asks; `None` for the project itself.
    pub by: Option<String>,
    pub span: Span,
}

impl Wanted {
    fn new(dependency: &JsDependency, by: Option<&str>) -> Wanted {
        Wanted {
            name: dependency.name.clone(),
            version: dependency.version.clone(),
            by: by.map(str::to_owned),
            span: dependency.span,
        }
    }

    fn asker(&self) -> String {
        self.by
            .as_ref()
            .map_or_else(|| "the project".to_owned(), |by| format!("`{by}`"))
    }
}

/// What the project and each library of `packages` ask for, in that order.
pub(super) fn closure(project: &Project, packages: &Packages) -> Vec<Wanted> {
    let own = project.config.js_dependencies.iter();
    let libraries = packages
        .libraries()
        .flat_map(|(name, _, l)| l.js_dependencies.iter().map(move |d| (d, Some(name))));
    own.map(|d| (d, None))
        .chain(libraries)
        .map(|(d, by)| Wanted::new(d, by))
        .collect()
}

/// [`closure`] of `project`, reading the manifests of its libraries.
pub fn wanted(
    host: &dyn Host,
    sources: &mut SourceMap,
    project: &Project,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Wanted> {
    closure(
        project,
        &Packages::load(host, sources, project, diagnostics),
    )
}

/// The dependencies of the `package.json` at `root`, by name, as written.
fn declared(
    host: &dyn Host,
    root: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> serde_json::Map<String, serde_json::Value> {
    manifest(host, &root.join("package.json"), diagnostics)
        .map_or_else(Default::default, |m| m.dependencies)
}

/// `P040` for each npm package two packages of the closure, or one of them
/// and the `package.json` at `root`, want at different versions: the text of
/// the requirement must agree, as a project installs one of it.
pub fn conflicts(host: &dyn Host, root: &Path, wanted: &[Wanted]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if wanted.is_empty() {
        return diagnostics;
    }
    let declared = declared(host, root, &mut diagnostics);
    let mut first: BTreeMap<&str, &Wanted> = BTreeMap::new();
    for w in wanted {
        let conflict = |message: String| {
            Diagnostic::error(codes::P040, message, w.span).with_fix(|| {
                (
                    "make them agree on one version, or use versions of the packages that do",
                    Vec::new(),
                )
            })
        };
        if let Some(version) = declared.get(&w.name).and_then(|v| v.as_str())
            && version != w.version
        {
            diagnostics.push(conflict(format!(
                "`package.json` names the npm package `{}` as `{version}`, and {} needs `{}`",
                w.name,
                w.asker(),
                w.version
            )));
        }
        let earlier = *first.entry(&w.name).or_insert(w);
        if earlier.version != w.version {
            diagnostics.push(
                conflict(format!(
                    "the npm package `{}` is needed as `{}` by {} and as `{}` by {}",
                    w.name,
                    earlier.version,
                    earlier.asker(),
                    w.version,
                    w.asker()
                ))
                .with_related(earlier.span, "also needed here"),
            );
        }
    }
    diagnostics
}

/// `P034` for each npm package of `wanted` that the `package.json` at `root`
/// does not name, or that Node would not find from `root`.
pub fn unmet(host: &dyn Host, root: &Path, wanted: &[Wanted]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if wanted.is_empty() {
        return diagnostics;
    }
    let declared = declared(host, root, &mut diagnostics);
    let installed = |name: &str| {
        root.ancestors().any(|dir| {
            let file = dir.join("node_modules").join(name).join("package.json");
            host.read(&file).is_ok()
        })
    };
    for w in wanted {
        if !declared.contains_key(&w.name) || !installed(&w.name) {
            diagnostics.push(Diagnostic::error(
                codes::P034,
                format!(
                    "{} needs the npm package `{}`, which is not installed: run `flick install`",
                    w.asker(),
                    w.name
                ),
                w.span,
            ));
        }
    }
    diagnostics
}

/// The text of a `package.json` with the packages of `wanted` it lacks among
/// its `dependencies`, keeping the rest as written; `None` when it has them
/// all. `text` is `None` for a project without one.
pub fn merged_manifest(text: Option<&str>, wanted: &[Wanted]) -> Result<Option<String>, String> {
    let parsed: serde_json::Value = match text {
        Some(text) => {
            serde_json::from_str(text).map_err(|e| format!("`package.json` is not JSON: {e}"))?
        }
        None if wanted.is_empty() => return Ok(None),
        None => serde_json::json!({}),
    };
    let mut dependencies = parsed
        .get("dependencies")
        .and_then(|d| d.as_object())
        .cloned()
        .unwrap_or_default();
    let before = dependencies.len();
    for w in wanted {
        dependencies
            .entry(w.name.clone())
            .or_insert_with(|| w.version.clone().into());
    }
    if dependencies.len() == before {
        return Ok(None);
    }
    let lines: Vec<String> = (dependencies.iter())
        .map(|(name, version)| format!("    {}: {version}", serde_json::json!(name)))
        .collect();
    let table = format!("{{\n{}\n  }}", lines.join(",\n"));
    let Some(text) = text else {
        return Ok(Some(format!(
            "{{\n  \"private\": true,\n  \"dependencies\": {table}\n}}\n"
        )));
    };
    if let Some((start, end)) = top_level_value(text, "dependencies") {
        return Ok(Some(format!("{}{table}{}", &text[..start], &text[end..])));
    }
    let close = text.rfind('}').expect("a JSON object");
    let last = text[..close].trim_end();
    let comma = if last.ends_with('{') { "" } else { "," };
    Ok(Some(format!(
        "{last}{comma}\n  \"dependencies\": {table}\n{}",
        &text[close..]
    )))
}

/// The bytes of the value of `key` in the object that is the valid JSON
/// `text`: a parse would forget the order of the rest.
fn top_level_value(text: &str, key: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let (mut depth, mut i) = (0, 0);
    let mut last_string = "";
    let mut value_start = None;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let start = i + 1;
                i += 1;
                while bytes[i] != b'"' {
                    i += if bytes[i] == b'\\' { 2 } else { 1 };
                }
                if depth == 1 && value_start.is_none() {
                    last_string = &text[start..i];
                }
            }
            b'{' | b'[' => depth += 1,
            b'}' | b']' | b',' if depth == 1 => {
                if let Some(start) = value_start {
                    return Some((start, text[..i].trim_end().len()));
                }
                if bytes[i] != b',' {
                    return None;
                }
            }
            b'}' | b']' => depth -= 1,
            b':' if depth == 1 && last_string == key && value_start.is_none() => {
                let rest = &text[i + 1..];
                value_start = Some(i + 1 + rest.len() - rest.trim_start().len());
            }
            _ => {}
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FileId;

    fn want(name: &str, version: &str) -> Wanted {
        Wanted {
            name: name.into(),
            version: version.into(),
            by: Some("lib".into()),
            span: Span::new(FileId::default(), 0, 0),
        }
    }

    #[test]
    fn a_manifest_gains_only_what_it_lacks_and_keeps_its_order() {
        let text = "{\n  \"name\": \"app\",\n  \"scripts\": { \"dependencies\": \"x\" },\n  \"dependencies\": {\n    \"b\": \"1\"\n  },\n  \"type\": \"module\"\n}\n";
        let merged = merged_manifest(Some(text), &[want("a", "^2"), want("b", "1")])
            .expect("valid")
            .expect("changed");
        assert_eq!(
            merged,
            "{\n  \"name\": \"app\",\n  \"scripts\": { \"dependencies\": \"x\" },\n  \"dependencies\": {\n    \"a\": \"^2\",\n    \"b\": \"1\"\n  },\n  \"type\": \"module\"\n}\n"
        );
        assert_eq!(merged_manifest(Some(&merged), &[want("a", "^2")]), Ok(None));
    }

    #[test]
    fn a_manifest_without_dependencies_gains_the_table() {
        let added = merged_manifest(Some("{\n  \"name\": \"app\"\n}\n"), &[want("a", "1")]);
        assert_eq!(
            added,
            Ok(Some(
                "{\n  \"name\": \"app\",\n  \"dependencies\": {\n    \"a\": \"1\"\n  }\n}\n".into()
            ))
        );
        let empty = merged_manifest(Some("{}"), &[want("a", "1")])
            .expect("valid")
            .expect("changed");
        let value: serde_json::Value = serde_json::from_str(&empty).expect("valid JSON");
        assert_eq!(value["dependencies"]["a"], "1");
    }

    #[test]
    fn a_project_without_a_manifest_gets_one_only_when_it_needs_it() {
        assert_eq!(merged_manifest(None, &[]), Ok(None));
        let made = merged_manifest(None, &[want("a", "1")])
            .expect("valid")
            .expect("made");
        let value: serde_json::Value = serde_json::from_str(&made).expect("valid JSON");
        assert_eq!(value["dependencies"]["a"], "1");
    }
}
