//! The commands of the fixture backend: one of each kind the host checks,
//! commands that read inputs, link a step's reads and resume after it, and
//! a session that echoes, traps or spins on request.

mod deps;

use std::fmt::Write as _;
use std::hint;
use std::path::Path;

use flick_backend::command::{
    self, Descriptor, Diagnostic, File, Handled, Invocation, Parameter, Plan, Severity, Span, Step,
    Style, TemplateDescriptor, TemplateRequest, Tool, ToolStep, ValueKind,
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
    let mut all = vec![
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
        Descriptor {
            name: "literal-in-place".into(),
            summary: "tests in-place tool step execution and validation".into(),
            positionals: vec![],
            options: vec![Parameter {
                name: "mode".into(),
                kind: ValueKind::Text,
                multiple: false,
                required: false,
                help: "test mode".into(),
            }],
            inputs: vec![],
            reads: vec![],
            writes: vec!["staged.txt".into(), "after.txt".into()],
            style: Style::Plan,
        },
        Descriptor {
            name: "doctor".into(),
            summary: "reports literal backend readiness for `flick doctor`".into(),
            positionals: vec![],
            options: vec![
                Parameter {
                    name: "in-project".into(),
                    kind: ValueKind::Text,
                    multiple: false,
                    required: false,
                    help: "whether inside a project".into(),
                },
                Parameter {
                    name: "compiler".into(),
                    kind: ValueKind::Text,
                    multiple: false,
                    required: false,
                    help: "compiler version".into(),
                },
                Parameter {
                    name: "granted-tools".into(),
                    kind: ValueKind::Text,
                    multiple: true,
                    required: false,
                    help: "granted tools".into(),
                },
            ],
            inputs: vec!["doctor-write.txt".into()],
            reads: vec![],
            writes: vec![],
            style: Style::Plan,
        },
    ];
    all.extend(deps::commands());
    all
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
        in_place: false,
    }))
}

fn value<'a>(invocation: &'a Invocation, name: &str) -> Option<&'a str> {
    (invocation.arguments.iter())
        .find(|a| a.name == name)
        .and_then(|a| a.values.first())
        .map(String::as_str)
}

fn values<'a>(invocation: &'a Invocation, name: &str) -> &'a [String] {
    (invocation.arguments.iter())
        .find(|a| a.name == name)
        .map_or(&[], |a| a.values.as_slice())
}

pub(super) fn run(invocation: &Invocation) -> Result<Plan, Vec<Diagnostic>> {
    if matches!(
        invocation.name.as_str(),
        "install" | "add" | "remove" | "update" | "deps"
    ) {
        return deps::run(invocation);
    }
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
            in_place: false,
        })),
        "literal-in-place" => run_in_place(invocation, &mut plan),
        "doctor" => run_doctor(invocation, &mut plan),
        _ => {}
    }
    Ok(plan)
}

fn run_in_place(invocation: &Invocation, plan: &mut Plan) {
    let mode = value(invocation, "mode").unwrap_or("ok");
    let node = invocation
        .tools
        .iter()
        .find(|t| t.id == "node")
        .cloned()
        .unwrap_or(Tool {
            id: "node".into(),
            version: String::new(),
            sha256: String::new(),
        });
    plan.files.push(file("staged.txt", "staged-content\n"));
    let (inputs, reads, outputs) = match mode {
        "bad-inputs" => (vec!["staged.txt".into()], vec![], vec!["after.txt".into()]),
        "bad-reads" => (vec![], vec!["staged.txt".into()], vec!["after.txt".into()]),
        "bad-outputs" => (vec![], vec![], vec!["undeclared.txt".into()]),
        _ => (vec![], vec![], vec!["after.txt".into()]),
    };
    plan.steps.push(Step::Tool(ToolStep {
        id: "in-place".into(),
        tool: node,
        arguments: vec![
            "-e".into(),
            "const fs = require('fs'); fs.writeFileSync('after.txt', 'saw:' + fs.readFileSync('staged.txt', 'utf8'));"
                .into(),
        ],
        environment: vec![],
        inputs,
        reads,
        outputs,
        in_place: true,
    }));
}

fn run_doctor(invocation: &Invocation, plan: &mut Plan) {
    if invocation
        .inputs
        .iter()
        .any(|f| f.path == "doctor-write.txt")
    {
        plan.files
            .push(file("doctor-side-effect.txt", "forbidden\n"));
        return;
    }
    let detail = match invocation
        .tools
        .iter()
        .find(|t| t.id == "node" && !t.sha256.is_empty() && !t.version.is_empty())
    {
        Some(t) => format!("ready (runtime {})", t.version),
        None => "ready (no runtime on PATH)".into(),
    };
    plan.diagnostics.push(Diagnostic {
        code: "doctor-ok".into(),
        severity: Severity::Note,
        message: format!("literal-backend\n{detail}"),
        notes: vec![],
        span: None,
        help: None,
    });
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
        needs: vec![],
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

pub(super) fn templates() -> Vec<TemplateDescriptor> {
    vec![
        TemplateDescriptor {
            name: "starter".into(),
            summary: "Minimal literal starter project".into(),
        },
        TemplateDescriptor {
            name: "escape".into(),
            summary: "Fixture template that escapes the target directory".into(),
        },
        TemplateDescriptor {
            name: "stepped".into(),
            summary: "Fixture template that returns a disallowed tool step".into(),
        },
        TemplateDescriptor {
            name: "bad-lock".into(),
            summary: "Fixture template that writes a malformed flick.lock".into(),
        },
    ]
}

pub(super) fn scaffold(request: &TemplateRequest) -> Result<Plan, Vec<Diagnostic>> {
    let mut plan = Plan::empty();
    match request.template.as_str() {
        "starter" => {
            let name = &request.name;
            let compiler = &request.compiler;
            let init = request.init;
            plan.files = vec![
                file(
                    "flick.toml",
                    "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n",
                ),
                file(
                    "src/App/Main.fl",
                    &format!(
                        "module App.Main exposing (main)\n\nmain : Unit ->{{Host.Console; AppError}} Unit\nlet main () = Platform.stdout_line \"hello from {name}\"\n"
                    ),
                ),
                file(
                    "README.md",
                    &format!("# {name}\ncompiler: {compiler}\ninit: {init}\n"),
                ),
                file("AGENTS.md", "# Literal template agents\n"),
            ];
            plan.stdout = format!("Next: literal starter `{name}` ready\n").into_bytes();
            Ok(plan)
        }
        "escape" => {
            plan.files.push(file("../outside.txt", "escaped\n"));
            Ok(plan)
        }
        "stepped" => {
            plan.files.push(file("flick.toml", "schema-version = 1\n"));
            plan.steps.push(Step::Tool(ToolStep {
                id: "forbidden".into(),
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
                in_place: false,
            }));
            Ok(plan)
        }
        "bad-lock" => {
            plan.files = vec![
                file(
                    "flick.toml",
                    "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n",
                ),
                file("flick.lock", "version = \"broken\"\n"),
            ];
            Ok(plan)
        }
        other => Err(command::unknown_template(other, &templates())),
    }
}
