mod common;

use std::cell::Cell;

use flick_backend::*;

use common::{descriptor, diagnostic, inputs, request, result, validate};

struct Fallback {
    calls: Cell<usize>,
}

impl Backend for Fallback {
    fn describe(&self) -> Descriptor {
        descriptor()
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
        self.calls.set(self.calls.get() + 1);
        Ok(result())
    }
}

#[test]
fn unavailable_declared_inputs_cannot_make_a_cacheable_fallback() {
    let backend = Fallback {
        calls: Cell::new(0),
    };
    let mut request = request();
    request.inputs.push(Input {
        id: "foreign/optional".into(),
        kind: InputKind::Foreign,
        sha256: sha256(b"payload"),
        size: 7,
    });
    let policy = Policy {
        toolchains: &[],
        environment: &[],
        reserved_paths: &[],
        authorize_step: &common::authorize,
        limits: Limits::default(),
    };
    let generated = generate(
        &backend,
        &request,
        &policy,
        |id| (id == "source/App").then(|| common::TEXT.as_bytes().to_vec()),
        true,
    );
    let error = generated
        .err()
        .expect("missing content must reject the request");
    assert!(matches!(&error, Failure::Protocol(message) if message.contains("foreign/optional")));
    assert_eq!(backend.calls.get(), 0);

    let generated = generate(
        &backend,
        &request,
        &policy,
        |id| {
            Some(if id == "source/App" {
                common::TEXT.as_bytes().to_vec()
            } else {
                b"payload".to_vec()
            })
        },
        true,
    )
    .expect("all declared bytes are available");
    assert!(generated.cache_key().is_some());
    assert_eq!(backend.calls.get(), 2);
}

#[test]
fn runtime_pins_are_validated_even_when_request_and_result_agree() {
    for digest in [
        String::new(),
        "a".repeat(63),
        "g".repeat(64),
        "a".repeat(65),
    ] {
        let mut request = request();
        let mut result = result();
        let pin = RuntimeDependency {
            package: "kernel".into(),
            version: "0.1.0".into(),
            sha256: digest,
        };
        request.runtime_dependencies.push(pin.clone());
        result.runtime_dependencies.push(pin);
        assert!(validate_request(&request, &mut inputs(&request), Limits::default()).is_err());
        assert!(validate(&request, &result).is_err());
    }
    let mut request = request();
    let mut result = result();
    let pin = RuntimeDependency {
        package: "kernel".into(),
        version: "0.1.0".into(),
        sha256: sha256(b"kernel"),
    };
    request.runtime_dependencies.push(pin.clone());
    result.runtime_dependencies.push(pin);
    assert!(validate_request(&request, &mut inputs(&request), Limits::default()).is_ok());
    assert_eq!(validate(&request, &result), Ok(vec![]));
}

#[test]
fn unicode_case_aliases_collide_with_each_other_and_host_owned_paths() {
    for (left, right) in [
        ("σ.o", "ς.o"),
        ("I.o", "ı.o"),
        ("ß.o", "ẞ.o"),
        ("Σ", "ς/child.o"),
    ] {
        let request = request();
        let mut result = result();
        result.artifacts[0].path = left.into();
        let mut other = result.artifacts[0].clone();
        other.id = "other".into();
        other.path = right.into();
        result.artifacts.push(other);
        assert!(validate(&request, &result).is_err(), "{left} and {right}");
        result.artifacts.remove(0);
        let policy = Policy {
            toolchains: &[],
            environment: &[],
            reserved_paths: &[left.into()],
            authorize_step: &common::authorize,
            limits: Limits::default(),
        };
        assert!(validate_result(&request, &result, &policy, &mut inputs(&request)).is_err());
    }
    let mut result = result();
    result.artifacts[0].path = "λ/雪.o".into();
    assert_eq!(validate(&request(), &result), Ok(vec![]));
}

#[test]
fn superscript_windows_devices_are_not_regular_output_files() {
    for path in ["COM¹.txt", "x/LPT²", "lpt³.o"] {
        let mut result = result();
        result.artifacts[0].path = path.into();
        assert!(validate(&request(), &result).is_err(), "{path}");
    }
    for path in ["COM0.o", "COM⁴.o", "COM¹x.o"] {
        let mut result = result();
        result.artifacts[0].path = path.into();
        assert_eq!(validate(&request(), &result), Ok(vec![]), "{path}");
    }
}

struct CountingInputs(usize);

impl Inputs for CountingInputs {
    fn read(&mut self, _: &str) -> Result<Vec<u8>, InputError> {
        self.0 += 1;
        Ok(common::TEXT.as_bytes().to_vec())
    }
}

#[test]
fn repeated_diagnostics_load_each_source_once_per_validation() {
    let request = request();
    let mut result = result();
    result.diagnostics = vec![diagnostic(); 4];
    let policy = Policy {
        toolchains: &[],
        environment: &[],
        reserved_paths: &[],
        authorize_step: &common::authorize,
        limits: Limits::default(),
    };
    let mut inputs = CountingInputs(0);
    assert_eq!(
        validate_result(&request, &result, &policy, &mut inputs),
        Ok(vec![])
    );
    assert_eq!(inputs.0, 1);
}
