//! Lightweight `BuildRequest` construction for backend `check-target` during
//! `flick check` and editor analysis.

use std::collections::{BTreeMap, HashSet};
use std::env;

use flick_backend::{
    Abi, BackendPin, BuildRequest, Descriptor, Environment, InputKind, Mode, ModuleInput,
    ModuleRole, OptionValue, Profile, Source, TargetRequest,
};
use flick_core::codegen;
use flick_core::project::{Compilation, ResolvedEnvironment};
use flick_core::source::SourceMap;
use flick_core::types;

use super::{Prepared, program};
use crate::toolchain;

impl Prepared {
    /// Lightweight request for backend `check-target` during `flick check` and
    /// editor analysis (no projections, foreign files, or entry classification).
    #[allow(clippy::too_many_arguments)]
    pub(in crate::backends) fn for_check(
        sources: &mut SourceMap,
        compilation: &Compilation,
        environment: &ResolvedEnvironment,
        stages_by_class: bool,
        pin: BackendPin,
        descriptor: &Descriptor,
        extra_options: Vec<OptionValue>,
    ) -> Self {
        let (program, files) = program(sources, compilation, stages_by_class);
        let option = |key: &str, value: &str| OptionValue {
            key: key.into(),
            value: value.into(),
        };
        let mut options = vec![
            option("command", "check"),
            option("host.os", env::consts::OS),
            option("labels", "none"),
        ];
        options.extend(extra_options);
        let target_request = TargetRequest {
            backend_api: flick_backend::VERSION.into(),
            ir_version: flick_ir::VERSION.into(),
            language_version: toolchain::COMPILER.into(),
            runtime_abis: vec![Abi {
                name: "flickrt".into(),
                version: toolchain::RUNTIME.into(),
            }],
            environment: Environment {
                backend: environment.backend.clone(),
                platform: environment.platform.clone(),
            },
            tags: environment.tags.iter().cloned().collect(),
            host_package: environment.host_package.clone(),
            profile: descriptor
                .profiles
                .first()
                .copied()
                .unwrap_or(Profile::Debug),
            mode: Mode::WholeProgram,
            required_capabilities: vec![],
            options,
        };
        let own: HashSet<&str> = compilation
            .modules
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        let builtins: Vec<&str> = types::builtin_sources()
            .iter()
            .map(|(name, _)| *name)
            .collect();
        let modules = program
            .modules
            .iter()
            .map(|module| ModuleInput {
                name: module.name.clone(),
                role: if own.contains(module.name.as_str()) {
                    ModuleRole::Own
                } else {
                    ModuleRole::Builtin
                },
                package: module.package.clone(),
                output: codegen::module_path(&builtins, false, &module.name),
                interface: None,
            })
            .collect();
        let mut data = Self {
            request: BuildRequest {
                target: target_request,
                backend: pin,
                program: program.to_cbor(),
                module_root: None,
                modules,
                sources: vec![],
                entries: vec![],
                test_roots: vec![],
                foreign: vec![],
                projections: vec![],
                package_targets: vec![],
                root_handlers: vec![],
                inputs: vec![],
                toolchains: vec![],
                runtime_dependencies: vec![],
                declared_dependencies: None,
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
        data
    }
}
