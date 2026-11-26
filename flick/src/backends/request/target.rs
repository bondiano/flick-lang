use std::env;

use flick_backend::{Abi, Descriptor, Environment, Mode, OptionValue, Profile, TargetRequest};
use flick_backend_support::kernel;
use flick_core::project::{Compilation, Project};

use crate::commands::build::{BuildPlan, Purpose};
use crate::toolchain;

pub(super) fn target(
    project: Option<&Project>,
    compilation: &Compilation,
    output: &BuildPlan,
    descriptor: &Descriptor,
) -> (TargetRequest, Option<String>) {
    let default_env = || {
        descriptor
            .environments
            .first()
            .cloned()
            .unwrap_or_else(|| Environment {
                backend: output.target.clone(),
                platform: "default".into(),
            })
    };
    let (environment, tags, host_package) = if let Some(resolved) = &compilation.environment {
        (
            Environment {
                backend: resolved.backend.clone(),
                platform: resolved.platform.clone(),
            },
            resolved.tags.iter().cloned().collect(),
            resolved.host_package.clone(),
        )
    } else {
        let env = project
            .and_then(|p| p.config.build.environment.as_ref())
            .filter(|e| !output.target_overridden || e.backend == output.target)
            .map_or_else(default_env, |e| Environment {
                backend: e.backend.clone(),
                platform: e.platform.clone(),
            });
        (env, vec![], None)
    };
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
    let mut options = vec![
        command,
        option("host.os", env::consts::OS),
        option("labels", &format!("{:?}", extras.labels).to_lowercase()),
    ];
    options.extend(output.options.to_wire());

    let (mode, module_root) = if output.purpose == Purpose::Inspect {
        let supports_per_module = descriptor.modes.contains(&Mode::PerModule);
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
            runtime_abis: runtime_abis(toolchain::RUNTIME),
            environment,
            tags,
            host_package,
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

/// The runtime ABIs a request names: Flick's runtime at `runtime`, and the
/// primitive kernel.
pub(in crate::backends) fn runtime_abis(runtime: &str) -> Vec<Abi> {
    vec![
        Abi {
            name: "flickrt".into(),
            version: runtime.into(),
        },
        Abi {
            name: kernel::ABI.into(),
            version: kernel::VERSION.into(),
        },
    ]
}
