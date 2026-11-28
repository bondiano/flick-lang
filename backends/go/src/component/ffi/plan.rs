//! Command descriptors, pinned `go` tool checks, and scratch `inspect.go`
//! step planning for `go:ffi import` and `go:ffi check`.

use std::collections::BTreeMap;
use std::fmt::Write;

use flick_backend::command::{
    Descriptor, Diagnostic, File, Invocation, Parameter, Plan, Step, Style, Tool, ToolStep,
    ValueKind, Variable,
};

use super::super::packages::parse_go_mod;
use super::{State, error, input, text};

const INSPECT_GO: &str = include_str!("inspect.go");
const STEP_INSPECT_GO: &str = ".flick-step/inspect.go";
const STEP_GO_MOD: &str = ".flick-step/go.mod";
const STEP_TARGET_GO_MOD: &str = ".flick-step/target/go.mod";
const STEP_TARGET_GO_SUM: &str = ".flick-step/target/go.sum";

fn parameter(name: &str, kind: ValueKind, multiple: bool, required: bool, help: &str) -> Parameter {
    Parameter {
        name: name.into(),
        kind,
        multiple,
        required,
        help: help.into(),
    }
}

pub(in super::super) fn descriptors() -> Vec<Descriptor> {
    let import = Descriptor {
        name: "ffi import".into(),
        summary: "Writes `<out>/<Pkg>/Generated.fl`, bindings to a Go package via `go/types`"
            .into(),
        positionals: vec![parameter(
            "package",
            ValueKind::Text,
            false,
            true,
            "The Go package import path (stdlib, dependency, or local module package)",
        )],
        options: vec![
            parameter(
                "out",
                ValueKind::Path,
                false,
                true,
                "Directory that gets `<Pkg>/Generated.fl`",
            ),
            parameter(
                "select",
                ValueKind::Text,
                false,
                false,
                "Import only these exports, comma-separated",
            ),
            parameter(
                "dir",
                ValueKind::Path,
                false,
                false,
                "Local Go module directory containing `go.mod`",
            ),
        ],
        inputs: vec![
            "$out".into(),
            "flick.toml".into(),
            "go.mod".into(),
            "go.sum".into(),
        ],
        reads: vec!["$dir".into()],
        writes: vec!["$out".into()],
        style: Style::Plan,
    };
    let check = Descriptor {
        name: "ffi check".into(),
        summary: "Checks generated and reviewed Go bindings against their Go packages".into(),
        positionals: vec![parameter(
            "paths",
            ValueKind::Path,
            true,
            false,
            "Files or directories to check; by default the project's source root",
        )],
        options: vec![parameter(
            "dir",
            ValueKind::Path,
            false,
            false,
            "Local Go module directory containing `go.mod`",
        )],
        inputs: vec![
            "$paths".into(),
            "$sources".into(),
            "flick.toml".into(),
            "go.mod".into(),
            "go.sum".into(),
        ],
        reads: vec!["$dir".into()],
        writes: vec![],
        style: Style::Plan,
    };
    vec![import, check]
}

pub(super) fn go_tool(invocation: &Invocation) -> Result<&Tool, Vec<Diagnostic>> {
    let found = invocation.tools.iter().find(|t| t.id == "go");
    if found.is_some_and(|t| t.sha256.is_empty()) {
        return Err(vec![Diagnostic::error(
            "P015",
            format!("`{}` runs `go`, which is not on PATH", invocation.name),
        )]);
    }
    found.ok_or_else(|| {
        let mut d = Diagnostic::error(
            "P051",
            format!(
                "`{}` runs `go`, which `[tools]` does not allow `flick/go`",
                invocation.name
            ),
        );
        d.help = Some("allow it in `flick.toml`: `[tools]` `\"flick/go\" = [\"go\"]`".into());
        vec![d]
    })
}

pub(super) fn surface_path(i: usize) -> String {
    format!("surface-{i}.json")
}

/// Extracts `[target.go.dependencies]` from `flick.toml` and `require` lines
/// from `go.mod` so the scratch inspection module can resolve third-party Go
/// dependencies offline via `GOPROXY` / `GOMODCACHE`.
fn target_go_mod(invocation: &Invocation) -> String {
    let mut requires: BTreeMap<String, String> = BTreeMap::new();
    if let Some(go_mod_file) = input(invocation, "go.mod") {
        let (_, mod_reqs) = parse_go_mod(&text(go_mod_file));
        requires.extend(mod_reqs);
    }
    if let Some(toml_file) = input(invocation, "flick.toml")
        && let Ok(parsed) = toml::from_str::<toml::Value>(&text(toml_file))
        && let Some(deps) = parsed
            .get("target")
            .and_then(|t| t.get("go"))
            .and_then(|g| g.get("dependencies"))
            .and_then(toml::Value::as_table)
    {
        for (mod_path, ver) in deps {
            if let Some(v) = ver.as_str() {
                requires.insert(mod_path.clone(), v.to_owned());
            }
        }
    }
    let mut out = String::from("module flick.app/ffi-target\n\ngo 1.23\n");
    if !requires.is_empty() {
        out.push_str("\nrequire (\n");
        for (mod_path, ver) in &requires {
            let _ = writeln!(out, "\t{mod_path} {ver}");
        }
        out.push_str(")\n");
    }
    out
}

fn scratch_files(invocation: &Invocation) -> (Vec<File>, Vec<String>) {
    let mut files = vec![
        File {
            path: STEP_INSPECT_GO.into(),
            bytes: INSPECT_GO.as_bytes().to_vec(),
            executable: false,
        },
        File {
            path: STEP_GO_MOD.into(),
            bytes: b"module flick.app/inspect\n\ngo 1.23\n".to_vec(),
            executable: false,
        },
        File {
            path: STEP_TARGET_GO_MOD.into(),
            bytes: target_go_mod(invocation).into_bytes(),
            executable: false,
        },
    ];
    let mut inputs = vec![
        STEP_INSPECT_GO.to_owned(),
        STEP_GO_MOD.to_owned(),
        STEP_TARGET_GO_MOD.to_owned(),
    ];
    if let Some(sum) = input(invocation, "go.sum") {
        files.push(File {
            path: STEP_TARGET_GO_SUM.into(),
            bytes: sum.bytes.clone(),
            executable: false,
        });
        inputs.push(STEP_TARGET_GO_SUM.to_owned());
    }
    (files, inputs)
}

pub(super) fn planned(
    invocation: &Invocation,
    go: &Tool,
    packages: &[String],
    moddir: Option<&str>,
    state: &State,
) -> Plan {
    let (files, step_inputs) = scratch_files(invocation);
    let env = |name: &str, value: &str| Variable {
        name: name.into(),
        value: value.into(),
    };
    let steps = packages
        .iter()
        .enumerate()
        .map(|(i, package)| {
            let out_rel = format!("../{}", surface_path(i));
            let mut arguments = vec![
                "run".into(),
                "-C".into(),
                ".flick-step".into(),
                "./inspect.go".into(),
                "--out".into(),
                out_rel,
                "--targetdir".into(),
                "target".into(),
            ];
            let mut reads = Vec::new();
            if let Some(dir) = moddir {
                reads.push(dir.to_owned());
                arguments.push("--moddir".into());
                arguments.push(format!("../{dir}"));
            }
            arguments.push(package.clone());
            Step::Tool(ToolStep {
                id: format!("inspect {package}"),
                tool: go.clone(),
                arguments,
                environment: vec![
                    env("GOFLAGS", "-mod=mod"),
                    env("GOTOOLCHAIN", "local"),
                    env("CGO_ENABLED", "0"),
                ],
                inputs: step_inputs.clone(),
                reads,
                outputs: vec![surface_path(i)],
                in_place: false,
            })
        })
        .collect();
    let mut plan = Plan::empty();
    plan.files = files;
    plan.steps = steps;
    plan.resume = Some(serde_json::to_vec(state).expect("the state serializes"));
    plan
}

pub(super) fn validate_moddir(moddir: Option<&str>) -> Result<Option<&str>, Vec<Diagnostic>> {
    if moddir == Some("") {
        return Err(vec![error(
            "`--dir` must name a subdirectory containing a local Go module; for dependencies declared in the project root's `go.mod` or `[target.go.dependencies]`, omit `--dir`"
                .into(),
        )]);
    }
    Ok(moddir)
}
