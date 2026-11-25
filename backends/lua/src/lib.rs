//! The Lua backend: the host profiles of `profiles/` and a `check-target`
//! that refuses what a selected profile cannot run, at the source of the
//! reachable use. It generates no code yet, and says so.

mod check;
// `generate` lowers through it once it assembles the chunks of a program
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the emitter is exercised by its tests until `generate` uses it"
    )
)]
mod emit;
mod profiles;
mod reach;

use flick_backend::{
    Backend, BackendId, BuildRequest, BuildResult, CHECK_TARGET_CAPABILITY, Capability, Descriptor,
    Diagnostic, Environment, Inputs, Mode, NamedRange, OptionDescriptor, Parameter, Profile,
    Severity, Span, TagAxis, TargetSupport, ValueKind, Verb, VersionRange,
};

use profiles::PROFILES;

#[derive(Default)]
pub struct Lua;

/// The diagnostic of the backend: `P012`, what a target cannot do.
fn refusal(message: String, span: Option<Span>, notes: Vec<String>) -> Diagnostic {
    Diagnostic {
        code: "P012".into(),
        severity: Severity::Error,
        message,
        span,
        notes,
    }
}

fn option(
    name: &str,
    kind: ValueKind,
    default: &str,
    choices: &[&str],
    help: &str,
) -> OptionDescriptor {
    OptionDescriptor {
        parameter: Parameter {
            name: name.into(),
            kind,
            multiple: false,
            required: false,
            help: help.into(),
        },
        default: Some(default.into()),
        implicit_value: None,
        choices: choices.iter().map(|&c| c.into()).collect(),
        verbs: vec![Verb::Build, Verb::Run, Verb::Dev, Verb::Test, Verb::Inspect],
    }
}

impl Backend for Lua {
    fn describe(&self) -> Descriptor {
        let range = |minimum: &str, maximum: &str| VersionRange {
            minimum: minimum.into(),
            maximum_exclusive: maximum.into(),
        };
        Descriptor {
            identity: BackendId {
                id: "flick/lua".into(),
                version: "0.1.0".into(),
            },
            backend_api: range("0.1.0", "0.2.0"),
            ir: range("0.19.0", "0.20.0"),
            language: range("0.2.0", "0.3.0"),
            // every request names the runtime ABI of the compiler and the
            // primitive kernel
            runtime_abis: vec![
                NamedRange {
                    name: "flickrt".into(),
                    versions: range("0.2.0", "0.3.0"),
                },
                NamedRange {
                    name: "flick.kernel".into(),
                    versions: range("0.1.0", "0.2.0"),
                },
            ],
            environments: PROFILES
                .iter()
                .map(|p| Environment {
                    backend: "lua".into(),
                    platform: p.platform.into(),
                })
                .collect(),
            tags: vec!["lua".into()],
            axes: vec![
                TagAxis {
                    name: "dialect".into(),
                    tags: vec!["lua51".into(), "luajit".into()],
                    default: Some("lua51".into()),
                    exclusive: true,
                },
                TagAxis {
                    name: "capabilities".into(),
                    tags: vec!["jit".into(), "ffi".into()],
                    default: None,
                    exclusive: false,
                },
            ],
            profiles: vec![Profile::Debug, Profile::Release],
            modes: vec![Mode::WholeProgram],
            // checks every program, not only one with foreign items
            capabilities: vec![Capability {
                name: CHECK_TARGET_CAPABILITY.into(),
                version: "0.1.0".into(),
            }],
            options: vec![
                option(
                    "dialect",
                    ValueKind::Text,
                    "lua51",
                    &["lua51", "luajit"],
                    "The Lua dialect: portable Lua 5.1 (`lua51`) or LuaJIT 2.1 (`luajit`).",
                ),
                option(
                    "jit",
                    ValueKind::Flag,
                    "false",
                    &[],
                    "Allow the LuaJIT `jit` library (needs `dialect = \"luajit\"`).",
                ),
                option(
                    "ffi",
                    ValueKind::Flag,
                    "false",
                    &[],
                    "Allow the LuaJIT FFI, which keeps Int64 and UInt64 exact (needs `dialect = \"luajit\"`).",
                ),
            ],
            launch_runtimes: vec![],
            host_tools: vec![],
            projections: vec![],
            stages_by_class_option: None,
            foreign_extension: None,
            foreign_suffixes: vec![],
            handled_services: vec![],
            foreign_adapters: vec![],
            type_form_attributes: vec![],
            value_abi: None,
        }
    }

    fn check_target(
        &self,
        request: &BuildRequest,
        _inputs: &mut dyn Inputs,
    ) -> Result<TargetSupport, Vec<Diagnostic>> {
        check::check(request)?;
        Ok(TargetSupport {
            capabilities: vec![],
        })
    }

    fn generate(
        &self,
        request: &BuildRequest,
        inputs: &mut dyn Inputs,
    ) -> Result<BuildResult, Vec<Diagnostic>> {
        self.check_target(request, inputs)?;
        Err(vec![refusal(
            "code generation is not implemented yet on the lua target: `flick check` checks a \
             program against its Lua profile, and nothing is written"
                .into(),
            None,
            vec![],
        )])
    }
}

#[cfg(feature = "guest")]
mod guest {
    use flick_backend::bindings::flick::backend::inputs;
    use flick_backend::bindings::{Guest, export_component};
    use flick_backend::command::Dependency;
    use flick_backend::{
        Backend, BuildRequest, BuildResult, Descriptor, Diagnostic, InputError, Inputs,
        OptionValue, Packages, ProjectFile, TargetSupport,
    };

    use super::Lua;

    struct Imports;
    impl Inputs for Imports {
        fn read(&mut self, id: &str) -> Result<Vec<u8>, InputError> {
            inputs::read(id)
        }
    }

    struct Component;
    impl Guest for Component {
        fn describe() -> Descriptor {
            Lua.describe()
        }
        fn check_target(request: BuildRequest) -> Result<TargetSupport, Vec<Diagnostic>> {
            Lua.check_target(&request, &mut Imports)
        }
        fn generate(request: BuildRequest) -> Result<BuildResult, Vec<Diagnostic>> {
            Lua.generate(&request, &mut Imports)
        }
        fn package_files() -> Vec<String> {
            Lua.package_files()
        }
        fn packages(
            files: Vec<ProjectFile>,
            dependencies: Vec<Dependency>,
            options: Vec<OptionValue>,
        ) -> Packages {
            Lua.packages(&files, &dependencies, &options)
        }
    }
    export_component!(Component);
    flick_backend::export_commands!(Component, Lua);
}

#[cfg(test)]
mod tests;
