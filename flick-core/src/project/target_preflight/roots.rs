//! Root seeding for app entries, runners, test suites, and library public
//! exports during environment preflight.

use std::path::PathBuf;

use super::refs::{module_reexports, reexport_span};
use super::{CheckOptions, Preflight, RunnerRef, WorkItem};
use crate::iface::Namespace;
use crate::ir::{ItemKind, reach};
use crate::source::Span;
use crate::types::builtins_of;

impl Preflight<'_> {
    pub(super) fn seed_roots(&mut self, options: &CheckOptions<'_>) {
        if options.for_tests {
            self.seed_test_roots(options.test_paths);
            return;
        }
        if !options.entries.is_empty() {
            self.seed_app_roots(options.entries, options.runners, options.by_class);
            self.seed_host_export_roots();
            return;
        }
        self.seed_library_roots();
        self.seed_host_export_roots();
    }

    fn seed_host_export_roots(&mut self) {
        let exports: Vec<(String, String, Span)> = self
            .compilation
            .modules
            .iter()
            .filter(|m| m.package.is_none() && !m.test)
            .filter(|m| self.module_unavailable(&m.name).is_none())
            .flat_map(|m| {
                m.ir.exports
                    .host
                    .iter()
                    .map(|e| (m.name.clone(), e.name.clone(), e.span))
            })
            .collect();
        for (mod_name, sym, span) in exports {
            let root = self.add_node(format!("{mod_name}.{sym}"), None);
            self.mark_loaded(&mod_name, root);
            self.queue.push_back(WorkItem {
                key: (mod_name, sym),
                origin: Some(span),
                node: root,
                covered: 0,
            });
        }
    }

    fn seed_test_roots(&mut self, test_paths: &[PathBuf]) {
        let suites: Vec<(String, Span)> = self
            .compilation
            .modules
            .iter()
            .filter(|m| m.test && m.interface.exports_value("tests"))
            .filter(|m| test_paths.is_empty() || test_paths.contains(&m.path))
            .filter_map(|m| {
                if self.module_unavailable(&m.name).is_some() {
                    return None;
                }
                let item =
                    m.ir.items
                        .iter()
                        .find(|i| reach::defined(i).iter().any(|n| n == "tests"))?;
                Some((m.name.clone(), item.span))
            })
            .collect();
        for (module, span) in suites {
            let root = self.add_node(format!("{module}.tests"), None);
            self.mark_loaded(&module, root);
            self.queue.push_back(WorkItem {
                key: (module, "tests".to_owned()),
                origin: Some(span),
                node: root,
                covered: 0,
            });
        }
    }

    #[allow(clippy::too_many_lines)]
    fn seed_app_roots(
        &mut self,
        entries: &[(String, String, Option<Span>)],
        runners: &[RunnerRef],
        by_class: bool,
    ) {
        for (entry_name, entry_mod, entry_span) in entries {
            let Some(compiled) = self
                .compilation
                .modules
                .iter()
                .find(|m| &m.name == entry_mod)
            else {
                continue;
            };
            let root_label = format!("{entry_mod}.main (entry {entry_name})");
            let root_node = self.add_node(root_label, None);
            if let Some(reason) = self.module_unavailable(entry_mod) {
                self.report_unavailable(&reason, *entry_span, root_node, None);
                continue;
            }
            self.mark_loaded(entry_mod, root_node);
            if let Some(export) = compiled.interface.find(Namespace::Value, "main") {
                let target_mod = export.entity.module.clone();
                let target_name = export.entity.name.clone();
                let via = export.via.clone();
                let main_span = export
                    .at
                    .and_then(|[start, end]| {
                        let def = self
                            .compilation
                            .modules
                            .iter()
                            .find(|m| m.name == target_mod)?;
                        Some(Span::new(def.file, start, end))
                    })
                    .or(*entry_span);
                let reexport_at = if target_mod == *entry_mod {
                    main_span
                } else {
                    reexport_span(compiled, &target_name, self.sources).or(*entry_span)
                };
                self.check_via_chain(&via, &target_name, reexport_at, root_node);
                if let Some(reason) = self.module_unavailable(&target_mod) {
                    self.report_unavailable(
                        &reason,
                        reexport_at,
                        root_node,
                        Some(&format!("{target_mod}.{target_name}")),
                    );
                } else {
                    let key_node = self.add_node(format!("{target_mod}.{target_name}"), None);
                    self.mark_loaded(&target_mod, key_node);
                    self.queue.push_back(WorkItem {
                        key: (target_mod, target_name),
                        origin: main_span,
                        node: key_node,
                        covered: 0,
                    });
                }
            }
        }

        for runner in runners {
            let runner_label = format!("{} (runner for {})", runner.module, runner.entry);
            let runner_node = self.add_node(runner_label, None);
            if let Some(reason) = self.module_unavailable(&runner.module) {
                self.report_unavailable(&reason, runner.span, runner_node, None);
                continue;
            }
            let interface = self
                .compilation
                .modules
                .iter()
                .find(|m| m.name == runner.module)
                .map(|m| &m.interface)
                .or_else(|| {
                    builtins_of(by_class)
                        .iter()
                        .find(|m| m.module == runner.module)
                });
            let Some(export) = interface.and_then(|m| m.find(Namespace::Value, "run")) else {
                continue;
            };
            let key = (export.entity.module.clone(), export.entity.name.clone());
            let via = export.via.clone();
            self.check_via_chain(&via, &key.1, runner.span, runner_node);
            if let Some(reason) = self.module_unavailable(&key.0) {
                self.report_unavailable(
                    &reason,
                    runner.span,
                    runner_node,
                    Some(&format!("{}.{}", key.0, key.1)),
                );
                continue;
            }
            let fallback_span = runner.span.or_else(|| {
                self.compilation
                    .modules
                    .iter()
                    .flat_map(|m| &m.ir.items)
                    .next()
                    .map(|item| item.span)
            });
            self.mark_loaded(&runner.module, runner_node);
            self.mark_loaded(&key.0, runner_node);
            self.queue.push_back(WorkItem {
                key,
                origin: fallback_span,
                node: runner_node,
                covered: 0,
            });
        }
    }

    fn seed_library_roots(&mut self) {
        // Library export policy: own public modules whose own `@when` (and own
        // package's `[package] when`) are satisfied in this environment are
        // roots; modules gated out for this environment are skipped.
        let own_public: Vec<usize> = self
            .compilation
            .modules
            .iter()
            .enumerate()
            .filter(|(_, m)| m.package.is_none() && !m.interface.private && !m.test)
            .filter(|(_, m)| self.module_unavailable(&m.name).is_none())
            .map(|(idx, _)| idx)
            .collect();

        for idx in own_public {
            let mod_name = self.compilation.modules[idx].name.clone();
            let mod_node = self.add_node(mod_name.clone(), None);
            self.mark_loaded(&mod_name, mod_node);

            // Re-exported values of this public library module.
            let reexports = module_reexports(&self.compilation.modules[idx], self.sources);
            for (key, via, span) in reexports {
                let export_node = self.add_node(format!("{mod_name}.{}", key.1), None);
                self.check_via_chain(&via, &key.1, Some(span), export_node);
                if let Some(reason) = self.module_unavailable(&key.0) {
                    self.report_unavailable(
                        &reason,
                        Some(span),
                        export_node,
                        Some(&format!("{}.{}", key.0, key.1)),
                    );
                } else {
                    self.mark_loaded(&key.0, export_node);
                    self.queue.push_back(WorkItem {
                        key,
                        origin: Some(span),
                        node: export_node,
                        covered: 0,
                    });
                }
            }

            // Own exported values and instances.
            let items: Vec<(String, Span)> = self.compilation.modules[idx]
                .ir
                .items
                .iter()
                .flat_map(|item| {
                    let is_instance = matches!(item.kind, ItemKind::Instance { .. });
                    let m = &self.compilation.modules[idx];
                    reach::defined(item)
                        .into_iter()
                        .filter(move |name| m.interface.exports_value(name) || is_instance)
                        .map(|name| (name, item.span))
                })
                .collect();
            for (name, span) in items {
                let item_node = self.add_node(format!("{mod_name}.{name}"), None);
                self.queue.push_back(WorkItem {
                    key: (mod_name.clone(), name),
                    origin: Some(span),
                    node: item_node,
                    covered: 0,
                });
            }
        }
    }
}
