#![allow(dead_code)]

use std::collections::BTreeMap;

use flick_backend::*;
use flick_ir::{
    Exports, Expr, ExprKind, FileId, Initialization, Item, ItemKind, Lit, Module, Param, Pat,
    Program, Stage,
};

pub const TEXT: &str = "let main () = ()\n";

pub fn request() -> BuildRequest {
    let span = flick_ir::Span::new(FileId::new(0), 0, 16);
    let module = Module {
        name: "App".into(),
        items: vec![Item {
            kind: ItemKind::Value {
                name: "main".into(),
                dicts: vec![],
                body: Expr::new(
                    ExprKind::Lambda {
                        params: vec![Param {
                            pat: Pat::Wild,
                            stage: Stage::Pure,
                            ty: None,
                        }],
                        body: Box::new(Expr::new(ExprKind::Lit(Lit::Unit), span)),
                    },
                    span,
                ),
            },
            span,
        }],
        labels: BTreeMap::new(),
        initialization: Initialization::default(),
        records: BTreeMap::new(),
        package: Some("demo".into()),
        exports: Exports::default(),
    };
    let program = Program::new("flick backend test", vec![module], vec![], &|_| {
        "App.fl".into()
    });
    BuildRequest {
        target: TargetRequest {
            backend_api: VERSION.into(),
            ir_version: flick_ir::VERSION.into(),
            language_version: "0.1.0".into(),
            runtime_abis: vec![Abi {
                name: "flickrt".into(),
                version: "0.1.0".into(),
            }],
            environment: Environment {
                backend: "js".into(),
                platform: "node".into(),
            },
            profile: Profile::Debug,
            mode: Mode::WholeProgram,
            required_capabilities: vec![],
            options: vec![],
        },
        backend: BackendPin {
            identity: BackendId {
                id: "test/js".into(),
                version: "1.2.3".into(),
            },
            sha256: sha256(b"test component"),
        },
        program: program.to_cbor(),
        module_root: None,
        modules: vec![ModuleInput {
            name: "App".into(),
            role: ModuleRole::Own,
            package: Some("demo".into()),
            output: "App.js".into(),
            interface: None,
        }],
        sources: vec![Source {
            name: "App.fl".into(),
            input: "source/App".into(),
            line_starts: vec![0, 17],
        }],
        entries: vec![Entry {
            name: "main".into(),
            symbol: Symbol {
                module: "App".into(),
                name: "main".into(),
            },
            runner: None,
        }],
        test_roots: vec![],
        foreign: vec![],
        projections: vec![],
        package_targets: vec![],
        root_handlers: vec![],
        inputs: vec![Input {
            id: "source/App".into(),
            kind: InputKind::Source,
            sha256: sha256(TEXT.as_bytes()),
            size: TEXT.len() as u64,
        }],
        toolchains: vec![toolchain()],
        runtime_dependencies: vec![],
    }
}

pub fn descriptor() -> Descriptor {
    let range = |a: &str, b: &str| VersionRange {
        minimum: a.into(),
        maximum_exclusive: b.into(),
    };
    Descriptor {
        identity: request().backend.identity,
        backend_api: range("0.1.0", "0.2.0"),
        ir: range("0.11.0", "0.12.0"),
        language: range("0.1.0", "0.2.0"),
        runtime_abis: vec![NamedRange {
            name: "flickrt".into(),
            versions: range("0.1.0", "0.2.0"),
        }],
        environments: vec![request().target.environment],
        profiles: vec![Profile::Debug, Profile::Release],
        modes: vec![Mode::PerModule, Mode::WholeProgram],
        capabilities: vec![],
    }
}

pub fn result() -> BuildResult {
    BuildResult {
        artifacts: vec![Artifact {
            id: "object".into(),
            path: "obj/app.o".into(),
            kind: ArtifactKind::Object,
            bytes: vec![0, 255, 128, 10],
        }],
        steps: vec![],
        runtime_dependencies: vec![],
        launches: vec![],
        diagnostics: vec![],
    }
}

pub fn toolchain() -> Toolchain {
    Toolchain {
        id: "go".into(),
        version: "1.23.0".into(),
        sha256: sha256(b"host-pinned-go"),
    }
}

pub fn step(name: &str, dependencies: &[&str]) -> ToolchainStep {
    ToolchainStep {
        id: name.into(),
        dependencies: dependencies.iter().map(|s| (*s).into()).collect(),
        toolchain: toolchain(),
        arguments: vec!["build".into()],
        environment: vec![],
        inputs: vec![InputRef::Artifact("object".into())],
        outputs: vec![StepOutput {
            id: format!("{name}-output"),
            path: format!("bin/{name}"),
            kind: ArtifactKind::Executable,
        }],
    }
}

pub fn inputs(request: &BuildRequest) -> DeclaredInputs {
    DeclaredInputs::new(&request.inputs, |_| Some(TEXT.as_bytes().to_vec()))
        .expect("declared inputs")
}

pub fn validate(request: &BuildRequest, result: &BuildResult) -> Result<Vec<usize>, String> {
    let policy = Policy {
        toolchains: &request.toolchains,
        environment: &[],
        reserved_paths: &[],
        limits: Limits::default(),
        authorize_step: &authorize,
    };
    validate_result(request, result, &policy, &mut inputs(request))
}

pub fn diagnostic() -> Diagnostic {
    Diagnostic {
        code: "B001".into(),
        severity: Severity::Warning,
        message: "unsupported operation".into(),
        span: Some(Span {
            file: 0,
            start: 0,
            end: 3,
        }),
        notes: vec![],
    }
}

pub fn authorize(step: &ToolchainStep) -> Result<(), String> {
    if step.arguments != ["build"] {
        return Err("host tool adapter rejected the command arguments".into());
    }
    Ok(())
}
