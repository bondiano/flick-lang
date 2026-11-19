//! The commands of the fixture backend: one of each kind the host checks,
//! and a session that echoes, traps or spins on request.

use std::hint;

use flick_backend::command::{
    self, Descriptor, Diagnostic, File, Handled, Invocation, Parameter, Plan, Step, Style, Tool,
    ToolStep, ValueKind,
};

fn descriptor(
    name: &str,
    positionals: Vec<Parameter>,
    writes: &[&str],
    style: Style,
) -> Descriptor {
    Descriptor {
        name: name.into(),
        summary: format!("the fixture's `{name}`"),
        positionals,
        options: vec![Parameter {
            name: "out".into(),
            kind: ValueKind::Path,
            multiple: false,
            required: false,
            help: "where to write".into(),
        }],
        writes: writes.iter().map(|&w| w.to_owned()).collect(),
        style,
    }
}

pub(super) fn commands() -> Vec<Descriptor> {
    let who = Parameter {
        name: "who".into(),
        kind: ValueKind::Text,
        multiple: false,
        required: true,
        help: "whom to greet".into(),
    };
    vec![
        descriptor("literal-hello", vec![who.clone()], &["$out"], Style::Plan),
        descriptor("greet", vec![who], &["greeted.txt"], Style::Plan),
        // a core command: refused with a warning, never run
        descriptor("build", vec![], &[], Style::Plan),
        descriptor("literal-escape", vec![], &["inside"], Style::Plan),
        descriptor("literal-unpinned", vec![], &["made.txt"], Style::Plan),
        descriptor("echo", vec![], &[], Style::Session),
    ]
}

fn value<'a>(invocation: &'a Invocation, name: &str) -> Option<&'a str> {
    (invocation.arguments.iter())
        .find(|a| a.name == name)
        .and_then(|a| a.values.first())
        .map(String::as_str)
}

pub(super) fn run(invocation: &Invocation) -> Plan {
    let mut plan = Plan::empty();
    let who = value(invocation, "who").unwrap_or("nobody");
    match invocation.name.as_str() {
        "literal-hello" => {
            let out = value(invocation, "out").unwrap_or("hello");
            plan.files
                .push(file(&format!("{out}/hello.txt"), &format!("hello {who}\n")));
            plan.stdout = format!("literal greets {who}\n").into_bytes();
        }
        "greet" => {
            plan.files
                .push(file("greeted.txt", &format!("literal {who}\n")));
        }
        "literal-escape" => plan.files.push(file("../outside.txt", "escaped\n")),
        "literal-unpinned" => plan.steps.push(Step::Tool(ToolStep {
            id: "make".into(),
            tool: Tool {
                id: "node".into(),
                version: "0.0.0".into(),
                sha256: "0".repeat(64),
            },
            arguments: vec![],
            environment: vec![],
            inputs: vec![],
            outputs: vec!["made.txt".into()],
        })),
        _ => {}
    }
    plan
}

fn file(path: &str, text: &str) -> File {
    File {
        path: path.into(),
        bytes: text.as_bytes().to_vec(),
        executable: false,
    }
}

/// A session counts its requests; `close` reports how many it answered.
pub(super) fn handle(state: &[u8], request: &[u8]) -> Result<Handled, String> {
    let count = state.first().copied().unwrap_or(0).saturating_add(1);
    let message: serde_json::Value =
        serde_json::from_slice(request).map_err(|e| format!("not JSON: {e}"))?;
    match message["method"].as_str() {
        Some("trap") => panic!("intentional session trap"),
        Some("spin") => loop {
            hint::black_box(());
        },
        _ => {}
    }
    let response = (message.get("id")).map(|id| {
        let answer = serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": message["params"] });
        answer.to_string().into_bytes()
    });
    Ok(Handled {
        state: vec![count],
        response,
    })
}

pub(super) fn close(state: &[u8]) -> Plan {
    let mut plan = Plan::empty();
    let count = state.first().copied().unwrap_or(0);
    plan.stdout = format!("echo answered {count}\n").into_bytes();
    plan
}

pub(super) fn unknown(name: &str) -> Vec<Diagnostic> {
    command::unknown(name)
}
