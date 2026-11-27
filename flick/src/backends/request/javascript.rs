//! Shared frontend projections. The guest receives only versioned plain
//! data and has no access to these interface lookups or host settings.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use super::{BuildPlan, Compilation, FsHost, Prepared, Project};
use crate::artifact::Artifact;
use crate::commands::build::settings::with_settings;
use flick_backend::{Abi, Descriptor, InputKind, OptionValue};
use flick_backend::{
    ForeignBinding, Projection, TypescriptFile, TypescriptModuleProjection, TypescriptProjection,
};
use flick_core::build::{Artifact as FrontendArtifact, Backend, Settings};
use flick_core::codegen::ts::{HostExportSpec, Typing, library_declaration_file};
use flick_core::codegen::{EffectBridge, Resolved};
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::iface::Interface;
use flick_core::ir;
use flick_core::source::Span;
use flick_core::{build, codegen, types};

impl Prepared {
    pub(super) fn javascript(
        &mut self,
        host: &FsHost,
        project: Option<&Project>,
        compilation: &Compilation,
        output: &BuildPlan,
        interfaces: &BTreeMap<&str, Interface>,
        descriptor: &Descriptor,
    ) {
        let binding_abi = descriptor.value_abi.as_ref().map_or_else(
            || Abi {
                name: "flick.javascript-ffi".into(),
                version: "0.1.0".into(),
            },
            |v| v.abi.clone(),
        );
        let assets = self.foreign_assets(compilation);
        if self
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
        {
            return;
        }
        let effect_bridge = self.effect_bridge();
        with_settings(
            host,
            project,
            compilation,
            output,
            effect_bridge,
            |settings| {
                let entries = build::javascript_entries(
                    settings,
                    &compilation.modules,
                    &mut self.diagnostics,
                );
                if self
                    .diagnostics
                    .iter()
                    .any(|d| d.severity == Severity::Error)
                {
                    return;
                }
                self.javascript_facts(compilation, settings, interfaces, &entries);
                let resolved =
                    RefCell::new(build::javascript_foreign_inputs(compilation, settings));
                let is_library = output.options.get("kind") == Some("library");
                let wants_library_headers =
                    is_library && self.request.headers.as_ref().is_some_and(|h| h.enabled);
                let iife_global = (is_library && output.options.get("format") == Some("iife"))
                    .then(|| output.options.get("global-name").unwrap_or("FlickLib"));
                let projected = if output.typescript
                    || output.extras.dts
                    || output.extras.promise_facade
                    || wants_library_headers
                {
                    self.frontend_files(
                        compilation,
                        settings,
                        interfaces,
                        &resolved,
                        wants_library_headers,
                        iife_global,
                    )
                } else {
                    Frontend::default()
                };
                self.shared
                    .extend(projected.shared.into_iter().map(Artifact::from));
                self.diagnostics.extend(projected.diagnostics);
                self.attach_typescript_projections(projected.typescript);
                self.attach_foreign_bindings(
                    compilation,
                    resolved.into_inner(),
                    &binding_abi,
                    &assets,
                );
            },
        );
    }

    fn effect_bridge(&self) -> EffectBridge {
        if self
            .request
            .declared_dependencies
            .as_ref()
            .is_none_or(|deps| deps.iter().any(|d| d == "@flick-lang/effect-ts"))
        {
            EffectBridge::Available
        } else {
            EffectBridge::Absent
        }
    }

    fn attach_typescript_projections(
        &mut self,
        projections: Vec<(String, TypescriptModuleProjection)>,
    ) {
        for (module, projection) in projections {
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
    }

    fn attach_foreign_bindings(
        &mut self,
        compilation: &Compilation,
        resolved: Vec<Resolved>,
        binding_abi: &Abi,
        assets: &[String],
    ) {
        self.request.foreign.extend(resolved.into_iter().map(|r| {
            let inputs = if compilation.library_of(&r.module).is_some() {
                assets.to_vec()
            } else {
                vec![]
            };
            ForeignBinding {
                module: r.module,
                specifier: r.spec,
                resolved: r.specifier,
                abi: binding_abi.clone(),
                inputs,
            }
        }));
    }
    fn frontend_files(
        &self,
        compilation: &Compilation,
        settings: &Settings<'_>,
        interfaces: &BTreeMap<&str, Interface>,
        resolved: &RefCell<Vec<Resolved>>,
        wants_library_headers: bool,
        iife_global: Option<&str>,
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
            natives: compilation.native_overrides.as_ref(),
        };
        let library_needs_module_dts = if wants_library_headers {
            self.project_library_headers(compilation, &typing, iife_global, &mut projected)
        } else {
            false
        };
        let emit_module_dts = settings.extras.dts || library_needs_module_dts;
        let own = compilation.modules.iter().map(|m| &*m.ir);
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
            if (emit_module_dts || (own && settings.extras.promise_facade))
                && let Some(interface) = (typing.interface)(&module.name)
            {
                let (files, issues) = build::javascript_shared_files(
                    &module.name,
                    &interface,
                    &typing,
                    emit_module_dts,
                    own && settings.extras.promise_facade,
                );
                projected.shared.extend(files);
                if own && settings.extras.dts {
                    projected.diagnostics.extend(issues);
                }
            }
        }
        projected
    }

    fn project_library_headers(
        &self,
        compilation: &Compilation,
        typing: &Typing<'_>,
        iife_global: Option<&str>,
        projected: &mut Frontend,
    ) -> bool {
        let declarations = |m: &str| {
            compilation
                .modules
                .iter()
                .find(|c| c.name == m)
                .map(|c| (*c.declarations).clone())
        };
        let specs: Vec<HostExportSpec<'_>> = self
            .request
            .exports
            .iter()
            .map(|export| HostExportSpec {
                export_name: export.name.as_str(),
                module: export.symbol.module.as_str(),
                symbol: export.symbol.name.as_str(),
                span: Span::new(
                    self.files[export.span.file as usize],
                    export.span.start,
                    export.span.end,
                ),
            })
            .collect();
        let index_import = |m: &str| codegen::relative("index.d.ts", &(typing.path)(m));
        let (dts, used, refused) = library_declaration_file(
            &specs,
            typing.interface,
            &declarations,
            typing.package,
            &index_import,
            typing.foreign,
            typing.rt,
            iife_global,
        );
        if refused.is_empty() {
            projected.shared.push(FrontendArtifact {
                path: "index.d.ts".into(),
                contents: dts,
            });
            !used.is_empty()
        } else {
            for r in refused {
                projected.diagnostics.push(Diagnostic::error(
                    codes::F004,
                    format!(
                        "`@export \"{}\"` (`{}.{}`) has no TypeScript form: {}",
                        r.export_name, r.module, r.symbol, r.reason
                    ),
                    r.span,
                ));
            }
            false
        }
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
