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

#[test]
fn windows_console_aliases_are_not_regular_output_paths() {
    for path in [
        "CONIN$",
        "conout$",
        "dir/CONIN$.txt",
        "CONIN$ .txt",
        "NUL .txt",
        "COM1  .dat",
    ] {
        let mut result = result();
        result.artifacts[0].path = path.into();
        assert!(validate(&request(), &result).is_err(), "{path}");
    }
    let mut result = result();
    result.artifacts[0].path = "CONIN_data.txt".into();
    assert_eq!(validate(&request(), &result), Ok(vec![]));
}

#[test]
fn filename_components_fit_common_byte_and_unicode_limits() {
    for path in ["x".repeat(256), "é".repeat(128), "ᾂ".repeat(64)] {
        let mut result = result();
        result.artifacts[0].path = path.clone();
        assert!(validate(&request(), &result).is_err(), "{path}");
    }
    for path in [
        "x".repeat(255),
        format!("{}a", "é".repeat(127)),
        format!("{}abc", "ᾂ".repeat(63)),
    ] {
        let mut result = result();
        result.artifacts[0].path = path;
        assert_eq!(validate(&request(), &result), Ok(vec![]));
    }
}

#[test]
fn launch_identity_includes_the_kind_and_name() {
    let request = request();
    let mut result = result();
    result.artifacts[0].kind = ArtifactKind::Executable;
    let run = Launch {
        name: "main".into(),
        kind: LaunchKind::Run,
        artifact: "object".into(),
        runtime: None,
        arguments: vec![],
    };
    let mut test = run.clone();
    test.kind = LaunchKind::Test;
    result.launches = vec![run, test.clone()];
    assert_eq!(validate(&request, &result), Ok(vec![]));
    result.launches.push(test);
    assert!(validate(&request, &result).is_err());
}

#[test]
fn hfs_ignorable_characters_cannot_bypass_output_ownership() {
    for code in [
        0x200c..=0x200f,
        0x202a..=0x202e,
        0x206a..=0x206f,
        0xfeff..=0xfeff,
    ]
    .into_iter()
    .flatten()
    {
        let character = char::from_u32(code).expect("HFS formatting character");
        let request = request();
        let mut result = result();
        result.artifacts[0].path = "App.iface".into();
        let mut alias = result.artifacts[0].clone();
        alias.id = "alias".into();
        alias.path = format!("Ap{character}p.iface");
        result.artifacts.push(alias.clone());
        assert!(validate(&request, &result).is_err(), "U+{code:04X}");
        result.artifacts[1].path.push_str("/child");
        assert!(
            validate(&request, &result).is_err(),
            "directory alias U+{code:04X}"
        );
        result.artifacts = vec![alias];
        let policy = Policy {
            toolchains: &[],
            environment: &[],
            reserved_paths: &["App.iface".into()],
            authorize_step: &common::authorize,
            limits: Limits::default(),
        };
        assert!(validate_result(&request, &result, &policy, &mut inputs(&request)).is_err());
        for path in [
            character.to_string(),
            format!(".{character}"),
            format!("..{character}"),
        ] {
            result.artifacts[0].path = path;
            assert!(validate(&request, &result).is_err());
        }
        result.artifacts[0].path = format!("👩{character}💻.png");
        assert_eq!(validate(&request, &result), Ok(vec![]));
    }
    let mut result = result();
    result.artifacts[0].path = "App.iface".into();
    let mut distinct = result.artifacts[0].clone();
    distinct.id = "distinct".into();
    distinct.path = "Ap\u{200b}p.iface".into();
    result.artifacts.push(distinct);
    assert_eq!(
        validate(&request(), &result),
        Ok(vec![]),
        "U+200B is not ignored by HFS+"
    );
}

#[test]
fn the_complete_request_budget_is_checked_before_loading_inputs() {
    let base = request();
    let mut encoded = Vec::new();
    ciborium::into_writer(&base, &mut encoded).expect("request");
    let backend = Fallback {
        calls: Cell::new(0),
    };
    let policy = Policy {
        toolchains: &[],
        environment: &[],
        reserved_paths: &[],
        authorize_step: &common::authorize,
        limits: Limits {
            request_bytes: encoded.len() + 128,
            ..Limits::default()
        },
    };
    for case in ["program", "options", "manifest"] {
        let mut request = base.clone();
        match case {
            "program" => request.program = vec![0; 65_536],
            "options" => request.target.options.push(OptionValue {
                key: "large".into(),
                value: "x".repeat(65_536),
            }),
            "manifest" => {
                for index in 0..500 {
                    request.inputs.push(Input {
                        id: format!("resource/{index}"),
                        kind: InputKind::Resource,
                        sha256: sha256(b""),
                        size: 0,
                    });
                }
            }
            _ => unreachable!(),
        }
        let mut reads = 0;
        let error = generate(
            &backend,
            &request,
            &policy,
            |id| {
                reads += 1;
                Some(if id == "source/App" {
                    common::TEXT.as_bytes().to_vec()
                } else {
                    vec![]
                })
            },
            true,
        )
        .err()
        .expect("oversized request");
        assert!(matches!(error, Failure::Protocol(_)));
        assert_eq!(reads, 0, "{case}");
        assert_eq!(backend.calls.get(), 0);
    }
    let generated = generate(
        &backend,
        &base,
        &policy,
        |_| Some(common::TEXT.as_bytes().to_vec()),
        true,
    )
    .expect("valid envelope");
    assert!(generated.cache_key().is_some());
    assert_eq!(backend.calls.get(), 2);
}

#[test]
fn dos_short_name_shapes_cannot_alias_artifacts_or_reserved_paths() {
    for path in ["LONGMO~1.IFA", "LONGDI~1/child.js", "a~123456.x"] {
        let mut result = result();
        result.artifacts[0].path = path.into();
        assert!(validate(&request(), &result).is_err(), "{path}");
        let policy = Policy {
            toolchains: &[],
            environment: &[],
            reserved_paths: &["LongModule.iface".into(), "LongDirectory".into()],
            authorize_step: &common::authorize,
            limits: Limits::default(),
        };
        assert!(validate_result(&request(), &result, &policy, &mut inputs(&request())).is_err());
    }
    for path in [
        "backup~.js",
        "version~next.js",
        "App~1.interface",
        "verylongname~1.js",
    ] {
        let mut result = result();
        result.artifacts[0].path = path.into();
        assert_eq!(validate(&request(), &result), Ok(vec![]), "{path}");
    }
}

#[test]
fn runtime_pins_match_exact_records_independently_of_list_order() {
    let mut request = request();
    request.runtime_dependencies = (0..1_000)
        .map(|index| RuntimeDependency {
            package: format!("runtime-{index}"),
            version: "1.0.0".into(),
            sha256: sha256(index.to_string().as_bytes()),
        })
        .collect();
    let mut result = result();
    result.runtime_dependencies = request.runtime_dependencies.iter().rev().cloned().collect();
    assert_eq!(validate(&request, &result), Ok(vec![]));
    result.runtime_dependencies[0].version = "2.0.0".into();
    assert!(validate(&request, &result).is_err());
}
