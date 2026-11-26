//! Mode root and module layout ownership tests across native and component backends.

use std::cell::Cell;

use super::{common, fixture};
use flick_backend::host::Budget;
use flick_backend::{
    Artifact, ArtifactKind, Backend, BuildRequest, BuildResult, Descriptor, Diagnostic, Failure,
    Generated, Input, InputKind, Inputs, Limits, Mode, ModuleInput, ModuleRole, Policy,
    TargetSupport, generate, sha256,
};
use flick_ir::Program;
use flick_test_backend::Literal;

fn multi_module_request(mut request: BuildRequest) -> BuildRequest {
    let dep_iface = b"dep-iface-v1";
    let mut program = Program::from_cbor(&request.program).expect("IR");
    let mut other = program.modules[0].clone();
    other.name = "Other".into();
    program.modules.push(other);
    let mut builtin = program.modules[0].clone();
    builtin.name = "BuiltinMod".into();
    builtin.package = None;
    program.modules.push(builtin);
    request.program = program.to_cbor();
    request.modules.push(ModuleInput {
        name: "Other".into(),
        role: ModuleRole::Own,
        package: Some("demo".into()),
        output: "Other.js".into(),
        interface: None,
    });
    request.modules.push(ModuleInput {
        name: "BuiltinMod".into(),
        role: ModuleRole::Builtin,
        package: None,
        output: "BuiltinMod.js".into(),
        interface: None,
    });
    request.modules.push(ModuleInput {
        name: "Dep".into(),
        role: ModuleRole::External,
        package: Some("dep".into()),
        output: "Dep.js".into(),
        interface: Some("iface/Dep".into()),
    });
    request.inputs.push(Input {
        id: "iface/Dep".into(),
        kind: InputKind::Interface,
        sha256: sha256(dep_iface),
        size: dep_iface.len() as u64,
    });
    request
}

fn compile_multi(backend: &dyn Backend, request: &BuildRequest) -> Result<Generated, Failure> {
    let policy = Policy {
        toolchains: &request.toolchains,
        environment: &[],
        reserved_paths: &[],
        authorize_step: &common::authorize,
        authorize_launch: &common::authorize_launch,
        limits: Limits::default(),
    };
    generate(
        backend,
        request,
        &policy,
        |id| {
            Some(if id == "iface/Dep" {
                b"dep-iface-v1".to_vec()
            } else {
                common::TEXT.as_bytes().to_vec()
            })
        },
        true,
    )
}

struct ModeRestricted {
    allowed: Mode,
    calls: Cell<usize>,
}

impl Backend for ModeRestricted {
    fn describe(&self) -> Descriptor {
        let mut d = Literal.describe();
        d.modes = vec![self.allowed];
        d
    }
    fn check_target(
        &self,
        _: &BuildRequest,
        _: &mut dyn Inputs,
    ) -> Result<TargetSupport, Vec<Diagnostic>> {
        self.calls.set(self.calls.get() + 1);
        Ok(TargetSupport {
            capabilities: vec![],
        })
    }
    fn generate(
        &self,
        _: &BuildRequest,
        _: &mut dyn Inputs,
    ) -> Result<BuildResult, Vec<Diagnostic>> {
        self.calls.set(self.calls.get() + 1);
        Ok(common::result())
    }
}

struct LayoutViolator {
    stolen_path: &'static str,
}

impl Backend for LayoutViolator {
    fn describe(&self) -> Descriptor {
        Literal.describe()
    }
    fn check_target(
        &self,
        _: &BuildRequest,
        _: &mut dyn Inputs,
    ) -> Result<TargetSupport, Vec<Diagnostic>> {
        Ok(TargetSupport {
            capabilities: vec![],
        })
    }
    fn generate(
        &self,
        _: &BuildRequest,
        _: &mut dyn Inputs,
    ) -> Result<BuildResult, Vec<Diagnostic>> {
        Ok(BuildResult {
            artifacts: vec![
                Artifact {
                    id: "module:App".into(),
                    path: "App.js".into(),
                    kind: ArtifactKind::Source,
                    bytes: b"export const main = () => undefined;\n".to_vec(),
                },
                Artifact {
                    id: "stolen".into(),
                    path: self.stolen_path.into(),
                    kind: ArtifactKind::Source,
                    bytes: b"// unowned\n".to_vec(),
                },
            ],
            steps: vec![],
            runtime_dependencies: vec![],
            launches: vec![],
            diagnostics: vec![],
        })
    }
}

#[test]
fn native_driver_enforces_mode_roots_and_layout_ownership_before_publication() {
    let mut base = multi_module_request(common::request());
    base.backend.identity = Literal.describe().identity;

    let whole = compile_multi(&Literal, &base).expect("whole-program generation");
    let whole_paths: Vec<&str> = whole
        .result()
        .artifacts
        .iter()
        .map(|a| a.path.as_str())
        .collect();
    assert!(whole_paths.contains(&"App.js"), "{whole_paths:?}");
    assert!(whole_paths.contains(&"Other.js"), "{whole_paths:?}");
    assert!(!whole_paths.contains(&"Dep.js"), "{whole_paths:?}");

    let mut per_module = base.clone();
    per_module.target.mode = Mode::PerModule;
    per_module.module_root = Some("App".into());
    let single = compile_multi(&Literal, &per_module).expect("per-module generation");
    let single_paths: Vec<&str> = single
        .result()
        .artifacts
        .iter()
        .map(|a| a.path.as_str())
        .collect();
    assert!(single_paths.contains(&"App.js"), "{single_paths:?}");
    assert!(!single_paths.contains(&"Other.js"), "{single_paths:?}");
    assert!(!single_paths.contains(&"Dep.js"), "{single_paths:?}");

    let mut whole_with_root = base.clone();
    whole_with_root.module_root = Some("App".into());
    assert!(matches!(
        compile_multi(&Literal, &whole_with_root),
        Err(Failure::Protocol(_))
    ));
    for bad_root in [
        None,
        Some("Unknown".into()),
        Some("BuiltinMod".into()),
        Some("Dep".into()),
    ] {
        let mut bad = per_module.clone();
        bad.module_root = bad_root.clone();
        assert!(
            matches!(compile_multi(&Literal, &bad), Err(Failure::Protocol(_))),
            "expected protocol refusal for per-module root {bad_root:?}"
        );
    }

    for (allowed, req) in [(Mode::WholeProgram, &per_module), (Mode::PerModule, &base)] {
        let restricted = ModeRestricted {
            allowed,
            calls: Cell::new(0),
        };
        assert!(matches!(
            compile_multi(&restricted, req),
            Err(Failure::Protocol(msg)) if msg.contains("compilation mode")
        ));
        assert_eq!(restricted.calls.get(), 0);
    }

    for (req, stolen) in [
        (&per_module, "Other.js"),
        (&per_module, "Dep.js"),
        (&base, "Dep.js"),
    ] {
        let violator = LayoutViolator {
            stolen_path: stolen,
        };
        assert!(
            matches!(
                compile_multi(&violator, req),
                Err(Failure::Protocol(msg)) if msg.contains("layout ownership")
            ),
            "expected layout ownership refusal for {stolen}"
        );
    }
}

#[test]
#[ignore = "build the real component fixture with `just wasm-backends`"]
fn component_and_native_match_across_multi_module_modes_and_reject_invalid_roots() {
    let (backend, base_request) = fixture(Budget::default());
    let base = multi_module_request(base_request);

    let native_whole = compile_multi(&Literal, &base).expect("native whole");
    let wasm_whole = compile_multi(&backend, &base).expect("wasm whole");
    assert_eq!(wasm_whole.result(), native_whole.result());
    let whole_paths: Vec<&str> = wasm_whole
        .result()
        .artifacts
        .iter()
        .map(|a| a.path.as_str())
        .collect();
    assert!(whole_paths.contains(&"App.js"), "{whole_paths:?}");
    assert!(whole_paths.contains(&"Other.js"), "{whole_paths:?}");
    assert!(!whole_paths.contains(&"Dep.js"), "{whole_paths:?}");

    let mut per_module = base.clone();
    per_module.target.mode = Mode::PerModule;
    per_module.module_root = Some("App".into());
    let native_per = compile_multi(&Literal, &per_module).expect("native per-module");
    let wasm_per = compile_multi(&backend, &per_module).expect("wasm per-module");
    assert_eq!(wasm_per.result(), native_per.result());
    let per_paths: Vec<&str> = wasm_per
        .result()
        .artifacts
        .iter()
        .map(|a| a.path.as_str())
        .collect();
    assert!(per_paths.contains(&"App.js"), "{per_paths:?}");
    assert!(!per_paths.contains(&"Other.js"), "{per_paths:?}");
    assert!(!per_paths.contains(&"Dep.js"), "{per_paths:?}");

    let mut whole_with_root = base;
    whole_with_root.module_root = Some("App".into());
    assert!(matches!(
        compile_multi(&backend, &whole_with_root),
        Err(Failure::Protocol(_))
    ));
    for bad_root in [
        None,
        Some("Unknown".into()),
        Some("BuiltinMod".into()),
        Some("Dep".into()),
    ] {
        let mut bad = per_module.clone();
        bad.module_root = bad_root;
        assert!(matches!(
            compile_multi(&backend, &bad),
            Err(Failure::Protocol(_))
        ));
    }
}
