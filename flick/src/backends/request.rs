//! Plain frontend data supplied to an isolated code generator. Compiler
//! objects and callbacks stop at this host boundary.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::time::Instant;

use flick_backend::{
    Abi, BackendPin, BuildRequest, Descriptor, Entry, ForeignBinding, Input, InputKind,
    ModuleInput, ModuleRole, PackageTarget, RootHandler, Source, Symbol,
};
#[cfg(test)]
use flick_backend::{Mode, Profile};

use flick_core::build;
use flick_core::codegen;
use flick_core::diagnostics::{Diagnostic, Severity, codes};
use flick_core::iface::{self, Target};
use flick_core::project::{self, Compilation, Host, Project};
use flick_core::source::{FileId, SourceMap};
use flick_core::{ir, types};
use flick_ir::Program;

use crate::artifact::Artifact;
use crate::commands::build::BuildPlan;
#[cfg(test)]
use crate::commands::build::Purpose;
use crate::commands::build::settings::with_settings;
use crate::sources::FsHost;
use crate::timings;
use crate::toolchain;

mod assets;
#[cfg(test)]
mod assets_tests;
#[cfg(test)]
mod component_tests;
mod exports;
mod go;
#[cfg(test)]
mod go_tests;
mod inline;
mod javascript;
mod javascript_facts;
#[cfg(test)]
mod projection_tests;
mod target;
#[cfg(test)]
mod tests;

pub(super) use exports::{check_header_request, collect_own_host_exports};
pub(super) use inline::collect_inline_foreign;
pub(super) use target::runtime_abis;
use target::target;

pub(super) struct Prepared {
    pub request: BuildRequest,
    pub bytes: BTreeMap<String, Vec<u8>>,
    pub files: Vec<FileId>,
    pub shared: Vec<Artifact>,
    pub diagnostics: Vec<Diagnostic>,
}

pub(super) fn program(
    sources: &mut SourceMap,
    compilation: &Compilation,
    stages_by_class: bool,
) -> (Program, Vec<FileId>) {
    let files = RefCell::new(Vec::new());
    let builtins = build::builtin_bundle_of(stages_by_class);
    let builtin_files: Vec<_> = builtins
        .sources
        .iter()
        .map(|s| {
            let existing = sources
                .files()
                .position(|f| f.path == s.name && f.text == s.text);
            existing.map_or_else(
                || sources.add(&s.name, &s.text),
                |i| FileId::new(u32::try_from(i).expect("source count fits u32")),
            )
        })
        .collect();
    let mut modules: Vec<_> = compilation
        .modules
        .iter()
        .map(|m| (*m.ir).clone())
        .collect();
    for (i, builtin) in builtins.modules.iter().enumerate() {
        if compilation.modules.iter().any(|m| m.name == builtin.name) {
            continue;
        }
        // its `@native` clauses as the build's environment selects them
        let mut module = builtins.module_for(i, compilation.native_overrides.as_ref());
        module.each_span_mut(&mut |s| s.file = builtin_files[s.file.index() as usize]);
        modules.push(module);
    }
    // Target optimization belongs in the component. This boundary always
    // exports the reference lowering, with its original source spans.
    let program = Program::new(
        &format!("flick {}", toolchain::COMPILER),
        modules,
        ir::lang_items(),
        &|file| {
            files.borrow_mut().push(file);
            sources.get(file).path.clone()
        },
    );
    (program, files.into_inner())
}

impl Prepared {
    #[allow(clippy::too_many_lines)]
    pub(super) fn new(
        host: &FsHost,
        sources: &mut SourceMap,
        project: Option<&Project>,
        compilation: &Compilation,
        output: &BuildPlan,
        pin: BackendPin,
        descriptor: &Descriptor,
    ) -> Self {
        let timing = timings::enabled();
        let t_preflight = timing.then(Instant::now);
        let package = project.and_then(|p| project::project_name(host, p));
        let (target_request, module_root) = target(project, compilation, output, descriptor);
        let declared_dependencies = declared_dependencies(host, project, compilation, output);
        let preflight_diagnostics = javascript_preflight(
            host,
            sources,
            project,
            compilation,
            output,
            descriptor,
            declared_dependencies.as_deref(),
        );
        let pruned;
        let compilation = if preflight_diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
        {
            compilation
        } else {
            pruned = compilation.prune_for_environment(sources, output.stages_by_class);
            &pruned
        };
        if let Some(t) = t_preflight {
            timings::phase("backend prepare: preflight & prune", t.elapsed());
        }
        let t_program = timing.then(Instant::now);
        let (program, files) = program(sources, compilation, output.stages_by_class);
        if let Some(t) = t_program {
            timings::phase("backend prepare: program build", t.elapsed());
        }
        let t_cbor = timing.then(Instant::now);
        let program_cbor = program.to_cbor();
        let program_bytes = u64::try_from(program_cbor.len()).unwrap_or(u64::MAX);
        if let Some(t) = t_cbor {
            timings::phase("backend prepare: program CBOR encode", t.elapsed());
        }
        let t_proj = timing.then(Instant::now);
        let inline_foreign = collect_inline_foreign(&program);
        let host_exports = exports::collect_own_host_exports(&program, compilation);
        let headers = exports::header_request(output, descriptor);
        let mut data = Self {
            request: BuildRequest {
                target: target_request,
                backend: pin,
                program: program_cbor,
                module_root,
                modules: vec![],
                sources: vec![],
                entries: request_entries(output, project),
                test_roots: output
                    .suites
                    .iter()
                    .map(|module| Symbol {
                        module: module.clone(),
                        name: "tests".into(),
                    })
                    .collect(),
                foreign: vec![],
                inline_foreign,
                projections: vec![],
                package_targets: vec![],
                root_handlers: vec![],
                inputs: vec![],
                toolchains: vec![],
                runtime_dependencies: vec![],
                declared_dependencies,
                exports: host_exports,
                headers,
            },
            bytes: BTreeMap::new(),
            files,
            shared: vec![],
            diagnostics: preflight_diagnostics,
        };
        for (i, name) in program.files.into_iter().enumerate() {
            let file = sources.get(data.files[i]);
            let input = data.input(
                format!("source/{i}"),
                InputKind::Source,
                file.text.as_bytes().to_vec(),
            );
            data.request.sources.push(Source {
                name,
                input,
                line_starts: file.line_starts().to_vec(),
            });
        }
        match data.modules(
            host,
            compilation,
            project,
            package.as_deref(),
            output,
            descriptor,
        ) {
            Ok(()) => data.fill_environment_facts(descriptor, package.as_deref()),
            Err(error) => {
                data.shared.clear();
                data.diagnostics
                    .push(Diagnostic::new(codes::P010, Severity::Error, error));
            }
        }
        if let Some(t) = t_proj {
            timings::phase("backend prepare: projections & inputs", t.elapsed());
            let inputs_bytes: u64 = data
                .request
                .inputs
                .iter()
                .fold(0_u64, |acc, input| acc.saturating_add(input.size));
            timings::bytes("program_bytes", program_bytes);
            timings::bytes("inputs_bytes", inputs_bytes);
        }

        data
    }

    #[allow(clippy::too_many_arguments)]
    fn modules(
        &mut self,
        host: &FsHost,
        compilation: &Compilation,
        project: Option<&Project>,
        package: Option<&str>,
        output: &BuildPlan,
        descriptor: &Descriptor,
    ) -> Result<(), String> {
        let interfaces = build::published_interfaces(
            compilation,
            package,
            project.map(|p| p.root.as_path()),
            match self.request.target.environment.backend.as_str() {
                "js" => Some(&[Target::Js]),
                "go" => Some(&[Target::Go]),
                _ => None,
            },
        )?;
        let builtins: Vec<&str> = types::builtin_sources()
            .iter()
            .map(|(name, _)| *name)
            .collect();
        for module in &compilation.modules {
            let interface = &interfaces[module.name.as_str()];
            let text = iface::write(interface)?;
            let input = self.input(
                format!("interface/{}", module.name),
                InputKind::Interface,
                text.as_bytes().to_vec(),
            );
            self.shared.push(Artifact::source(
                format!("{}.iface", module.name.replace('.', "/")),
                text,
            ));
            self.request.modules.push(ModuleInput {
                name: module.name.clone(),
                role: ModuleRole::Own,
                package: module
                    .ir
                    .package
                    .clone()
                    .or_else(|| module.package.clone())
                    .or_else(|| package.map(str::to_owned)),
                output: codegen::module_path(&builtins, output.typescript, &module.name),
                interface: Some(input),
            });
        }
        // Built-ins have explicit lowered bodies and the actual texts used
        // during lowering. They never alias the project's source identities.
        for interface in types::builtins_of(output.stages_by_class) {
            if self
                .request
                .modules
                .iter()
                .any(|m| m.name == interface.module)
            {
                continue;
            }
            let input = self.input(
                format!("interface/{}", interface.module),
                InputKind::Interface,
                iface::write(interface)?.into_bytes(),
            );
            self.request.modules.push(ModuleInput {
                name: interface.module.clone(),
                role: ModuleRole::Builtin,
                package: None,
                output: codegen::module_path(&builtins, output.typescript, &interface.module),
                interface: Some(input),
            });
        }
        let wants = |name: &str| descriptor.projections.iter().any(|p| p.name == name);
        if wants("flick.go") {
            self.go(host, project, compilation, output, descriptor);
        }
        if wants("flick.javascript") || wants("flick.typescript") {
            self.javascript(host, project, compilation, output, &interfaces, descriptor);
        } else if !wants("flick.go")
            && let Some(value_abi) = &descriptor.value_abi
        {
            self.generic_foreign_bindings(compilation, &value_abi.abi);
        }
        Ok(())
    }

    fn generic_foreign_bindings(&mut self, compilation: &Compilation, abi: &Abi) {
        let mut seen = HashSet::new();
        for module in &compilation.modules {
            for item in &module.ir.items {
                if let flick_ir::ItemKind::Foreign(foreign) = &item.kind
                    && let flick_ir::Binding::Module { from, .. } = &foreign.binding
                    && !from.starts_with("flick:")
                    && seen.insert((module.name.as_str(), from.as_str()))
                {
                    self.request.foreign.push(ForeignBinding {
                        module: module.name.clone(),
                        specifier: from.clone(),
                        resolved: from.clone(),
                        abi: abi.clone(),
                        inputs: Vec::new(),
                    });
                }
            }
        }
    }

    fn input(&mut self, id: String, kind: InputKind, bytes: Vec<u8>) -> String {
        self.request.inputs.push(Input {
            id: id.clone(),
            kind,
            sha256: flick_backend::sha256(&bytes),
            size: u64::try_from(bytes.len()).expect("host input fits u64"),
        });
        self.bytes.insert(id.clone(), bytes);
        id
    }

    /// Record package targets and std root handlers for the selected environment.
    fn fill_environment_facts(&mut self, descriptor: &Descriptor, package: Option<&str>) {
        let environment = &self.request.target.environment;
        let mut packages: Vec<String> = self
            .request
            .modules
            .iter()
            .filter_map(|module| module.package.clone())
            .collect();
        if let Some(name) = package {
            packages.push(name.to_owned());
        }
        packages.sort();
        packages.dedup();
        let present: HashSet<_> = self
            .request
            .modules
            .iter()
            .map(|module| module.name.as_str())
            .collect();
        let environments = vec![environment.clone()];
        self.request.package_targets = packages
            .into_iter()
            .map(|package| PackageTarget {
                package,
                environments: environments.clone(),
            })
            .collect();
        let services: Vec<&str> = descriptor
            .handled_services
            .iter()
            .filter(|entry| {
                entry.environment.backend == environment.backend
                    && entry.environment.platform == environment.platform
            })
            .flat_map(|entry| entry.services.iter().map(String::as_str))
            .collect();
        self.request.root_handlers = build::target_preflight::std_root_handlers_for(services)
            .into_iter()
            .filter(|(service_module, _, handler_module, _)| {
                present.contains(service_module) && present.contains(handler_module)
            })
            .map(
                |(service_module, service_name, handler_module, handler_name)| RootHandler {
                    environment: environment.clone(),
                    service: Symbol {
                        module: service_module.into(),
                        name: service_name.into(),
                    },
                    handler: Symbol {
                        module: handler_module.into(),
                        name: handler_name.into(),
                    },
                },
            )
            .collect();
    }
}

fn declared_dependencies(
    host: &FsHost,
    project: Option<&Project>,
    compilation: &Compilation,
    output: &BuildPlan,
) -> Option<Vec<String>> {
    project.map(|p| {
        let mut deps = host.foreign(p).declared;
        deps.extend(
            compilation
                .target_dependencies
                .iter()
                .filter(|d| d.target == output.target)
                .map(|d| d.name.clone()),
        );
        deps.sort();
        deps.dedup();
        deps
    })
}

fn javascript_preflight(
    host: &FsHost,
    sources: &SourceMap,
    project: Option<&Project>,
    compilation: &Compilation,
    output: &BuildPlan,
    descriptor: &Descriptor,
    declared_dependencies: Option<&[String]>,
) -> Vec<Diagnostic> {
    let wants = |name: &str| descriptor.projections.iter().any(|p| p.name == name);
    let mut diagnostics = Vec::new();
    if wants("flick.javascript") || wants("flick.typescript") {
        let effect_bridge = if declared_dependencies
            .is_none_or(|deps| deps.iter().any(|d| d == "@flick-lang/effect-ts"))
        {
            codegen::EffectBridge::Available
        } else {
            codegen::EffectBridge::Absent
        };
        with_settings(
            host,
            project,
            compilation,
            output,
            effect_bridge,
            |settings| {
                build::javascript_preflight(compilation, sources, settings, &mut diagnostics);
            },
        );
    }
    diagnostics
}

fn request_entries(output: &BuildPlan, project: Option<&Project>) -> Vec<Entry> {
    output
        .entries
        .iter()
        .map(|(name, module)| Entry {
            name: name.clone(),
            symbol: Symbol {
                module: module.clone(),
                name: "main".into(),
            },
            runner: project
                .and_then(|p| p.config.entries.iter().find(|e| e.name == *name))
                .and_then(|e| e.runner.as_ref().map(|r| r.0.clone())),
        })
        .collect()
}
