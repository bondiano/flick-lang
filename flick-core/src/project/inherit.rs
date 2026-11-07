//! What a member of a workspace gets from the workspace's root: the plugins
//! of `[[workspace.plugins]]`, as dependencies and grants of its own.

use std::path::{Component, Path, PathBuf};

use super::config::{self, Config, Dependency, Source, WorkspacePlugin};
use super::{CONFIG_FILE, Host, normalize};
use crate::diagnostics::{Diagnostic, codes};
use crate::source::SourceMap;

/// The root of the workspace the project at `member` is a member of, its
/// manifest and the plugins it gives its members.
pub(super) fn around(
    host: &dyn Host,
    sources: &mut SourceMap,
    member: &Path,
) -> Result<Option<(PathBuf, Vec<WorkspacePlugin>)>, Diagnostic> {
    for dir in member.ancestors().skip(1) {
        let path = dir.join(CONFIG_FILE);
        let Ok(bytes) = host.read(&path) else {
            continue;
        };
        let (file, utf8) = sources.add_bytes(host.display(&path), bytes);
        if let Some(diagnostic) = utf8 {
            return Err(diagnostic);
        }
        let manifest = config::parse_manifest(&sources.get(file).text, file)?;
        let Some(members) = &manifest.members else {
            continue;
        };
        let inside = members.iter().any(|m| normalize(&dir.join(m)) == member);
        return Ok(inside.then_some((path, manifest.plugins)));
    }
    Ok(None)
}

/// `config`, of the member at `member` of the workspace at `root`, with the
/// plugins it gets: each a dependency, unless it is the plugin's package, and
/// its grants added to those of the member, whose root `fs-read` is.
pub(super) fn inherit(
    config: &mut Config,
    member: &Path,
    root: &Path,
    plugins: &[WorkspacePlugin],
) -> Result<(), Diagnostic> {
    for plugin in plugins {
        let own = config
            .package
            .as_ref()
            .is_some_and(|p| p.name == plugin.package);
        if own {
            continue;
        }
        let source = rebased(&plugin.source, member, root);
        let same = |s: &Source| match (s, &source) {
            (Source::Path(a), Source::Path(b)) => {
                normalize(&member.join(a)) == normalize(&member.join(b))
            }
            (a, b) => a == b,
        };
        match config
            .dependencies
            .iter()
            .find(|d| d.name == plugin.package)
        {
            Some(written) if !same(&written.source) => {
                let message = format!(
                    "`{}` comes from `{}` here and from `{}` in `[[workspace.plugins]]`",
                    plugin.package,
                    written.source.key(),
                    plugin.source.key()
                );
                return Err(Diagnostic::error(codes::P005, message, written.span)
                    .with_related(plugin.span, "the workspace's plugin"));
            }
            Some(_) => {}
            None => config.dependencies.push(Dependency {
                name: plugin.package.clone(),
                source,
                span: plugin.span,
            }),
        }
        let grants = config.plugins.entry(plugin.package.clone()).or_default();
        let given = &plugin.grants;
        for (mine, theirs) in [
            (&mut grants.fs_read, &given.fs_read),
            (&mut grants.net, &given.net),
            (&mut grants.env, &given.env),
            (&mut grants.disable, &given.disable),
        ] {
            mine.extend(
                theirs
                    .iter()
                    .filter(|t| !mine.contains(t))
                    .cloned()
                    .collect::<Vec<_>>(),
            );
        }
        grants.span = grants.span.or(given.span).or(Some(plugin.span));
    }
    Ok(())
}

/// `source`, written relative to the workspace's root, relative to `member`.
fn rebased(source: &Source, member: &Path, root: &Path) -> Source {
    let Source::Path(path) = source else {
        return source.clone();
    };
    let up = member.strip_prefix(root).map(|below| {
        below
            .components()
            .filter(|c| matches!(c, Component::Normal(_)))
            .map(|_| "..")
            .collect::<PathBuf>()
    });
    let at = match up {
        Ok(up) => normalize(&up.join(path)),
        Err(_) => normalize(&root.join(path)),
    };
    Source::Path(at.to_string_lossy().into_owned())
}

#[cfg(test)]
#[path = "inherit_tests.rs"]
mod tests;
