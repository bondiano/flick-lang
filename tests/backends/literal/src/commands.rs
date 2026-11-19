//! The commands of the fixture backend: one of each kind the host checks,
//! commands that read inputs, link a step's reads and resume after it, and
//! a session that echoes, traps or spins on request.

use std::fmt::Write as _;
use std::hint;
use std::path::Path;

use flick_backend::command::{
    self, Descriptor, Diagnostic, File, Handled, Invocation, Parameter, Plan, Severity, Span, Step,
    Style, Tool, ToolStep, ValueKind,
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
        inputs: vec![],
        reads: vec![],
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
        Descriptor {
            inputs: vec!["$sources".into(), "notes.txt".into()],
            ..descriptor("literal-read", vec![], &["read.txt"], Style::Plan)
        },
        Descriptor {
            reads: vec!["data".into()],
            ..descriptor("literal-resume", vec![], &["resumed.txt"], Style::Plan)
        },
        Descriptor {
            reads: vec!["data".into()],
            ..descriptor("literal-sneak", vec![], &[], Style::Plan)
        },
    ]
}

/// A step of the granted `node` that copies `data/in.txt` to `copy.txt`,
/// reading `read` in place.
fn copy_step(invocation: &Invocation, read: &str) -> Option<Step> {
    let node = invocation.tools.iter().find(|t| t.id == "node")?;
    Some(Step::Tool(ToolStep {
        id: "copy".into(),
        tool: node.clone(),
        arguments: vec![
            "-e".into(),
            "const fs = require('fs'); fs.writeFileSync('copy.txt', fs.readFileSync('data/in.txt'))"
                .into(),
        ],
        environment: vec![],
        inputs: vec![],
        reads: vec![read.into()],
        outputs: vec!["copy.txt".into()],
    }))
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
        "literal-read" => {
            let mut listed = String::new();
            for f in &invocation.inputs {
                let _ = writeln!(listed, "{} {}", f.path, f.bytes.len());
            }
            plan.files.push(file("read.txt", &listed));
            let module = (invocation.inputs.iter())
                .find(|f| Path::new(&f.path).extension().is_some_and(|e| e == "fl"));
            plan.diagnostics.extend(module.map(|f| Diagnostic {
                severity: Severity::Note,
                span: Some(Span {
                    path: f.path.clone(),
                    start: 0,
                    end: 6,
                }),
                help: Some("literal helps".into()),
                ..Diagnostic::error("P013", "literal points at its first module".into())
            }));
        }
        "literal-resume" => {
            plan.steps.extend(copy_step(invocation, "data"));
            plan.resume = Some(b"r1".to_vec());
        }
        "literal-sneak" => {
            plan.steps.extend(copy_step(invocation, "src"));
            plan.resume = Some(b"s1".to_vec());
        }
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
            reads: vec![],
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

/// The plan after `literal-resume`'s step: what it copied, never written.
pub(super) fn resume(state: &[u8], outputs: &[File]) -> Plan {
    let mut plan = Plan::empty();
    let mut copied = String::new();
    for f in outputs {
        let _ = write!(copied, "{}: {}", f.path, String::from_utf8_lossy(&f.bytes));
    }
    let state = String::from_utf8_lossy(state);
    plan.files
        .push(file("resumed.txt", &format!("resumed {state} {copied}")));
    plan
}

pub(super) fn unknown(name: &str) -> Vec<Diagnostic> {
    command::unknown(name)
}
