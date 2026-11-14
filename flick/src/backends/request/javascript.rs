//! Shared frontend projections. The guest receives only versioned plain
//! data and has no access to these interface lookups or host settings.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use super::{BuildPlan, Compilation, FsHost, Prepared, Project};
use crate::artifact::Artifact;
use crate::commands::build::settings::with_settings;
use flick_backend::{Abi, InputKind, OptionValue};
use flick_backend::{
    ForeignBinding, Projection, TypescriptFile, TypescriptModuleProjection, TypescriptProjection,
};
use flick_core::build::{Artifact as FrontendArtifact, Settings};
use flick_core::codegen::ts::Typing;
use flick_core::codegen::{EffectBridge, Resolved};
use flick_core::diagnostics::Diagnostic;
use flick_core::diagnostics::Severity;
use flick_core::iface::Interface;
use flick_core::ir;
use flick_core::project::config::Backend;
use flick_core::source::SourceMap;
use flick_core::{build, codegen, types};

impl Prepared {
    pub(super) fn javascript(
        &mut self,
        host: &FsHost,
        sources: &SourceMap,
        project: Option<&Project>,
        compilation: &Compilation,
        output: &BuildPlan,
        interfaces: &BTreeMap<&str, Interface>,
    ) {
        let assets = self.foreign_assets(compilation);
        if self
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
        {
            return;
        }
        with_settings(host, project, compilation, output, |settings| {
            build::javascript_preflight(compilation, sources, settings, &mut self.diagnostics);
            let entries =
                build::javascript_entries(settings, &compilation.modules, &mut self.diagnostics);
            if self
                .diagnostics
                .iter()
                .any(|d| d.severity == Severity::Error)
            {
                return;
            }
            self.javascript_facts(compilation, settings, interfaces, &entries);
            let rt = settings.backend == Backend::Rt;
            let bridge = build::javascript_effect_bridge(
                compilation,
                settings.effect_bridge,
                settings.runners,
            );
            if let Some(mode) = self
                .request
                .target
                .options
                .iter_mut()
                .find(|o| o.key == "js.runtime")
            {
                mode.value = if rt { "rt" } else { "effect" }.into();
            }
            self.request.target.options.push(OptionValue {
                key: "js.effect-bridge".into(),
                value: match bridge {
                    EffectBridge::Absent => "absent",
                    EffectBridge::Available => "available",
                    EffectBridge::AvailableWithUi => "available-with-ui",
                    EffectBridge::AvailableWithWeb => "available-with-web",
                    EffectBridge::AvailableWithUiAndWeb => "available-with-ui-and-web",
                }
                .into(),
            });
            let resolved = RefCell::new(build::javascript_foreign_inputs(compilation, settings));
            let projected =
                if output.typescript || output.extras.dts || output.extras.promise_facade {
                    self.frontend_files(compilation, settings, interfaces, &resolved)
                } else {
                    Frontend::default()
                };
            self.shared
                .extend(projected.shared.into_iter().map(Artifact::from));
            self.diagnostics.extend(projected.diagnostics);
            for (module, projection) in projected.typescript {
                let mut bytes = Vec::new();
                ciborium::into_writer(&projection, &mut bytes).expect("plain TS projection");
                let input = self.input(
                    format!("typescript/{module}"),
                    InputKind::TargetFacts,
                    bytes,
                );
                self.request.projections.push(Projection {
                    module,
                    schema: Abi {
                        name: "flick.typescript".into(),
                        version: "0.2.0".into(),
                    },
                    input,
                });
            }
            self.request
                .foreign
                .extend(resolved.into_inner().into_iter().map(|r| {
                    let inputs = if compilation.library_of(&r.module).is_some() {
                        assets.clone()
                    } else {
                        vec![]
                    };
                    ForeignBinding {
                        module: r.module,
                        specifier: r.spec,
                        resolved: r.specifier,
                        abi: Abi {
                            name: "flick.javascript-ffi".into(),
                            version: "0.1.0".into(),
                        },
                        inputs,
                    }
                }));
        });
    }
    fn frontend_files(
        &self,
        compilation: &Compilation,
        settings: &Settings<'_>,
        interfaces: &BTreeMap<&str, Interface>,
        resolved: &RefCell<Vec<Resolved>>,
    ) -> Frontend {
        let rt = settings.backend == Backend::Rt;
        let mut projected = Frontend::default();
        let mut seen = BTreeSet::new();
        let builtins: Vec<_> = types::builtin_sources()
            .iter()
            .map(|(name, _)| *name)
            .collect();
        let path = |m: &str| codegen::module_path(&builtins, settings.typescript, m);
        let package = |m: &str| {
            self.request
                .modules
                .iter()
                .find(|v| v.name == m)
                .and_then(|v| v.package.clone())
        };
        let interface = |m: &str| {
            interfaces.get(m).cloned().or_else(|| {
                types::builtins_of(rt)
                    .iter()
                    .find(|i| i.module == m)
                    .cloned()
            })
        };
        let foreign = |module: &str, spec: &str| {
            let specifier = if compilation.shipped_foreign(module, spec).is_some() {
                spec.to_owned()
            } else {
                build::javascript_foreign_specifier(settings, &path, module, spec)
            };
            let mut all = resolved.borrow_mut();
            if !all.iter().any(|r| r.module == module && r.spec == spec) {
                all.push(Resolved {
                    module: module.into(),
                    spec: spec.into(),
                    specifier: specifier.clone(),
                });
            }
            specifier
        };
        let typing = Typing {
            interface: &interface,
            package: &package,
            foreign: &foreign,
            path: &path,
            rt,
        };
        let own = compilation.modules.iter().map(|m| &m.ir);
        for module in own.chain(build::builtin_modules_of(rt)) {
            if !seen.insert(module.name.clone()) {
                continue;
            }
            if settings.typescript {
                projected
                    .typescript
                    .push((module.name.clone(), typescript(module, &typing)));
            }
            let own = interfaces.contains_key(module.name.as_str());
            if (settings.extras.dts || (own && settings.extras.promise_facade))
                && let Some(interface) = (typing.interface)(&module.name)
            {
                let (files, issues) = build::javascript_shared_files(
                    &module.name,
                    &interface,
                    &typing,
                    settings.extras.dts,
                    own && settings.extras.promise_facade,
                );
                projected.shared.extend(files);
                if own {
                    projected.diagnostics.extend(issues);
                }
            }
        }
        projected
    }
}

#[derive(Default)]
struct Frontend {
    typescript: Vec<(String, TypescriptModuleProjection)>,
    shared: Vec<FrontendArtifact>,
    diagnostics: Vec<Diagnostic>,
}

fn typescript(module: &ir::Module, typing: &Typing<'_>) -> TypescriptModuleProjection {
    let files = typing
        .prepare(module)
        .into_iter()
        .map(|(module, ty)| {
            let mut annotations: Vec<_> = ty
                .annotations
                .into_iter()
                .map(|(key, value)| OptionValue { key, value })
                .collect();
            annotations.sort_by(|a, b| a.key.cmp(&b.key));
            TypescriptFile {
                output: (typing.path)(&module),
                module,
                projection: TypescriptProjection {
                    declarations: ty.declarations,
                    annotations,
                    imports: ty.imports,
                    modules: ty.modules,
                },
            }
        })
        .collect();
    TypescriptModuleProjection { files }
}
