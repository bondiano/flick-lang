use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{self, Command};
use std::time::{Duration, Instant};
use std::{env, fs, slice, thread};

use flick_backend::host::runtime::Cancellation;
use flick_backend::{
    Artifact, ArtifactKind, BuildResult, InputRef, StepOutput, Toolchain, ToolchainStep,
};

use super::{Failure, run};
use crate::backends::tools::Tool;
use crate::backends::tools::tests::script;
use crate::{artifact, component};

/// A fake `node`: `copy IN OUT`, `fail OUT`, `link OUT`, `wait PIDFILE`.
const NODE: &str = r#"#!/bin/sh
case "$1" in
--version) echo v1.0.0 ;;
copy) cat "$2" > "$3" ;;
fail) echo partial > "$2"; echo boom >&2; exit 3 ;;
link) ln -s /etc/hostname "$2" ;;
wait) sleep 30 & echo $! > "$2"; wait ;;
esac
"#;

struct Case {
    root: PathBuf,
    out: PathBuf,
    tool: Tool,
}

fn case(name: &str) -> Case {
    let root = env::temp_dir().join(format!("flick-execute-{name}-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("bin")).expect("directory");
    script(&root.join("bin/node"), NODE.as_bytes(), 0o755);
    let tool = Tool::at_path("node", &root.join("bin/node")).expect("a fake node");
    Case {
        out: root.join("out"),
        root,
        tool,
    }
}

fn step(
    tool: &Toolchain,
    arguments: &[&str],
    inputs: Vec<InputRef>,
    output: &str,
) -> ToolchainStep {
    ToolchainStep {
        id: "make".into(),
        dependencies: vec![],
        toolchain: tool.clone(),
        arguments: arguments.iter().map(|&a| a.to_owned()).collect(),
        environment: vec![],
        inputs,
        outputs: vec![StepOutput {
            id: "made".into(),
            path: output.into(),
            kind: ArtifactKind::Resource,
        }],
    }
}

impl Case {
    fn run(
        &self,
        steps: Vec<ToolchainStep>,
        cancel: &Cancellation,
    ) -> Result<Vec<artifact::Artifact>, Failure> {
        let result = BuildResult {
            artifacts: vec![Artifact {
                id: "source".into(),
                path: "src/a.txt".into(),
                kind: ArtifactKind::Source,
                bytes: b"from an artifact".to_vec(),
            }],
            steps,
            runtime_dependencies: vec![],
            launches: vec![],
            diagnostics: vec![],
        };
        let sources = BTreeMap::from([("side".to_owned(), b"from a side input".to_vec())]);
        let order: Vec<usize> = (0..result.steps.len()).collect();
        run(
            &result,
            &order,
            &sources,
            slice::from_ref(&self.tool),
            &self.out,
            cancel,
        )
    }

    /// No work directory is left beside the output.
    fn clean(&self) -> bool {
        fs::read_dir(&self.root)
            .expect("root")
            .flatten()
            .all(|e| !e.file_name().to_string_lossy().contains("flick-work"))
    }
}

fn token() -> Cancellation {
    Cancellation::new(component::engine().expect("engine"))
}

#[test]
fn a_step_sees_its_declared_inputs_and_gives_its_declared_outputs() {
    let case = case("copy");
    let pin = case.tool.pin.clone();
    let made = case
        .run(
            vec![step(
                &pin,
                &["copy", "src/a.txt", "a.out"],
                vec![InputRef::Artifact("source".into())],
                "a.out",
            )],
            &token(),
        )
        .unwrap_or_else(|_| panic!("the step runs"));
    assert_eq!(made[0].path, "a.out");
    assert_eq!(made[0].contents, b"from an artifact");
    let side = case
        .run(
            vec![step(
                &pin,
                &["copy", ".flick-inputs/side", "b.out"],
                vec![InputRef::Source("side".into())],
                "b.out",
            )],
            &token(),
        )
        .unwrap_or_else(|_| panic!("the step runs"));
    assert_eq!(side[0].contents, b"from a side input");
    let undeclared = case.run(
        vec![step(&pin, &["copy", "src/a.txt", "c.out"], vec![], "c.out")],
        &token(),
    );
    assert!(
        matches!(undeclared, Err(Failure::Step(_))),
        "an undeclared input is not there"
    );
    assert!(case.clean());
    assert!(!case.out.exists(), "nothing is published");
    fs::remove_dir_all(&case.root).expect("cleanup");
}

#[test]
fn a_failing_tool_or_a_symlink_output_fails_its_step() {
    let case = case("fail");
    let pin = case.tool.pin.clone();
    let Err(Failure::Step(why)) = case.run(
        vec![step(&pin, &["fail", "a.out"], vec![], "a.out")],
        &token(),
    ) else {
        panic!("a failed step");
    };
    assert!(why.contains("boom") && why.contains("make"), "{why}");
    let Err(Failure::Step(why)) = case.run(
        vec![step(&pin, &["link", "a.out"], vec![], "a.out")],
        &token(),
    ) else {
        panic!("a symlink output");
    };
    assert!(why.contains("not a regular file"), "{why}");
    assert!(case.clean());
    fs::remove_dir_all(&case.root).expect("cleanup");
}

#[test]
fn cancellation_ends_the_tool_and_what_it_started() {
    let case = case("cancel");
    let pid_file = case.root.join("child.pid");
    let pin = case.tool.pin.clone();
    let cancel = token();
    let stopper = cancel.clone();
    let watched = pid_file.clone();
    let canceller = thread::spawn(move || {
        while !watched.exists() || fs::read_to_string(&watched).is_ok_and(|p| p.trim().is_empty()) {
            thread::sleep(Duration::from_millis(10));
        }
        stopper.cancel();
    });
    let started = Instant::now();
    let path = pid_file.to_string_lossy().into_owned();
    let result = case.run(vec![step(&pin, &["wait", &path], vec![], "a.out")], &cancel);
    canceller.join().expect("canceller");
    assert!(matches!(result, Err(Failure::Cancelled)));
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
    let pid = fs::read_to_string(&pid_file).expect("pid");
    let alive = |pid: &str| {
        Command::new("kill")
            .args(["-0", pid.trim()])
            .status()
            .is_ok_and(|s| s.success())
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while alive(&pid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(!alive(&pid), "the tool's own child ends too");
    assert!(case.clean());
    fs::remove_dir_all(&case.root).expect("cleanup");
}
