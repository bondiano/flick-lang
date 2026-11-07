//! `[workspace]`: its members and the plugins they all get.

use std::collections::BTreeMap;
use std::ops;

use serde::Deserialize;
use toml::Spanned;

use super::WorkspacePlugin;
use super::deps::{RawDependency, RawGrants, grants, source};
use crate::diagnostics::{Diagnostic, codes};
use crate::source::Span;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawWorkspace {
    pub(super) members: Vec<String>,
    #[serde(default)]
    pub(super) plugins: Vec<Spanned<RawPlugin>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawPlugin {
    package: String,
    source: RawDependency,
    grants: Option<Spanned<RawGrants>>,
}

/// The plugins of `[[workspace.plugins]]`, each named once.
pub(super) fn plugins(
    raw: Vec<Spanned<RawPlugin>>,
    span: &dyn Fn(ops::Range<usize>) -> Span,
) -> Result<Vec<WorkspacePlugin>, Diagnostic> {
    let mut out: Vec<WorkspacePlugin> = Vec::new();
    for entry in raw {
        let at = span(entry.span());
        let RawPlugin {
            package,
            source: from,
            grants: given,
        } = entry.into_inner();
        let wrong = |what: String| {
            Diagnostic::error(codes::P005, format!("`[[workspace.plugins]]`: {what}"), at)
        };
        if out.iter().any(|p| p.package == package) {
            return Err(wrong(format!("`{package}` is named twice")));
        }
        let source = source(from).map_err(|why| wrong(format!("`{package}`: {why}")))?;
        let granted = match given {
            Some(given) => grants(BTreeMap::from([(package.clone(), given)]), span)?
                .remove(&package)
                .unwrap_or_default(),
            None => super::Grants::default(),
        };
        out.push(WorkspacePlugin {
            package,
            source,
            grants: granted,
            span: at,
        });
    }
    Ok(out)
}
