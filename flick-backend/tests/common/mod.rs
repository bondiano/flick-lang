#![allow(dead_code)]

use std::collections::BTreeMap;

use flick_backend::*;
use flick_ir::{
    Exports, Expr, ExprKind, FileId, Initialization, Item, ItemKind, Lit, Module, Param, Pat,
    Program, Stage,
};

pub const TEXT: &str = "let main () = ()\n";

#[allow(clippy::too_many_lines)]
pub fn request() -> BuildRequest {
    let span = flick_ir::Span::new(FileId::new(0), 0, 16);
    let module = Module {
        name: "App".into(),
        types: vec![],
        preambles: vec![],
        items: vec![Item {
            kind: ItemKind::Value {
                name: "main".into(),
                type_params: vec![],
                dicts: vec![],
                dict_constraints: vec![],
                ty: None,
                body: Expr::new(
                    ExprKind::Lambda {
                        params: vec![Param {
                            pat: Pat::Wild,
                            stage: Stage::Pure,
                            ty: None,
                            binder_types: vec![],
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
            language_version: "0.2.0".into(),
            runtime_abis: vec![
                Abi {
                    name: "flickrt".into(),
                    version: "0.2.0".into(),
                },
                Abi {
                    name: "flick.kernel".into(),
                    version: "0.1.0".into(),
                },
            ],
            environment: Environment {
                backend: "js".into(),
                platform: "node".into(),
            },
            tags: vec![],
            host_package: None,
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
        inline_foreign: vec![],
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
        declared_dependencies: None,
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
        environments: vec![request().target.environment],
        tags: vec!["js".into()],
        axes: vec![],
        profiles: vec![Profile::Debug, Profile::Release],
        modes: vec![Mode::PerModule, Mode::WholeProgram],
        capabilities: vec![],
        options: vec![],
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
        authorize_launch: &authorize_launch,
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

pub fn authorize_launch(launch: &Launch) -> Result<(), String> {
    for env in &launch.environment {
        if env.key != "FLICK_DEVTOOLS" {
            return Err(format!(
                "host launch policy rejected environment `{}`",
                env.key
            ));
        }
    }
    for arg in &launch.runtime_arguments {
        if arg == "-e" || arg == "--eval" {
            return Err(format!("host launch policy rejected runtime arg `{arg}`"));
        }
    }
    Ok(())
}
