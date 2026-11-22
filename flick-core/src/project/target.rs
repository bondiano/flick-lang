//! The packages of a backend's ecosystem that a project needs beyond its
//! own: what the `[target.<name>.dependencies]` of its manifest, of the
//! implicit packages and of the libraries it depends on name. The backend
//! `<name>` checks them against what its ecosystem has installed and
//! installs them; the core only gathers them and reports two packages of
//! the closure asking for one at different versions.

use std::collections::BTreeMap;

use super::config::TargetDependency;
use super::packages::Packages;
use super::{Host, Project};
use crate::diagnostics::{Diagnostic, codes};
use crate::source::{FileId, SourceMap, Span};
use crate::types;

/// A package of a backend's ecosystem and the package of the closure that
/// asks for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanted {
    /// The backend whose ecosystem has it, by the name of its `[target]`.
    pub target: String,
    pub name: String,
    pub version: String,
    /// The library that asks; `None` for the project itself.
    pub by: Option<String>,
    pub span: Span,
}

impl Wanted {
    fn new(dependency: &TargetDependency, by: Option<&str>) -> Wanted {
        Wanted {
            target: dependency.target.clone(),
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

/// What the project, each implicit package and each library of `packages`
/// ask for, in that order. An implicit package's manifest is no file of this
/// run, so its requirement points at no place of it.
pub(super) fn closure(project: &Project, packages: &Packages) -> Vec<Wanted> {
    let own = project.config.target_dependencies.iter();
    let implicit =
        types::implicit_target_dependencies().map(|(by, target, name, version)| Wanted {
            target: target.to_owned(),
            name: name.to_owned(),
            version: version.to_owned(),
            by: Some(by.to_owned()),
            span: Span::new(FileId::default(), 0, 0),
        });
    let libraries = packages
        .libraries()
        .flat_map(|(name, _, l)| l.target_dependencies.iter().map(move |d| (d, Some(name))));
    (own.map(|d| Wanted::new(d, None)))
        .chain(implicit)
        .chain(libraries.map(|(d, by)| Wanted::new(d, by)))
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

/// `P040` for each package two packages of the closure want of one target
/// at different versions: the text of the requirement must agree, as a
/// project installs one of it.
pub fn conflicts(wanted: &[Wanted]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut first: BTreeMap<(&str, &str), &Wanted> = BTreeMap::new();
    for w in wanted {
        let earlier = *first.entry((&w.target, &w.name)).or_insert(w);
        if earlier.version != w.version {
            diagnostics.push(
                Diagnostic::error(
                    codes::P040,
                    format!(
                        "the package `{}` of `[target.{}]` is needed as `{}` by {} and as `{}` by {}",
                        w.name,
                        w.target,
                        earlier.version,
                        earlier.asker(),
                        w.version,
                        w.asker()
                    ),
                    w.span,
                )
                .with_related(earlier.span, "also needed here")
                .with_fix(|| {
                    (
                        "make them agree on one version, or use versions of the packages that do",
                        Vec::new(),
                    )
                }),
            );
        }
    }
    diagnostics
}
