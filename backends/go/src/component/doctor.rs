//! Pure read-only `doctor` plan for the official Go backend.

use flick_backend::command::{
    self, Descriptor, Diagnostic, Invocation, Parameter, Plan, Severity, Style, ValueKind,
};

pub(super) fn descriptor() -> Descriptor {
    let param = |name: &str, multiple: bool, help: &str| Parameter {
        name: name.into(),
        kind: ValueKind::Text,
        multiple,
        required: false,
        help: help.into(),
    };
    Descriptor {
        name: "doctor".into(),
        summary: "Checks the Go toolchain for `flick doctor`".into(),
        positionals: vec![],
        options: vec![
            param(
                "in-project",
                false,
                "Whether `flick doctor` is running inside a project",
            ),
            param("compiler", false, "Flick compiler version"),
            param("granted-tools", true, "Tools granted to `flick/go`"),
        ],
        inputs: vec![],
        reads: vec![],
        writes: vec![],
        style: Style::Plan,
    }
}

fn push(lines: &mut Vec<Diagnostic>, ok: Option<bool>, what: &str, detail: &str) {
    let (code, severity) = match ok {
        Some(true) => ("doctor-ok", Severity::Note),
        Some(false) => ("doctor-fail", Severity::Error),
        None => ("doctor-note", Severity::Note),
    };
    lines.push(Diagnostic {
        code: code.into(),
        severity,
        message: format!("{what}\n{detail}"),
        notes: vec![],
        span: None,
        help: None,
    });
}

pub(super) fn run(invocation: &Invocation) -> Result<Plan, Vec<Diagnostic>> {
    if invocation.name != "doctor" {
        return Err(command::unknown(&invocation.name));
    }
    let arg = |name: &str| {
        invocation
            .arguments
            .iter()
            .find(|a| a.name == name)
            .map_or(&[][..], |a| a.values.as_slice())
    };
    let in_project = arg("in-project").first().map(String::as_str) == Some("true");
    let granted = arg("granted-tools");
    let go = invocation
        .tools
        .iter()
        .find(|t| t.id == "go" && !t.sha256.is_empty() && !t.version.is_empty());

    let mut lines = Vec::new();
    match go {
        Some(tool) => push(&mut lines, Some(true), "Go", &tool.version),
        None => push(
            &mut lines,
            Some(false),
            "Go",
            "not found (install Go 1.23 or later to build for `target = \"go\"`)",
        ),
    }
    if in_project && !granted.iter().any(|t| t == "go") {
        push(
            &mut lines,
            None,
            "tools",
            "`[tools]` does not allow `flick/go` to run `go`: add `\"flick/go\" = [\"go\"]`",
        );
    }

    let mut plan = Plan::empty();
    plan.diagnostics = lines;
    Ok(plan)
}
