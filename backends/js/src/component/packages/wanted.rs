//! Validation of `[target.js.dependencies]` specs against npm registry rules
//! and the project's `package.json`.

use flick_backend::command::Dependency;
use flick_backend::{Diagnostic, Severity, Span};
use serde_json::Value;

use super::{Known, resolve};

/// What is wrong with the npm packages `dependencies` ask for: a name or a
/// version that is no package of the registry, a version `package.json`
/// names otherwise and, with `known`, one that is not installed. What a
/// fetched library asks for reaches the project's `package.json`, so it
/// names a package of the registry: no path, no URL, no protocol.
pub(super) fn wanted(
    project: &Value,
    dependencies: &[Dependency],
    mut installed: Option<(&mut Known, &str, Option<&str>)>,
) -> Vec<Diagnostic> {
    let declared = project["dependencies"].as_object();
    let mut diagnostics = Vec::new();
    for (i, d) in dependencies.iter().enumerate() {
        let error = |code: &str, message: String| Diagnostic {
            code: code.into(),
            severity: Severity::Error,
            message,
            span: u32::try_from(i).ok().map(|file| Span {
                file,
                start: 0,
                end: 0,
            }),
            notes: Vec::new(),
        };
        let asker =
            d.by.as_ref()
                .map_or_else(|| "the project".to_owned(), |by| format!("`{by}`"));
        if !is_npm_name(&d.name) {
            let message = format!(
                "`[target.js.dependencies]`: `{}` is not the name of an npm package",
                d.name
            );
            diagnostics.push(error("P032", message));
            continue;
        }
        if !is_registry_version(&d.version) {
            let message = format!(
                "`[target.js.dependencies]`: `{} = \"{}\"` is not a version or range of the registry",
                d.name, d.version
            );
            diagnostics.push(error("P032", message));
            continue;
        }
        let written = declared.and_then(|m| m.get(&d.name));
        if let Some(version) = written.and_then(Value::as_str)
            && version != d.version
        {
            let mut conflict = error(
                "P040",
                format!(
                    "`package.json` names the npm package `{}` as `{version}`, and {asker} needs `{}`",
                    d.name, d.version
                ),
            );
            conflict.notes.push(
                "make them agree on one version, or use versions of the packages that do".into(),
            );
            diagnostics.push(conflict);
            continue;
        }
        // as Node finds it from the root; one not known yet is asked for
        let unmet = installed.as_mut().is_some_and(|(known, root, boundary)| {
            written.is_none() || matches!(resolve(known, root, &d.name, true, *boundary), Ok(None))
        });
        if unmet {
            diagnostics.push(error(
                "P034",
                format!(
                    "{asker} needs the npm package `{}`, which is not installed: run `flick install`",
                    d.name
                ),
            ));
        }
    }
    diagnostics
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
