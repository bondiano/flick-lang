use flick_backend::*;

use super::common::{self, request, result, validate};

#[test]
fn launch_runtime_arguments_and_environment_are_validated_and_policy_checked() {
    let req = request();
    let base_launch = |runtime: Option<Toolchain>| Launch {
        name: "main".into(),
        kind: LaunchKind::Run,
        artifact: "object".into(),
        runtime,
        runtime_arguments: vec![],
        arguments: vec![],
        environment: vec![],
    };

    // Runtime arguments on a direct executable launch are rejected.
    let mut no_rt = result();
    no_rt.artifacts[0].kind = ArtifactKind::Executable;
    let mut launch = base_launch(None);
    launch.runtime_arguments.push("--flag".into());
    no_rt.launches.push(launch);
    assert!(validate(&req, &no_rt).is_err());

    // Duplicate or empty environment keys are rejected.
    for env in [
        vec![OptionValue {
            key: String::new(),
            value: "1".into(),
        }],
        vec![
            OptionValue {
                key: "FLICK_DEVTOOLS".into(),
                value: "ws://localhost:1".into(),
            },
            OptionValue {
                key: "FLICK_DEVTOOLS".into(),
                value: "ws://localhost:2".into(),
            },
        ],
    ] {
        let mut res = result();
        let mut launch = base_launch(Some(common::toolchain()));
        launch.environment = env;
        res.launches.push(launch);
        assert!(validate(&req, &res).is_err());
    }

    // Disallowed launch environment variables and runtime flags are rejected by policy.
    for key in [
        "LD_PRELOAD",
        "DYLD_INSERT_LIBRARIES",
        "NODE_OPTIONS",
        "PATH",
    ] {
        let mut res = result();
        let mut launch = base_launch(Some(common::toolchain()));
        launch.environment.push(OptionValue {
            key: key.into(),
            value: "evil".into(),
        });
        res.launches.push(launch);
        assert!(validate(&req, &res).is_err(), "{key}");
    }
    let mut eval_res = result();
    let mut eval_launch = base_launch(Some(common::toolchain()));
    eval_launch.runtime_arguments = vec!["-e".into(), "process.exit(1)".into()];
    eval_res.launches.push(eval_launch);
    assert!(validate(&req, &eval_res).is_err());

    // NUL bytes in launch runtime arguments or environment values are rejected.
    let mut nul_res = result();
    let mut nul_launch = base_launch(Some(common::toolchain()));
    nul_launch.runtime_arguments.push("bad\0arg".into());
    nul_res.launches.push(nul_launch);
    assert!(validate(&req, &nul_res).is_err());

    // Allowed runtime arguments + allowed environment pass validation.
    let mut ok_res = result();
    let mut ok_launch = base_launch(Some(common::toolchain()));
    ok_launch.runtime_arguments = vec!["--enable-source-maps".into()];
    ok_launch.environment = vec![OptionValue {
        key: "FLICK_DEVTOOLS".into(),
        value: "ws://localhost:34437".into(),
    }];
    ok_res.launches.push(ok_launch);
    assert_eq!(validate(&req, &ok_res), Ok(vec![]));
}
