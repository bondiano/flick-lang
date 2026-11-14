//! Plain frontend data supplied to an isolated code generator. Compiler
//! objects and callbacks stop at this host boundary.

use std::cell::RefCell;
use std::collections::BTreeMap;

use flick_backend::{
    Abi, BackendPin, BuildRequest, Capability, Entry, Environment, Input, InputKind, Mode,
    ModuleInput, ModuleRole, OptionValue, Profile, Source, Symbol, TargetRequest,
};
use flick_core::build;
use flick_core::codegen;
use flick_core::diagnostics::Diagnostic;
use flick_core::iface::{self, Target};
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
mod javascript;
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
        let (program, files) = program(
            sources,
            compilation,
            project::stages_by_class(project, output.extras.rt),
        );
        let package = project.and_then(|p| project::project_name(host, &p.root));
        let mut data = Self {
            request: BuildRequest {
                target: target(project, output),
                backend: pin,
                program: program.to_cbor(),
                module_root: None,
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
        data.modules(host, compilation, project, package.as_deref(), output);

        data
    }

    fn modules(
        &mut self,
        host: &FsHost,
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
        for interface in types::builtins_of(project::stages_by_class(project, output.extras.rt)) {
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
        if !output.go {
            self.javascript(host, project, compilation, output, &interfaces);
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
}

fn target(project: Option<&Project>, output: &BuildPlan) -> TargetRequest {
    let environment = project
        .and_then(|p| p.config.build.environment.as_ref())
        .map_or_else(
            || Environment {
                backend: if output.go { "go" } else { "js" }.into(),
                platform: if output.go { "native" } else { "node" }.into(),
            },
            |e| Environment {
                backend: e.backend.clone(),
                platform: e.platform.clone(),
            },
        );
    let extras = output.extras;
    let mut capabilities = vec![];
    for (needed, name) in [
        (output.typescript, "typescript"),
        (extras.dts, "declarations"),
        (extras.promise_facade, "promise-facade"),
        (extras.schema_aot, "schema-aot"),
    ] {
        if needed {
            capabilities.push(Capability {
                name: name.into(),
                version: "1.0.0".into(),
            });
        }
    }
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
        mode: Mode::WholeProgram,
        required_capabilities: capabilities,
        options: vec![
            OptionValue {
                key: "command".into(),
                value: if output.purpose == Purpose::Test {
                    "test"
                } else {
                    "build"
                }
                .into(),
            },
            OptionValue {
                key: "js.runtime".into(),
                value: if extras.rt { "rt" } else { "effect" }.into(),
            },
            OptionValue {
                key: "labels".into(),
                value: format!("{:?}", extras.labels).to_lowercase(),
            },
        ],
    }
}
