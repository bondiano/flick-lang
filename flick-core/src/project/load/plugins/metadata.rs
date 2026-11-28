//! Discovery and validation of external plugin static metadata (`heads`,
//! `watch`, `derives`, `own_codes`) before parsing project source files.

#[cfg(feature = "native-project")]
use std::collections::HashMap;
#[cfg(feature = "native-project")]
use std::path::{Path, PathBuf};
#[cfg(feature = "native-project")]
use std::sync::Arc;

#[cfg(feature = "native-project")]
use flick_plugin::{AttributeSpec, HeadSpec};

use super::super::Loader;
#[cfg(feature = "native-project")]
use super::results;
#[cfg(feature = "native-project")]
use crate::diagnostics::{Diagnostic, Severity, codes};
#[cfg(feature = "native-project")]
use crate::plugin::{first_party, metadata};
#[cfg(feature = "native-project")]
use crate::syntax::{HeadCatalog, HeadRule};
#[cfg(feature = "native-project")]
use crate::types::PluginAttribute;

impl Loader<'_> {
    /// Discovers and validates static metadata (`heads`, `watch`, `derives`,
    /// `own_codes`) from external dependency plugins before parsing source
    /// files, extending `self.head_catalog` with valid external `HeadSpec`s.
    #[cfg_attr(not(feature = "native-project"), allow(clippy::unused_self))]
    pub(in crate::project::load) fn init_plugin_metadata(&mut self) {
        #[cfg(feature = "native-project")]
        {
            let mut attributes: Vec<PluginAttribute> = first_party::metadata()
                .attributes
                .iter()
                .cloned()
                .map(|spec| PluginAttribute {
                    package: first_party::PACKAGE.to_owned(),
                    spec,
                })
                .collect();
            let candidates: Vec<(String, Option<PathBuf>)> = self
                .packages
                .plugin_candidates()
                .map(|(name, path)| (name.to_owned(), path.map(Path::to_path_buf)))
                .collect();
            if candidates.is_empty() {
                if !attributes.is_empty() {
                    attributes.sort_by(|a, b| {
                        a.spec
                            .name
                            .cmp(&b.spec.name)
                            .then_with(|| a.package.cmp(&b.package))
                    });
                    self.plugin_attributes = Arc::from(attributes);
                }
                return;
            }
            let std_catalog = first_party::head_catalog();
            let mut catalog = std_catalog.clone();
            let mut owners: HashMap<String, String> = HashMap::new();
            let mut attr_owners: HashMap<String, String> = attributes
                .iter()
                .map(|a| (a.spec.name.clone(), a.package.clone()))
                .collect();
            let mut added_head = false;
            for (package, component) in candidates {
                if first_party::has_plugin(&package) {
                    continue;
                }
                // Defer load errors (missing component, wrong protocol version,
                // wasm disabled) to the import/derive/watch site that uses it.
                let Ok(plugin) = self.host.plugin(&package, component.as_deref()) else {
                    continue;
                };
                self.preloaded_plugins
                    .insert(package.clone(), Arc::clone(&plugin));
                let meta = match results::metadata(self.host, plugin.as_ref()) {
                    Ok(meta) => meta,
                    Err(why) => {
                        let mut d = Diagnostic::new(
                            codes::P036,
                            Severity::Error,
                            format!("plugin `{package}` failed to provide metadata: {why}"),
                        );
                        if let Some(at) = self.dependency_span(&package) {
                            d = d.with_span(at);
                        }
                        self.diagnostics.push(d);
                        continue;
                    }
                };
                let issues = metadata::validate(&package, &meta, std_catalog);
                if !issues.is_empty() {
                    let at = self.dependency_span(&package);
                    for issue in issues {
                        let mut d = Diagnostic::new(issue.code, Severity::Error, issue.message);
                        if let Some(span) = at {
                            d = d.with_span(span);
                        }
                        self.diagnostics.push(d);
                    }
                    continue;
                }
                added_head |=
                    self.register_plugin_heads(&package, &meta.heads, &mut owners, &mut catalog);
                self.register_plugin_attributes(
                    &package,
                    &meta.attributes,
                    &mut attr_owners,
                    &mut attributes,
                );
                self.plugin_metadata.insert(package, meta);
            }
            if added_head {
                self.head_catalog = Arc::new(catalog);
            }
            if !attributes.is_empty() {
                attributes.sort_by(|a, b| {
                    a.spec
                        .name
                        .cmp(&b.spec.name)
                        .then_with(|| a.package.cmp(&b.package))
                });
                self.plugin_attributes = Arc::from(attributes);
            }
        }
    }

    #[cfg(feature = "native-project")]
    fn register_plugin_heads(
        &mut self,
        package: &str,
        specs: &[HeadSpec],
        owners: &mut HashMap<String, String>,
        catalog: &mut HeadCatalog,
    ) -> bool {
        let mut added = false;
        for spec in specs {
            let rule = HeadRule::from_spec(spec, false);
            if let Some(first_pkg) = owners.get(&spec.head) {
                if let Some(existing) = catalog.get(&spec.head)
                    && (
                        existing.bare,
                        &existing.declarations,
                        existing.named,
                        existing.top,
                        existing.bracket_args,
                    ) != (
                        rule.bare,
                        &rule.declarations,
                        rule.named,
                        rule.top,
                        rule.bracket_args,
                    )
                {
                    let mut d = Diagnostic::new(
                        codes::P044,
                        Severity::Error,
                        format!(
                            "plugins `{first_pkg}` and `{package}` declare conflicting syntax for block head `{}`",
                            spec.head
                        ),
                    );
                    if let Some(at) = self.dependency_span(package) {
                        d = d.with_span(at);
                    }
                    self.diagnostics.push(d);
                }
            } else {
                owners.insert(spec.head.clone(), package.to_owned());
                catalog.push(rule);
                added = true;
            }
        }
        added
    }

    #[cfg(feature = "native-project")]
    fn register_plugin_attributes(
        &mut self,
        package: &str,
        specs: &[AttributeSpec],
        attr_owners: &mut HashMap<String, String>,
        attributes: &mut Vec<PluginAttribute>,
    ) {
        for spec in specs {
            if self
                .env_ctx
                .foreign_backend
                .type_form_attribute(&spec.name)
                .is_some()
            {
                let mut d = Diagnostic::new(
                    codes::P044,
                    Severity::Error,
                    format!(
                        "plugin `{package}` cannot declare attribute `@{}`: it conflicts with the active backend's type-form attribute",
                        spec.name
                    ),
                );
                if let Some(at) = self.dependency_span(package) {
                    d = d.with_span(at);
                }
                self.diagnostics.push(d);
            } else if let Some(first_pkg) = attr_owners.get(&spec.name) {
                let leaf = spec
                    .name
                    .rsplit_once('.')
                    .map_or(spec.name.as_str(), |(_, attr)| attr);
                let pkg_ns = package.replace('-', "_");
                let hint = if spec
                    .name
                    .split_once('.')
                    .is_some_and(|(ns, _)| ns == pkg_ns)
                {
                    "rename one to avoid the collision".to_owned()
                } else {
                    format!("namespace one as `@{pkg_ns}.{leaf}`")
                };
                let mut d = Diagnostic::new(
                    codes::P044,
                    Severity::Error,
                    format!(
                        "plugins `{first_pkg}` and `{package}` both declare attribute `@{}`: {hint}",
                        spec.name
                    ),
                );
                if let Some(at) = self.dependency_span(package) {
                    d = d.with_span(at);
                }
                self.diagnostics.push(d);
            } else {
                attr_owners.insert(spec.name.clone(), package.to_owned());
                attributes.push(PluginAttribute {
                    package: package.to_owned(),
                    spec: spec.clone(),
                });
            }
        }
    }
}
