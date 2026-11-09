//! The plugins that watch calls: each package that declares `watch` in its
//! manifest, and std's, which the compiler links in. Every module checked
//! is read for the places that call one of their names, blocks or no
//! blocks.

use std::sync::Arc;

use super::{Loader, Plugins};
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::plugin::{Expander, first_party};
use crate::source::Span;
use crate::types;
/// A plugin that watches names: its package, the plugin, and the names its
/// package declares in `watch`.
pub(in crate::project::load) struct Watching {
    pub package: String,
    pub plugin: Arc<dyn Expander>,
    pub watched: types::Watched,
}

/// The plugins watching names of the modules loaded.
#[derive(Default)]
pub(in crate::project::load) struct Watchers {
    pub plugins: Vec<Watching>,
    /// Every name any of them watches, which the checker collects the
    /// places of.
    pub watched: types::Watched,
}

impl Watchers {
    pub(super) fn new(plugins: Vec<Watching>) -> Watchers {
        let mut watched = types::Watched::default();
        for w in &plugins {
            watched.extend(&w.watched);
        }
        Watchers { plugins, watched }
    }
}

impl Loader<'_> {
    /// The plugins that watch names of every module: std's, which the
    /// compiler links in, and those of the packages whose manifest says
    /// `watch`. A module without blocks gets them too.
    pub(super) fn watching(
        &self,
        plugins: &mut Plugins,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Vec<Watching> {
        let mut out: Vec<_> = linked(plugins, diagnostics).into_iter().collect();
        for (package, names) in self.watchers() {
            let watched = types::Watched::new(names.iter().map(String::as_str));
            let plugin =
                plugins.entry(package.clone()).or_insert_with(|| {
                    match first_party::plugin(&package) {
                        Some(plugin) => Ok(plugin),
                        None => self.host.plugin(&package, self.component(&package)),
                    }
                });
            match plugin {
                Ok(plugin) => out.push(Watching {
                    package,
                    plugin: Arc::clone(plugin),
                    watched,
                }),
                // the names it watches go unchecked, which is no less than a
                // block of it would say
                Err(why) => {
                    let message =
                        format!("the plugin of package `{package}` cannot be loaded: {why}");
                    let mut d = Diagnostic::new(codes::P036, Severity::Error, message);
                    if let Some(at) = self.dependency_span(&package) {
                        d = d.with_span(at);
                    }
                    diagnostics.push(d);
                }
            }
        }
        out
    }

    /// The names each package of the project watches, with the package.
    #[cfg_attr(not(feature = "native-project"), allow(clippy::unused_self))]
    fn watchers(&self) -> Vec<(String, Vec<String>)> {
        #[cfg(feature = "native-project")]
        return (self.packages.watchers())
            .map(|(name, names)| (name.to_owned(), names.to_vec()))
            .collect();
        #[cfg(not(feature = "native-project"))]
        Vec::new()
    }

    /// Where the manifest declares the dependency `package`.
    fn dependency_span(&self, package: &str) -> Option<Span> {
        let dependencies = &self.project?.config.dependencies;
        (dependencies.iter().find(|d| d.name == package)).map(|d| d.span)
    }
}

/// std watches calls even when no block imports its plugin.
fn linked(plugins: &mut Plugins, diagnostics: &mut Vec<Diagnostic>) -> Option<Watching> {
    let std = first_party::PACKAGE;
    let linked = plugins.entry(std.to_owned()).or_insert_with(|| {
        first_party::plugin(std).ok_or_else(|| format!("`{std}` has no linked plugin"))
    });
    match linked {
        Ok(plugin) => Some(Watching {
            package: std.to_owned(),
            plugin: Arc::clone(plugin),
            watched: types::Watched::new(first_party::WATCH),
        }),
        Err(why) => {
            diagnostics.push(Diagnostic::new(
                codes::P036,
                Severity::Error,
                format!("the plugin of package `{std}` cannot be loaded: {why}"),
            ));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Plugins, linked};
    use crate::diagnostics::codes;
    use crate::plugin::first_party;

    #[test]
    fn a_failed_linked_watcher_is_reported() {
        let mut plugins =
            Plugins::from([(first_party::PACKAGE.to_owned(), Err("unavailable".into()))]);
        let mut diagnostics = Vec::new();
        assert!(linked(&mut plugins, &mut diagnostics).is_none());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, codes::P036);
        assert!(
            diagnostics[0]
                .message
                .contains("package `std` cannot be loaded: unavailable")
        );
    }

    #[test]
    fn the_linked_watcher_needs_no_manifest() {
        let mut diagnostics = Vec::new();
        let watcher = linked(&mut Plugins::new(), &mut diagnostics).expect("linked std");
        assert!(watcher.watched.holds("Ui.Event", "queue"));
        assert!(diagnostics.is_empty());
    }
}
