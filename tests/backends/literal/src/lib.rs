//! Test-only backend: a pure Unit main, with explicit unsupported diagnostics.
//! The same implementation is exercised natively and as a component.

mod commands;
mod launches;
mod packages;
mod steps;
mod tests;

use std::hint;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::SystemTime;

use flick_backend::command;
use flick_backend::{
    Artifact, ArtifactKind, Backend, BackendId, BuildRequest, BuildResult, Descriptor, Diagnostic,
    Environment, EnvironmentServices, Inputs, Mode, ModuleRole, NamedRange, OptionDescriptor,
    OptionValue, Packages, Parameter, Profile, ProjectFile, Severity, Span, TargetSupport,
    ValueKind, Verb, VersionRange,
};
use flick_ir::{ExprKind, ItemKind, Lit, Program, Stage};

#[derive(Default)]
pub struct Literal;

const READ_SOURCE: &str = "literal read case requires a source input";

fn diagnostic(message: &str, span: Option<Span>) -> Vec<Diagnostic> {
    vec![Diagnostic {
        code: "P012".into(),
        severity: Severity::Error,
        message: message.into(),
        span,
        notes: vec![],
    }]
}

fn case(request: &BuildRequest) -> &str {
    request
        .target
        .options
        .iter()
        .find(|o| o.key == "option.case")
        .or_else(|| {
            request
                .target
                .options
                .iter()
                .find(|o| o.key == "fixture.case")
        })
        .map_or_else(
            || {
                request
                    .modules
                    .iter()
                    .find_map(|m| match m.name.as_str() {
                        "Fixture.Warning" => Some("warning"),
                        "Fixture.Note" => Some("note"),
                        "Fixture.Refuse" => Some("refuse-warning"),
                        "Fixture.BadSpan" => Some("span"),
                        "Fixture.Ir" => Some("ir"),
                        "Fixture.CommonJs" => Some("commonjs"),
                        "Fixture.Steps" => Some("steps"),
                        "Fixture.StepsEscape" => Some("steps-escape"),
                        _ => None,
                    })
                    .unwrap_or("")
            },
            |o| &o.value,
        )
}

fn banner(request: &BuildRequest) -> Option<&str> {
    request
        .target
        .options
        .iter()
        .find(|o| o.key == "option.banner")
        .map(|o| o.value.as_str())
}

fn is_inspect(request: &BuildRequest) -> bool {
    request
        .target
        .options
        .iter()
        .any(|o| o.key == "command" && o.value == "inspect")
}

fn selected(request: &BuildRequest, module: &str) -> bool {
    request.target.mode != Mode::PerModule || request.module_root.as_deref() == Some(module)
}

impl Backend for Literal {
    fn package_files(&self) -> Vec<String> {
        packages::package_files()
    }

    fn packages(
        &self,
        files: &[ProjectFile],
        dependencies: &[command::Dependency],
        options: &[OptionValue],
    ) -> Packages {
        packages::packages(files, dependencies, options)
    }

    fn commands(&self) -> Vec<command::Descriptor> {
        commands::commands()
    }

    fn run(
        &self,
        invocation: &command::Invocation,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        if commands::commands()
            .iter()
            .any(|d| d.name == invocation.name)
        {
            commands::run(invocation)
        } else {
            Err(commands::unknown(&invocation.name))
        }
    }

    fn resume(
        &self,
        state: Vec<u8>,
        outputs: Vec<command::File>,
        answers: Vec<command::Answer>,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        let _ = answers;
        Ok(commands::resume(&state, &outputs))
    }

    fn open(
        &self,
        invocation: &command::Invocation,
    ) -> Result<command::Opened, Vec<command::Diagnostic>> {
        if invocation.name == "echo" {
            Ok(command::Opened::Session(vec![0]))
        } else {
            Err(commands::unknown(&invocation.name))
        }
    }

    fn handle(
        &self,
        state: Vec<u8>,
        request: Vec<u8>,
        answers: Vec<command::Answer>,
    ) -> Result<command::Handled, String> {
        let _ = answers;
        commands::handle(&state, &request)
    }

    fn close(&self, state: Vec<u8>) -> command::Plan {
        commands::close(&state)
    }

    fn templates(&self) -> Vec<command::TemplateDescriptor> {
        commands::templates()
    }

    fn scaffold(
        &self,
        request: &command::TemplateRequest,
    ) -> Result<command::Plan, Vec<command::Diagnostic>> {
        commands::scaffold(request)
    }

    fn describe(&self) -> Descriptor {
        let range = |a: &str, b: &str| VersionRange {
            minimum: a.into(),
            maximum_exclusive: b.into(),
        };
        Descriptor {
            identity: BackendId {
                id: "test/literal".into(),
                version: "1.0.0".into(),
            },
            backend_api: range("0.1.0", "0.2.0"),
            ir: range("0.13.0", "0.14.0"),
            language: range("0.1.0", "0.2.0"),
            runtime_abis: vec![NamedRange {
                name: "flickrt".into(),
                versions: range("0.1.0", "0.2.0"),
            }],
            environments: vec![Environment {
                backend: "js".into(),
                platform: "node".into(),
            }],
            profiles: vec![Profile::Debug, Profile::Release],
            modes: vec![Mode::PerModule, Mode::WholeProgram],
            capabilities: vec![],
            options: vec![
                OptionDescriptor {
                    parameter: Parameter {
                        name: "case".into(),
                        kind: ValueKind::Text,
                        multiple: false,
                        required: false,
                        help: "Select a test fixture behavior.".into(),
                    },
                    default: None,
                    implicit_value: None,
                    choices: vec![],
                    verbs: vec![Verb::Build, Verb::Run, Verb::Test],
                },
                OptionDescriptor {
                    parameter: Parameter {
                        name: "banner".into(),
                        kind: ValueKind::Text,
                        multiple: false,
                        required: false,
                        help: "Prepend a comment banner to generated JavaScript output.".into(),
                    },
                    default: None,
                    implicit_value: None,
                    choices: vec![],
                    verbs: vec![Verb::Build, Verb::Inspect],
                },
            ],
            launch_runtimes: vec!["node".into()],
            host_tools: vec!["node".into()],
            projections: vec![],
            unsupported_modules: vec![],
            stages_by_class_option: None,
            foreign_extension: None,
            foreign_suffixes: vec![],
            handled_services: vec![EnvironmentServices {
                environment: Environment {
                    backend: "js".into(),
                    platform: "node".into(),
                },
                services: vec!["Database".into(), "Db".into(), "Server".into()],
            }],
        }
    }

    fn check_target(
        &self,
        request: &BuildRequest,
        _: &mut dyn Inputs,
    ) -> Result<TargetSupport, Vec<Diagnostic>> {
        if case(request) == "refuse-warning" {
            let mut refusal = diagnostic("intentional warning-only refusal", None);
            refusal[0].severity = Severity::Warning;
            return Err(refusal);
        }
        if case(request) == "reads" && request.sources.is_empty() {
            return Err(diagnostic(READ_SOURCE, None));
        }
        let program =
            Program::from_cbor(&request.program).map_err(|e| diagnostic(&e.to_string(), None))?;
        for module in &program.modules {
            if !request
                .modules
                .iter()
                .any(|m| m.name == module.name && m.role == ModuleRole::Own)
            {
                continue;
            }
            if module.items.len() != 1 {
                return Err(diagnostic(
                    "literal backend needs exactly one Unit main",
                    None,
                ));
            }
            for item in &module.items {
                if tests::supported(item, request) {
                    continue;
                }
                let supported = matches!(&item.kind, ItemKind::Value { name, dicts, body }
                    if name == "main" && dicts.is_empty()
                    && matches!(&body.kind, ExprKind::Lambda { params, body }
                        if params.len() == 1 && params[0].stage == Stage::Pure
                            && matches!(body.kind, ExprKind::Lit(Lit::Unit))));
                if !supported && case(request) != "ir" {
                    let span = item.span;
                    return Err(diagnostic(
                        "literal backend supports only a pure Unit main",
                        Some(Span {
                            file: span.file.index(),
                            start: span.start,
                            end: span.end,
                        }),
                    ));
                }
            }
        }
        if request.entries.iter().any(|entry| entry.runner.is_some()) {
            return Err(diagnostic(
                "literal backend does not implement custom root runners",
                None,
            ));
        }
        Ok(TargetSupport {
            capabilities: self.describe().capabilities,
        })
    }

    fn generate(
        &self,
        request: &BuildRequest,
        inputs: &mut dyn Inputs,
    ) -> Result<BuildResult, Vec<Diagnostic>> {
        match case(request) {
            "trap" => panic!("intentional fixture trap"),
            "fuel" => loop {
                hint::black_box(());
            },
            "undeclared" => {
                let _ = inputs.read("not-declared");
            }
            "reads" => {
                let source = request
                    .sources
                    .first()
                    .ok_or_else(|| diagnostic(READ_SOURCE, None))?;
                for _ in 0..1000 {
                    let _ = inputs.read(&source.input);
                }
            }
            "clock" => {
                hint::black_box(SystemTime::now());
            }
            _ => {}
        }
        for source in &request.sources {
            inputs
                .read(&source.input)
                .map_err(|_| diagnostic("source input is unavailable", None))?;
        }
        let mut artifacts = module_artifacts(request);
        if is_inspect(request) {
            return Ok(BuildResult {
                steps: vec![],
                artifacts,
                runtime_dependencies: vec![],
                launches: vec![],
                diagnostics: vec![],
            });
        }
        artifacts.push(Artifact {
            id: "binary".into(),
            path: "data.bin".into(),
            kind: ArtifactKind::Resource,
            bytes: vec![0, 255, 128],
        });
        if case(request) == "ir" {
            artifacts.push(Artifact {
                id: "request-ir".into(),
                path: "request.ir.cbor".into(),
                kind: ArtifactKind::Metadata,
                bytes: request.program.clone(),
            });
        }
        if case(request) == "state" {
            static COUNT: AtomicU32 = AtomicU32::new(0);
            artifacts.push(Artifact {
                id: "state".into(),
                path: "state.bin".into(),
                kind: ArtifactKind::Resource,
                bytes: COUNT.fetch_add(1, Ordering::Relaxed).to_le_bytes().to_vec(),
            });
        }
        if case(request) == "path" {
            artifacts[0].path = "../outside.js".into();
        }
        if case(request) == "duplicate" {
            artifacts.push(artifacts[0].clone());
        }
        let diagnostics = if matches!(case(request), "warning" | "note") {
            let mut diagnostics = diagnostic("successful backend diagnostic", None);
            diagnostics[0].severity = if case(request) == "note" {
                Severity::Note
            } else {
                Severity::Warning
            };
            diagnostics
        } else if case(request) == "span" {
            vec![Diagnostic {
                code: "P012".into(),
                severity: Severity::Warning,
                message: "bad span".into(),
                span: Some(Span {
                    file: u32::MAX,
                    start: 0,
                    end: 1,
                }),
                notes: vec![],
            }]
        } else {
            vec![]
        };
        let mut launches = launches::run(request, &mut artifacts);
        let runtime_dependencies = tests::runner(request, &mut artifacts, &mut launches);
        Ok(BuildResult {
            steps: steps::steps(request, case(request), &artifacts),
            artifacts,
            runtime_dependencies,
            launches,
            diagnostics,
        })
    }
}

fn module_artifacts(request: &BuildRequest) -> Vec<Artifact> {
    let prefix = banner(request).map(|b| format!("// {b}\n").into_bytes());
    request
        .modules
        .iter()
        .filter(|m| m.role == ModuleRole::Own && selected(request, &m.name))
        .map(|m| {
            let mut bytes = prefix.clone().unwrap_or_default();
            bytes.extend(
                tests::source(request, &m.name)
                    .unwrap_or_else(|| b"export const main = (_) => undefined;\n".to_vec()),
            );
            Artifact {
                id: format!("module:{}", m.name),
                path: m.output.clone(),
                kind: ArtifactKind::Source,
                bytes,
            }
        })
        .collect()
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

    use super::Literal;

    struct Imports;
    impl Inputs for Imports {
        fn read(&mut self, id: &str) -> Result<Vec<u8>, InputError> {
            inputs::read(id)
        }
    }

    struct Component;
    impl Guest for Component {
        fn describe() -> Descriptor {
            Literal.describe()
        }
        fn check_target(request: BuildRequest) -> Result<TargetSupport, Vec<Diagnostic>> {
            Literal.check_target(&request, &mut Imports)
        }
        fn generate(request: BuildRequest) -> Result<BuildResult, Vec<Diagnostic>> {
            Literal.generate(&request, &mut Imports)
        }
        fn package_files() -> Vec<String> {
            Literal.package_files()
        }
        fn packages(
            files: Vec<ProjectFile>,
            dependencies: Vec<Dependency>,
            options: Vec<OptionValue>,
        ) -> Packages {
            Literal.packages(&files, &dependencies, &options)
        }
    }
    export_component!(Component);
    flick_backend::export_commands!(Component, Literal);
}
