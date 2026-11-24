//! Test-only backend: a pure Unit main, with explicit unsupported diagnostics.
//! The same implementation is exercised natively and as a component.

mod commands;
mod fallback;
mod foreign;
mod kernel;
mod launches;
mod options;
mod packages;
mod steps;
mod tests;
mod typed_ir;

use std::hint;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::SystemTime;

use flick_backend::command;
use flick_backend::{
    Artifact, ArtifactKind, Backend, BackendId, BuildRequest, BuildResult, Descriptor, Diagnostic,
    Environment, EnvironmentServices, Inputs, Mode, ModuleRole, NamedRange, OptionValue, Packages,
    Profile, ProjectFile, Severity, Span, TagAxis, TargetSupport, VersionRange,
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
                        "Fixture.TypedIr" => Some("typed-ir"),
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

fn is_check(request: &BuildRequest) -> bool {
    request
        .target
        .options
        .iter()
        .any(|o| o.key == "command" && o.value == "check")
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
            ir: range("0.18.0", "0.19.0"),
            language: range("0.2.0", "0.3.0"),
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
            environments: vec![Environment {
                backend: "js".into(),
                platform: "node".into(),
            }],
            tags: vec!["literal".into()],
            axes: vec![
                TagAxis {
                    name: "dialect".into(),
                    tags: vec!["alpha".into(), "beta".into()],
                    default: Some("alpha".into()),
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
            modes: vec![Mode::PerModule, Mode::WholeProgram],
            capabilities: vec![],
            options: options::literal_options(),
            launch_runtimes: vec!["node".into()],
            host_tools: vec!["node".into()],
            projections: vec![],
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
            foreign_adapters: foreign::foreign_adapters(),
            type_form_attributes: foreign::type_form_attributes(),
            value_abi: Some(foreign::value_abi()),
        }
    }

    fn check_target(
        &self,
        request: &BuildRequest,
        inputs: &mut dyn Inputs,
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
        foreign::validate_foreigns(&program, request, inputs)?;
        if is_check(request) {
            return Ok(TargetSupport {
                capabilities: self.describe().capabilities,
            });
        }
        if case(request) == "typed-ir" {
            typed_ir::validate_program(&program, request)?;
            return Ok(TargetSupport {
                capabilities: self.describe().capabilities,
            });
        }
        if case(request) == "fallback" {
            fallback::check_target(&program, request)?;
            return Ok(TargetSupport {
                capabilities: self.describe().capabilities,
            });
        }
        if case(request) == "kernel" {
            kernel::check_target(&program, &kernel::roots(request))?;
            return Ok(TargetSupport {
                capabilities: self.describe().capabilities,
            });
        }
        if request.target.host_package.is_none() && !matches!(case(request), "env" | "foreign") {
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
                    let supported = matches!(&item.kind, ItemKind::Value { name, dicts, body, .. }
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
        foreign::validate_foreign_abis(request)?;
        let mut artifacts = module_artifacts(request)?;
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
        apply_case_artifacts(request, &mut artifacts)?;
        let diagnostics = case_diagnostics(request);
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

fn apply_case_artifacts(
    request: &BuildRequest,
    artifacts: &mut Vec<Artifact>,
) -> Result<(), Vec<Diagnostic>> {
    match case(request) {
        "typed-ir" => {
            let program = Program::from_cbor(&request.program)
                .map_err(|e| diagnostic(&e.to_string(), None))?;
            artifacts.push(typed_ir::report_artifact(program, request)?);
        }
        "ir" => artifacts.push(Artifact {
            id: "request-ir".into(),
            path: "request.ir.cbor".into(),
            kind: ArtifactKind::Metadata,
            bytes: request.program.clone(),
        }),
        "state" => {
            static COUNT: AtomicU32 = AtomicU32::new(0);
            artifacts.push(Artifact {
                id: "state".into(),
                path: "state.bin".into(),
                kind: ArtifactKind::Resource,
                bytes: COUNT.fetch_add(1, Ordering::Relaxed).to_le_bytes().to_vec(),
            });
        }
        "path" => artifacts[0].path = "../outside.js".into(),
        "duplicate" => artifacts.push(artifacts[0].clone()),
        "foreign" => foreign::reports(request, artifacts)?,
        _ => {}
    }
    Ok(())
}

fn case_diagnostics(request: &BuildRequest) -> Vec<Diagnostic> {
    if matches!(case(request), "warning" | "note") {
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
    }
}

fn module_artifacts(request: &BuildRequest) -> Result<Vec<Artifact>, Vec<Diagnostic>> {
    let prefix = banner(request).map(|b| format!("// {b}\n").into_bytes());
    let env_header = (request.target.host_package.is_some() || case(request) == "env").then(|| {
        format!(
            "// env: {}/{} tags: {} host: {}\n",
            request.target.environment.backend,
            request.target.environment.platform,
            request.target.tags.join(","),
            request.target.host_package.as_deref().unwrap_or("none"),
        )
        .into_bytes()
    });
    let kernel = if case(request) == "kernel" {
        let program =
            Program::from_cbor(&request.program).map_err(|e| diagnostic(&e.to_string(), None))?;
        let profile = format!("{:?}", request.target.profile).to_lowercase();
        Some(kernel::generate(
            &program,
            &kernel::roots(request),
            &profile,
        )?)
    } else {
        None
    };
    let mut out = Vec::new();
    for m in request
        .modules
        .iter()
        .filter(|m| m.role == ModuleRole::Own && selected(request, &m.name))
    {
        let mut bytes = prefix.clone().unwrap_or_default();
        if let Some(header) = &env_header {
            bytes.extend(header);
        }
        let suite = (kernel.iter())
            .flat_map(|(sources, _)| sources)
            .find(|(module, _)| *module == m.name);
        if let Some((_, source)) = suite {
            bytes.extend(source);
        } else if case(request) == "fallback" {
            bytes.extend(fallback::source(request, &m.name)?);
        } else {
            bytes.extend(
                tests::source(request, &m.name)
                    .unwrap_or_else(|| b"export const main = (_) => undefined;\n".to_vec()),
            );
        }
        out.push(Artifact {
            id: format!("module:{}", m.name),
            path: m.output.clone(),
            kind: ArtifactKind::Source,
            bytes,
        });
    }
    out.extend(kernel.map(|(_, report)| report));
    Ok(out)
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
