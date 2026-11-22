//! Delegated client entry runners (`run-client` and `dev-client`) for the
//! JavaScript backend: builds and previews or serves a browser entry via Vite.

use flick_backend::command::{
    Descriptor, Diagnostic, File, Invocation, Parameter, Plan, Severity, Step, Style, Tool,
    ToolStep, ValueKind, unknown,
};
use serde_json::Value;

pub(super) const NAMES: [&str; 2] = ["run-client", "dev-client"];

const VITE_MANIFEST: &str = "node_modules/vite/package.json";
const DEFAULT_VITE_BIN: &str = "bin/vite.js";

fn parameter(name: &str, kind: ValueKind, multiple: bool, help: &str) -> Parameter {
    Parameter {
        name: name.into(),
        kind,
        multiple,
        required: false,
        help: help.into(),
    }
}

pub(super) fn descriptors() -> Vec<Descriptor> {
    vec![
        Descriptor {
            name: "run-client".into(),
            summary: "Builds and previews a browser client entry with Vite".into(),
            positionals: vec![parameter(
                "args",
                ValueKind::Text,
                true,
                "Arguments forwarded to `vite preview`",
            )],
            options: vec![parameter(
                "entry",
                ValueKind::Text,
                false,
                "Client entry name",
            )],
            inputs: vec![VITE_MANIFEST.into()],
            reads: vec![],
            writes: vec!["dist".into()],
            style: Style::Plan,
        },
        Descriptor {
            name: "dev-client".into(),
            summary: "Starts the Vite development server for a browser client entry".into(),
            positionals: vec![parameter(
                "args",
                ValueKind::Text,
                true,
                "Arguments forwarded to Vite",
            )],
            options: vec![parameter(
                "entry",
                ValueKind::Text,
                false,
                "Client entry name",
            )],
            inputs: vec![VITE_MANIFEST.into()],
            reads: vec![],
            writes: vec![],
            style: Style::Plan,
        },
    ]
}

fn values<'a>(invocation: &'a Invocation, name: &str) -> &'a [String] {
    invocation
        .arguments
        .iter()
        .find(|a| a.name == name)
        .map_or(&[], |a| a.values.as_slice())
}

fn input<'a>(invocation: &'a Invocation, path: &str) -> Option<&'a File> {
    invocation.inputs.iter().find(|f| f.path == path)
}

fn vite_bin_path(manifest_file: &File) -> Result<String, Vec<Diagnostic>> {
    let parsed: Value = serde_json::from_slice(&manifest_file.bytes).map_err(|err| {
        vec![Diagnostic::error(
            "P017",
            format!("invalid `{VITE_MANIFEST}`: {err}"),
        )]
    })?;
    let rel = match parsed.get("bin") {
        Some(Value::String(s)) => s.as_str(),
        Some(Value::Object(map)) => map
            .get("vite")
            .and_then(Value::as_str)
            .unwrap_or(DEFAULT_VITE_BIN),
        _ => DEFAULT_VITE_BIN,
    };
    let clean = safe_relative_bin(rel).ok_or_else(|| {
        vec![Diagnostic::error(
            "P017",
            format!("invalid Vite bin path `{rel}` in `{VITE_MANIFEST}`"),
        )]
    })?;
    Ok(format!("node_modules/vite/{clean}"))
}

fn safe_relative_bin(path: &str) -> Option<String> {
    if path.is_empty() || path.contains(['\\', '\0']) || path.starts_with('/') {
        return None;
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            part => parts.push(part),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn node_tool(invocation: &Invocation) -> Result<Tool, Vec<Diagnostic>> {
    match invocation.tools.iter().find(|t| t.id == "node") {
        None => {
            let mut d = Diagnostic::error(
                "P051",
                format!(
                    "`{}` runs `node`, which `[tools]` does not allow `flick/js`",
                    invocation.name
                ),
            );
            d.help = Some(
                "allow it in `flick.toml`: `[tools]` `\"flick/js\" = [\"pnpm\", \"node\"]`".into(),
            );
            Err(vec![d])
        }
        Some(t) if t.sha256.is_empty() => Err(vec![Diagnostic::error(
            "P015",
            "`node` is not on PATH; install Node.js to run client entries".into(),
        )]),
        Some(t) => Ok(t.clone()),
    }
}

pub(super) fn run(invocation: &Invocation) -> Result<Plan, Vec<Diagnostic>> {
    let Some(vite_manifest) = input(invocation, VITE_MANIFEST) else {
        let message = "the client entry needs Vite: install it with `flick install`".to_owned();
        if invocation.name == "dev-client" {
            let mut plan = Plan::empty();
            plan.diagnostics.push(Diagnostic {
                code: "P017".into(),
                severity: Severity::Note,
                message,
                notes: vec![],
                span: None,
                help: None,
            });
            return Ok(plan);
        }
        return Err(vec![Diagnostic::error("P017", message)]);
    };

    let vite_bin = vite_bin_path(vite_manifest)?;
    let node = node_tool(invocation)?;
    let extra = values(invocation, "args");
    let mut plan = Plan::empty();
    match invocation.name.as_str() {
        "run-client" => {
            plan.steps.push(Step::Tool(ToolStep {
                id: "vite-build".into(),
                tool: node.clone(),
                arguments: vec![vite_bin.clone(), "build".into()],
                environment: vec![],
                inputs: vec![],
                reads: vec![],
                outputs: vec!["dist".into()],
                in_place: true,
            }));
            let mut preview_args = vec![vite_bin, "preview".into()];
            preview_args.extend(extra.iter().cloned());
            plan.steps.push(Step::Tool(ToolStep {
                id: "vite-preview".into(),
                tool: node,
                arguments: preview_args,
                environment: vec![],
                inputs: vec![],
                reads: vec![],
                outputs: vec![],
                in_place: true,
            }));
        }
        "dev-client" => {
            let mut dev_args = vec![vite_bin];
            dev_args.extend(extra.iter().cloned());
            plan.steps.push(Step::Tool(ToolStep {
                id: "vite-dev".into(),
                tool: node,
                arguments: dev_args,
                environment: vec![],
                inputs: vec![],
                reads: vec![],
                outputs: vec![],
                in_place: true,
            }));
        }
        other => return Err(unknown(other)),
    }
    Ok(plan)
}
