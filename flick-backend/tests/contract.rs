mod common;

use std::cell::Cell;

use flick_backend::*;

use common::{descriptor, diagnostic, inputs, request, result, step, validate};

#[test]
fn binary_results_and_requests_round_trip_without_text_conversion() {
    let request = request();
    let result = result();
    let mut bytes = Vec::new();
    ciborium::into_writer(&request, &mut bytes).expect("CBOR");
    let read: BuildRequest = ciborium::from_reader(&bytes[..]).expect("request");
    assert_eq!(read, request);
    bytes.clear();
    ciborium::into_writer(&result, &mut bytes).expect("CBOR");
    let read: BuildResult = ciborium::from_reader(&bytes[..]).expect("result");
    assert_eq!(read.artifacts[0].bytes, [0, 255, 128, 10]);
    assert_eq!(read, result);
    assert_eq!(validate(&request, &read), Ok(vec![]));
}

#[test]
fn binary_executables_need_no_external_linker_when_the_backend_emits_them() {
    let mut result = result();
    result.artifacts[0].kind = ArtifactKind::Executable;
    result.launches.push(Launch {
        name: "main".into(),
        kind: LaunchKind::Run,
        artifact: "object".into(),
        runtime: None,
        arguments: vec![],
    });
    assert_eq!(validate(&request(), &result), Ok(vec![]));
    assert_eq!(result.artifacts[0].bytes, [0, 255, 128, 10]);
}

#[test]
fn handshake_rejects_each_incompatible_dimension() {
    let request = request();
    let descriptor = descriptor();
    assert_eq!(
        handshake(&request.backend, &descriptor, &request.target),
        Ok(())
    );
    for field in [
        "api",
        "ir",
        "language",
        "runtime",
        "environment",
        "profile",
        "mode",
        "capability",
    ] {
        let mut target = request.target.clone();
        let mut descriptor = descriptor.clone();
        match field {
            "api" => target.backend_api = "1.0.0".into(),
            "ir" => target.ir_version = "0.12.0".into(),
            "language" => target.language_version = "0.2.0".into(),
            "runtime" => target.runtime_abis[0].version = "0.2.0".into(),
            "environment" => target.environment.platform = "browser".into(),
            "profile" => descriptor.profiles.clear(),
            "mode" => descriptor.modes.clear(),
            "capability" => target.required_capabilities.push(Capability {
                name: "source-maps".into(),
                version: "1.0.0".into(),
            }),
            _ => unreachable!(),
        }
        assert!(
            handshake(&request.backend, &descriptor, &target).is_err(),
            "{field}"
        );
    }
    let mut pin = request.backend;
    pin.identity.version = "9.9.9".into();
    assert!(handshake(&pin, &descriptor, &request.target).is_err());
}

#[test]
fn prereleases_and_malformed_versions_are_not_accepted_implicitly() {
    let request = request();
    for version in ["0.1.0-dev", "0.1", "0.1.0.1", "garbage"] {
        let mut target = request.target.clone();
        target.backend_api = version.into();
        assert!(
            handshake(&request.backend, &descriptor(), &target).is_err(),
            "{version}"
        );
    }
}

#[test]
fn inputs_are_content_addressed_and_undeclared_reads_fail() {
    let request = request();
    let mut given = inputs(&request);
    assert_eq!(
        given.read("source/App"),
        Ok(common::TEXT.as_bytes().to_vec())
    );
    assert_eq!(given.read("/etc/passwd"), Err(InputError::Undeclared));
    assert!(DeclaredInputs::new(&request.inputs, |_| Some(b"changed".to_vec())).is_err());
    let mut duplicate = request.inputs;
    duplicate.push(duplicate[0].clone());
    assert!(
        DeclaredInputs::new(&duplicate, |_| Some(common::TEXT.as_bytes().to_vec()))
            .err()
            .expect("duplicate input")
            .contains("duplicate input id")
    );
}

#[test]
fn valid_dag_is_sorted_even_when_steps_arrive_in_reverse_order() {
    let mut result = result();
    let mut link = step("link", &["compile"]);
    link.inputs
        .push(InputRef::Artifact("compile-output".into()));
    result.steps = vec![link, step("compile", &[])];
    result.launches.push(Launch {
        name: "main".into(),
        kind: LaunchKind::Run,
        artifact: "link-output".into(),
        runtime: None,
        arguments: vec![],
    });
    assert_eq!(validate(&request(), &result), Ok(vec![1, 0]));
}

#[test]
fn cycles_unknown_steps_and_missing_data_dependencies_are_rejected() {
    for case in ["cycle", "unknown", "missing-producer", "duplicate"] {
        let mut result = result();
        result.steps = vec![step("first", &[]), step("second", &["first"])];
        match case {
            "cycle" => result.steps[0].dependencies.push("second".into()),
            "unknown" => result.steps[0].dependencies.push("unknown".into()),
            "missing-producer" => result.steps[0]
                .inputs
                .push(InputRef::Artifact("second-output".into())),
            "duplicate" => result.steps[1].dependencies.push("first".into()),
            _ => unreachable!(),
        }
        assert!(validate(&request(), &result).is_err(), "{case}");
    }
}

#[test]
fn paths_cannot_escape_alias_or_collide() {
    for path in [
        "/absolute",
        "../escape",
        "a/../b",
        "C:/x",
        "C:x",
        "a\\b",
        "a//b",
        "./x",
        "a/.",
        "a/",
        "a.",
        "a ",
        "NUL.js",
        "a/COM1.txt",
        "a\0b",
        "e\u{301}.js",
    ] {
        let mut result = result();
        result.artifacts[0].path = path.into();
        assert!(validate(&request(), &result).is_err(), "{path:?}");
    }
    for other in ["obj/app.o", "OBJ/APP.O", "obj", "obj/app.o/child"] {
        let mut result = result();
        let mut second = result.artifacts[0].clone();
        second.id = "other".into();
        second.path = other.into();
        result.artifacts.push(second);
        assert!(validate(&request(), &result).is_err(), "{other}");
    }
}

#[test]
fn duplicate_ids_and_host_owned_outputs_are_rejected() {
    let request = request();
    let mut result = result();
    result.steps.push(step("object", &[]));
    assert!(validate(&request, &result).is_err());
    result.steps.clear();
    let reserved = vec!["obj/app.o".into()];
    let policy = Policy {
        authorize_step: &common::authorize,
        toolchains: &[],
        environment: &[],
        reserved_paths: &reserved,
        limits: Limits::default(),
    };
    assert!(validate_result(&request, &result, &policy, &mut inputs(&request)).is_err());
}

#[test]
fn undeclared_toolchain_input_and_environment_are_rejected() {
    for case in [
        "toolchain",
        "source",
        "artifact",
        "environment",
        "launch",
        "runtime",
    ] {
        let mut result = result();
        result.steps.push(step("compile", &[]));
        match case {
            "toolchain" => result.steps[0].toolchain.sha256 = "f".repeat(64),
            "source" => result.steps[0]
                .inputs
                .push(InputRef::Source("secret".into())),
            "artifact" => result.steps[0]
                .inputs
                .push(InputRef::Artifact("missing".into())),
            "environment" => result.steps[0].environment.push(OptionValue {
                key: "HOME".into(),
                value: "/tmp".into(),
            }),
            "launch" => result.launches.push(Launch {
                name: "main".into(),
                kind: LaunchKind::Run,
                artifact: "object".into(),
                runtime: None,
                arguments: vec![],
            }),
            "runtime" => result.runtime_dependencies.push(RuntimeDependency {
                package: "unlocked".into(),
                version: "1.0.0".into(),
                sha256: "a".repeat(64),
            }),
            _ => unreachable!(),
        }
        assert!(validate(&request(), &result).is_err(), "{case}");
    }
}

#[test]
fn a_pinned_toolchain_does_not_authorize_arbitrary_command_arguments() {
    let mut result = result();
    let mut command = step("compile", &[]);
    command.arguments = vec!["build".into(), "-o".into(), "../../outside".into()];
    result.steps.push(command);
    assert!(validate(&request(), &result).is_err());
}

#[test]
fn output_limits_cover_payloads_diagnostics_and_metadata() {
    let request = request();
    for case in ["file", "total", "count", "steps", "diagnostics", "metadata"] {
        let mut result = result();
        let mut limits = Limits::default();
        match case {
            "file" => limits.artifact_bytes = 3,
            "total" => limits.result_bytes = 1,
            "count" => limits.files = 0,
            "steps" => {
                result.steps.push(step("compile", &[]));
                limits.steps = 0;
            }
            "diagnostics" => {
                result.diagnostics.push(diagnostic());
                limits.diagnostics = 0;
            }
            "metadata" => {
                result.diagnostics.push(diagnostic());
                limits.string_bytes = 1;
            }
            _ => unreachable!(),
        }
        let policy = Policy {
            authorize_step: &common::authorize,
            toolchains: &request.toolchains,
            environment: &[],
            reserved_paths: &[],
            limits,
        };
        assert!(
            validate_result(&request, &result, &policy, &mut inputs(&request)).is_err(),
            "{case}"
        );
    }
}

#[test]
fn diagnostic_spans_are_checked_against_utf8_source_bytes() {
    let request = request();
    for span in [
        Span {
            file: 1,
            start: 0,
            end: 0,
        },
        Span {
            file: 0,
            start: 3,
            end: 2,
        },
        Span {
            file: 0,
            start: 0,
            end: 99,
        },
    ] {
        let mut result = result();
        let mut d = diagnostic();
        d.span = Some(span);
        result.diagnostics.push(d);
        assert!(validate(&request, &result).is_err(), "{span:?}");
    }
    let mut request = request;
    request.inputs[0].size = 2;
    request.inputs[0].sha256 = sha256("ё".as_bytes());
    let mut inputs =
        DeclaredInputs::new(&request.inputs, |_| Some("ё".as_bytes().to_vec())).expect("input");
    let mut result = result();
    let mut d = diagnostic();
    d.span = Some(Span {
        file: 0,
        start: 0,
        end: 1,
    });
    result.diagnostics.push(d);
    let policy = Policy {
        authorize_step: &common::authorize,
        toolchains: &[],
        environment: &[],
        reserved_paths: &[],
        limits: Limits::default(),
    };
    assert!(validate_result(&request, &result, &policy, &mut inputs).is_err());
}

struct Mock {
    calls: Cell<usize>,
    refuse: bool,
    unstable: bool,
}
impl Backend for Mock {
    fn describe(&self) -> Descriptor {
        descriptor()
    }
    fn check_target(
        &self,
        _: &BuildRequest,
        _: &mut dyn Inputs,
    ) -> Result<TargetSupport, Vec<Diagnostic>> {
        if self.refuse {
            Err(vec![diagnostic()])
        } else {
            Ok(TargetSupport {
                capabilities: vec![],
            })
        }
    }
    fn generate(
        &self,
        _: &BuildRequest,
        _: &mut dyn Inputs,
    ) -> Result<BuildResult, Vec<Diagnostic>> {
        let n = self.calls.replace(self.calls.get() + 1);
        let mut result = result();
        if self.unstable {
            result.artifacts[0]
                .bytes
                .push(u8::try_from(n).expect("two calls"));
        }
        Ok(result)
    }
}

#[test]
fn preflight_refusal_happens_before_generation() {
    let backend = Mock {
        calls: Cell::new(0),
        refuse: true,
        unstable: false,
    };
    let request = request();
    let policy = Policy {
        authorize_step: &common::authorize,
        toolchains: &[],
        environment: &[],
        reserved_paths: &[],
        limits: Limits::default(),
    };
    let result = generate(
        &backend,
        &request,
        &policy,
        |_| Some(common::TEXT.as_bytes().to_vec()),
        true,
    );
    assert!(
        matches!(result, Err(Failure::Rejected(_))),
        "{}",
        result.err().expect("failure")
    );
    assert_eq!(backend.calls.get(), 0);
}

#[test]
fn unstable_generation_is_rejected_and_unchecked_generation_is_uncacheable() {
    let backend = Mock {
        calls: Cell::new(0),
        refuse: false,
        unstable: true,
    };
    let request = request();
    let policy = Policy {
        authorize_step: &common::authorize,
        toolchains: &[],
        environment: &[],
        reserved_paths: &[],
        limits: Limits::default(),
    };
    assert!(matches!(
        generate(
            &backend,
            &request,
            &policy,
            |_| Some(common::TEXT.as_bytes().to_vec()),
            true
        ),
        Err(Failure::Unstable)
    ));
    let generated = generate(
        &backend,
        &request,
        &policy,
        |_| Some(common::TEXT.as_bytes().to_vec()),
        false,
    )
    .expect("uncached");
    assert_eq!(generated.cache_key(), None);
}

#[test]
fn stable_validated_results_become_cacheable_after_two_generations() {
    let backend = Mock {
        calls: Cell::new(0),
        refuse: false,
        unstable: false,
    };
    let request = request();
    let policy = Policy {
        authorize_step: &common::authorize,
        toolchains: &[],
        environment: &[],
        reserved_paths: &[],
        limits: Limits::default(),
    };
    let generated = generate(
        &backend,
        &request,
        &policy,
        |_| Some(common::TEXT.as_bytes().to_vec()),
        true,
    )
    .expect("stable generation");
    assert_eq!(backend.calls.get(), 2);
    assert_eq!(generated.cache_key(), Some(cache_key(&request).as_str()));
    assert_eq!(generated.result(), &result());
    assert!(generated.execution_order().is_empty());
}

#[test]
fn incompatible_handshake_reads_no_inputs_and_never_generates() {
    let backend = Mock {
        calls: Cell::new(0),
        refuse: false,
        unstable: false,
    };
    let mut request = request();
    request.target.ir_version = "0.12.0".into();
    let policy = Policy {
        authorize_step: &common::authorize,
        toolchains: &[],
        environment: &[],
        reserved_paths: &[],
        limits: Limits::default(),
    };
    let reads = Cell::new(0);
    let error = generate(
        &backend,
        &request,
        &policy,
        |_| {
            reads.set(reads.get() + 1);
            None
        },
        true,
    )
    .err()
    .expect("incompatible IR");
    assert!(error.to_string().contains("IR version"));
    assert_eq!(reads.get(), 0);
    assert_eq!(backend.calls.get(), 0);
}

#[test]
fn cache_key_tracks_backend_runtime_toolchain_environment_and_side_inputs() {
    let request = request();
    let key = cache_key(&request);
    for case in [
        "backend",
        "profile",
        "environment",
        "mode",
        "input",
        "toolchain",
        "runtime",
        "option",
        "ir",
    ] {
        let mut changed = request.clone();
        match case {
            "backend" => changed.backend.sha256 = "b".repeat(64),
            "profile" => changed.target.profile = Profile::Release,
            "environment" => changed.target.environment.platform = "browser".into(),
            "mode" => changed.target.mode = Mode::PerModule,
            "input" => changed.inputs[0].sha256 = "c".repeat(64),
            "toolchain" => changed.toolchains[0].version = "1.24.0".into(),
            "runtime" => changed.target.runtime_abis[0].version = "0.1.1".into(),
            "option" => changed.target.options.push(OptionValue {
                key: "runtime".into(),
                value: "rt".into(),
            }),
            "ir" => changed.program.push(0),
            _ => unreachable!(),
        }
        assert_ne!(cache_key(&changed), key, "{case}");
    }
}
