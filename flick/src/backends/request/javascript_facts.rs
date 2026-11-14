//! Plain frontend facts that the JS component cannot derive by typechecking.

use std::collections::BTreeMap;

use flick_backend::{
    Abi, InputKind, JavascriptEntry, JavascriptFile, JavascriptProjection, Projection,
};
use flick_core::build::{self, EntryFacts, Settings};
use flick_core::iface::{Interface, Namespace};
use flick_core::project::{Compilation, config::Backend};
use flick_core::{codegen, types};

use super::Prepared;

impl Prepared {
    pub(super) fn javascript_facts(
        &mut self,
        compilation: &Compilation,
        settings: &Settings<'_>,
        interfaces: &BTreeMap<&str, Interface>,
        entries: &[EntryFacts],
    ) {
        let builtin_names: Vec<_> = types::builtin_sources()
            .iter()
            .map(|(name, _)| *name)
            .collect();
        let rt = settings.backend == Backend::Rt;
        for meta in self.request.modules.clone() {
            let own = compilation
                .modules
                .iter()
                .find(|module| module.name == meta.name);
            let interface = interfaces.get(meta.name.as_str()).or_else(|| {
                types::builtins_of(rt)
                    .iter()
                    .find(|i| i.module == meta.name)
            });
            let Some(interface) = interface else {
                continue;
            };
            let mut physical = vec![meta.name.clone()];
            if let Some(module) = own {
                physical.extend(
                    module
                        .ir
                        .initialization
                        .prefixes
                        .iter()
                        .map(|p| codegen::prefix_key(&meta.name, p.before)),
                );
            }
            let facts = JavascriptProjection {
                builtin: builtin_names.contains(&meta.name.as_str()),
                exports: interface
                    .exports
                    .iter()
                    .filter(|e| {
                        e.entity.namespace == Namespace::Value && e.entity.module == meta.name
                    })
                    .map(|e| e.entity.name.clone())
                    .collect(),
                entries: entries
                    .iter()
                    .filter(|entry| {
                        self.request
                            .entries
                            .iter()
                            .any(|e| e.name == entry.name && e.symbol.module == meta.name)
                    })
                    .map(|e| JavascriptEntry {
                        name: e.name.clone(),
                        module: e.defining.clone(),
                        pure: e.pure,
                    })
                    .collect(),
                files: physical
                    .into_iter()
                    .map(|module| {
                        let output =
                            codegen::module_path(&builtin_names, settings.typescript, &module);
                        let source_paths = self
                            .files
                            .iter()
                            .map(|&file| build::javascript_source_path(settings, &output, file))
                            .collect();
                        JavascriptFile {
                            module,
                            output,
                            source_paths,
                        }
                    })
                    .collect(),
            };
            let mut bytes = Vec::new();
            ciborium::into_writer(&facts, &mut bytes).expect("plain JS facts");
            let input = self.input(
                format!("javascript/{}", meta.name),
                InputKind::TargetFacts,
                bytes,
            );
            self.request.projections.push(Projection {
                module: meta.name,
                schema: Abi {
                    name: "flick.javascript".into(),
                    version: "0.1.0".into(),
                },
                input,
            });
        }
    }
}
