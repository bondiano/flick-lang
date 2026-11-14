//! Test-only backend: a pure Unit main, with explicit unsupported diagnostics.
//! The same implementation is exercised natively and as a component.

use std::hint;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::SystemTime;

use flick_backend::{
    Artifact, ArtifactKind, Backend, BackendId, BuildRequest, BuildResult, Descriptor, Diagnostic,
    Environment, Inputs, Mode, ModuleRole, NamedRange, Profile, Severity, Span, TargetSupport,
    VersionRange,
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
        .find(|o| o.key == "fixture.case")
        .map_or("", |o| &o.value)
}

impl Backend for Literal {
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
            ir: range("0.11.0", "0.12.0"),
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
        }
    }

    fn check_target(
        &self,
        request: &BuildRequest,
        _: &mut dyn Inputs,
    ) -> Result<TargetSupport, Vec<Diagnostic>> {
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
                let supported = matches!(&item.kind, ItemKind::Value { name, dicts, body }
                    if name == "main" && dicts.is_empty()
                    && matches!(&body.kind, ExprKind::Lambda { params, body }
                        if params.len() == 1 && params[0].stage == Stage::Pure
                            && matches!(body.kind, ExprKind::Lit(Lit::Unit))));
                if !supported {
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
        Ok(TargetSupport {
            capabilities: vec![],
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
        let mut artifacts: Vec<_> = request
            .modules
            .iter()
            .filter(|m| m.role == ModuleRole::Own)
            .map(|m| Artifact {
                id: format!("module:{}", m.name),
                path: m.output.clone(),
                kind: ArtifactKind::Source,
                bytes: b"export const main = (_) => undefined;\n".to_vec(),
            })
            .collect();
        artifacts.push(Artifact {
            id: "binary".into(),
            path: "data.bin".into(),
            kind: ArtifactKind::Resource,
            bytes: vec![0, 255, 128],
        });
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
        let diagnostics = if case(request) == "span" {
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
        Ok(BuildResult {
            artifacts,
            steps: vec![],
            runtime_dependencies: vec![],
            launches: vec![],
            diagnostics,
        })
    }
}

#[cfg(feature = "guest")]
mod guest {
    use flick_backend::bindings::flick::backend::inputs;
    use flick_backend::bindings::{Guest, export_component};
    use flick_backend::{
        Backend, BuildRequest, BuildResult, Descriptor, Diagnostic, InputError, Inputs,
        TargetSupport,
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
    }
    export_component!(Component);
}
