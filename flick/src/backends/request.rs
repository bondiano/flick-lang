//! Plain frontend data supplied to an isolated code generator. Compiler
//! objects and callbacks stop at this host boundary.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::env;

use flick_backend::{
    Abi, BackendPin, BuildRequest, Entry, Environment, Input, InputKind, Mode, ModuleInput,
    ModuleRole, OptionValue, PackageTarget, Profile, RootHandler, Source, Symbol, TargetRequest,
};
use std::collections::HashSet;

use flick_core::build;
use flick_core::codegen;
use flick_core::diagnostics::Diagnostic;
use flick_core::iface::{self, Target};
use flick_core::project::config::BackendEnvironment;
use flick_core::project::{self, Compilation, Project};
use flick_core::source::{FileId, SourceMap};
use flick_core::{ir, types};
use flick_ir::Program;

use crate::artifact::Artifact;
use crate::commands::build::{BuildPlan, Purpose};
use crate::sources::FsHost;
use crate::toolchain;

mod assets;
#[cfg(test)]
mod assets_tests;
#[cfg(test)]
mod component_tests;
mod go;
mod javascript;
mod javascript_facts;
#[cfg(test)]
mod projection_tests;
#[cfg(test)]
mod tests;

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
    let mut modules: Vec<_> = compilation.modules.iter().map(|m| m.ir.clone()).collect();
    for builtin in &builtins.modules {
        if compilation.modules.iter().any(|m| m.name == builtin.name) {
            continue;
        }
        let mut module = builtin.clone();
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
    pub(super) fn new(
        host: &FsHost,
        sources: &mut SourceMap,
        project: Option<&Project>,
        compilation: &Compilation,
        output: &BuildPlan,
        pin: BackendPin,
    ) -> Self {
        let (program, files) = program(sources, compilation, by_class(project, output));
        let package = project.and_then(|p| project::project_name(host, p));
        let (target_request, module_root) = target(project, output);
        let mut data = Self {
            request: BuildRequest {
                target: target_request,
                backend: pin,
                program: program.to_cbor(),
                module_root,
                modules: vec![],
                sources: vec![],
                entries: output
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
                    .collect(),
                test_roots: output
                    .suites
                    .iter()
                    .map(|module| Symbol {
                        module: module.clone(),
                        name: "tests".into(),
                    })
                    .collect(),
                foreign: vec![],
                projections: vec![],
                package_targets: vec![],
                root_handlers: vec![],
                inputs: vec![],
                toolchains: vec![],
                runtime_dependencies: vec![],
            },
            bytes: BTreeMap::new(),
            files,
            shared: vec![],
            diagnostics: vec![],
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
        data.modules(
            host,
            sources,
            compilation,
            project,
            package.as_deref(),
            output,
        );
        data.fill_environment_facts(package.as_deref());

        data
    }

    fn modules(
        &mut self,
        host: &FsHost,
        sources: &SourceMap,
        compilation: &Compilation,
        project: Option<&Project>,
        package: Option<&str>,
        output: &BuildPlan,
    ) {
        let interfaces = build::published_interfaces(
            compilation,
            package,
            project.map(|p| p.root.as_path()),
            match self.request.target.environment.backend.as_str() {
                "js" => Some(&[Target::Js]),
                "go" => Some(&[Target::Go]),
                _ => None,
            },
        );
        let builtins: Vec<&str> = types::builtin_sources()
            .iter()
            .map(|(name, _)| *name)
            .collect();
        for module in &compilation.modules {
            let interface = &interfaces[module.name.as_str()];
            let text = iface::write(interface);
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
        for interface in types::builtins_of(by_class(project, output)) {
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
                iface::write(interface).into_bytes(),
            );
            self.request.modules.push(ModuleInput {
                name: interface.module.clone(),
                role: ModuleRole::Builtin,
                package: None,
                output: codegen::module_path(&builtins, output.typescript, &interface.module),
                interface: Some(input),
            });
        }
        if self.request.target.environment.backend == "go" {
            self.go(host, project, compilation, output);
        } else {
            self.javascript(host, sources, project, compilation, output, &interfaces);
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
    fn fill_environment_facts(&mut self, package: Option<&str>) {
        let environment = &self.request.target.environment;
        let selected = BackendEnvironment {
            backend: environment.backend.clone(),
            platform: environment.platform.clone(),
        };
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
        self.request.root_handlers = build::target_preflight::std_root_handlers(&selected)
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

/// The built-in modules a build compiles: the go target's are always those
/// of the non-class stages, as its runtime implements them.
fn by_class(project: Option<&Project>, output: &BuildPlan) -> bool {
    !output.go && project::stages_by_class(project, Some(output.extras.rt))
}

fn target(project: Option<&Project>, output: &BuildPlan) -> (TargetRequest, Option<String>) {
    let default_env = || {
        output
            .backend
            .descriptor()
            .and_then(|d| d.environments.into_iter().next())
            .unwrap_or_else(|| Environment {
                backend: if output.go { "go" } else { "js" }.into(),
                platform: if output.go { "native" } else { "node" }.into(),
            })
    };
    let environment = project
        .and_then(|p| p.config.build.environment.as_ref())
        .filter(|e| !output.target_overridden || e.backend == output.target)
        .map_or_else(default_env, |e| Environment {
            backend: e.backend.clone(),
            platform: e.platform.clone(),
        });
    let extras = output.extras;
    let option = |key: &str, value: &str| OptionValue {
        key: key.into(),
        value: value.into(),
    };
    let command = option(
        "command",
        match output.purpose {
            Purpose::Test => "test",
            Purpose::Run => "run",
            Purpose::Dev => "dev",
            Purpose::Build => "build",
            Purpose::Inspect => "inspect",
        },
    );
    let mut options = vec![command];
    if environment.backend == "go" {
        options.push(option("host.os", env::consts::OS));
    } else {
        options.push(option(
            "labels",
            &format!("{:?}", extras.labels).to_lowercase(),
        ));
    }
    options.extend(output.options.to_wire());

    let (mode, module_root) = if output.purpose == Purpose::Inspect {
        let supports_per_module = output
            .backend
            .descriptor()
            .is_some_and(|d| d.modes.contains(&Mode::PerModule));
        if supports_per_module {
            (Mode::PerModule, output.inspect_module.clone())
        } else {
            if let Some(module) = &output.inspect_module {
                options.push(option("inspect.module", module));
            }
            (Mode::WholeProgram, None)
        }
    } else {
        (Mode::WholeProgram, None)
    };

    (
        TargetRequest {
            backend_api: flick_backend::VERSION.into(),
            ir_version: flick_ir::VERSION.into(),
            language_version: toolchain::COMPILER.into(),
            runtime_abis: vec![Abi {
                name: "flickrt".into(),
                version: toolchain::RUNTIME.into(),
            }],
            environment,
            profile: if output.release {
                Profile::Release
            } else {
                Profile::Debug
            },
            mode,
            required_capabilities: vec![],
            options,
        },
        module_root,
    )
}
