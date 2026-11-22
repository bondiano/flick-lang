//! The commands of the fixture plugin: one of its own, and `greet`, which
//! the fixture backend also adds.

use std::path::Path;
use std::str;

use flick_plugin::command::{
    self, Descriptor, Diagnostic, File, FlickModule, Invocation, Parameter, Plan, RunFlickStep,
    Span, Step, Style, TemplateDescriptor, TemplateRequest, ValueKind,
};

pub(super) fn commands() -> Vec<Descriptor> {
    let mut ds: Vec<Descriptor> = ["pairs-hello", "greet"]
        .into_iter()
        .map(|name| Descriptor {
            name: name.into(),
            summary: format!("the pairs plugin's `{name}`"),
            positionals: vec![Parameter {
                name: "who".into(),
                kind: ValueKind::Text,
                multiple: false,
                required: true,
                help: "whom to greet".into(),
            }],
            options: vec![],
            inputs: vec![],
            reads: vec![],
            writes: vec!["pairs.txt".into()],
            style: Style::Plan,
        })
        .collect();
    ds.push(Descriptor {
        name: "pairs-eval".into(),
        summary: "run an in-memory Flick expression through the selected backend".into(),
        positionals: vec![Parameter {
            name: "body".into(),
            kind: ValueKind::Text,
            multiple: false,
            required: true,
            help: "Flick body expression for Platform.stdout_write".into(),
        }],
        options: vec![Parameter {
            name: "out".into(),
            kind: ValueKind::Path,
            multiple: false,
            required: true,
            help: "output file path".into(),
        }],
        inputs: vec!["$sources".into()],
        reads: vec![],
        writes: vec!["$out".into()],
        style: Style::Plan,
    });
    ds
}

pub(super) fn run(invocation: &Invocation) -> Result<Plan, Vec<Diagnostic>> {
    if invocation.name == "pairs-eval" {
        return Ok(run_eval(invocation));
    }
    if !commands().iter().any(|d| d.name == invocation.name) {
        return Err(command::unknown(&invocation.name));
    }
    let who = (invocation.arguments.iter())
        .find(|a| a.name == "who")
        .and_then(|a| a.values.first())
        .map_or("nobody", String::as_str);
    let mut plan = Plan::empty();
    plan.files.push(File {
        path: "pairs.txt".into(),
        bytes: format!("pairs {} {who}\n", invocation.name).into_bytes(),
        executable: false,
    });
    plan.stdout = format!("pairs greets {who}\n").into_bytes();
    Ok(plan)
}

fn run_eval(invocation: &Invocation) -> Plan {
    let body = (invocation.arguments.iter())
        .find(|a| a.name == "body")
        .and_then(|a| a.values.first())
        .map_or("\"\"", String::as_str);
    let out = (invocation.arguments.iter())
        .find(|a| a.name == "out")
        .and_then(|a| a.values.first())
        .map_or("eval.txt", String::as_str);
    let has_main = invocation
        .inputs
        .iter()
        .any(|f| f.path.ends_with("App/Main.fl"));
    let import = if has_main { "import App.Main\n\n" } else { "" };
    let source = format!(
        "module PairsEvalGen exposing (main)\n\n{import}main : Unit ->{{Host.Console; AppError}} Unit\nlet main () =\n  {body}\n"
    );
    let origin = invocation
        .inputs
        .iter()
        .find(|f| {
            Path::new(&f.path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("fl"))
        })
        .and_then(|f| {
            let text = str::from_utf8(&f.bytes).ok()?;
            let end = text.floor_char_boundary(text.len().min(16));
            Some(Span {
                path: f.path.clone(),
                start: 0,
                end: u32::try_from(end).unwrap_or(0),
            })
        });
    let mut plan = Plan::empty();
    plan.steps.push(Step::RunFlick(RunFlickStep {
        id: "eval".into(),
        entry: "PairsEvalGen".into(),
        modules: vec![FlickModule {
            name: "PairsEvalGen".into(),
            source,
        }],
        arguments: vec![],
        output: Some("eval.raw".into()),
        origin,
    }));
    plan.resume = Some(out.as_bytes().to_vec());
    plan
}

pub(super) fn resume(state: &[u8], outputs: &[File]) -> Plan {
    let out = String::from_utf8_lossy(state).into_owned();
    let raw = outputs
        .iter()
        .find(|f| f.path == "eval.raw")
        .map_or(&[][..], |f| f.bytes.as_slice());
    let mut bytes = b"pairs-eval: ".to_vec();
    bytes.extend_from_slice(raw);
    let mut plan = Plan::empty();
    plan.files.push(File {
        path: out,
        bytes,
        executable: false,
    });
    plan.stdout = b"pairs-eval ok\n".to_vec();
    plan
}

pub(super) fn templates() -> Vec<TemplateDescriptor> {
    vec![TemplateDescriptor {
        name: "pair-app".into(),
        summary: "Minimal project from the pairs plugin".into(),
    }]
}

pub(super) fn scaffold(request: &TemplateRequest) -> Result<Plan, Vec<Diagnostic>> {
    if request.template != "pair-app" {
        return Err(command::unknown_template(&request.template, &templates()));
    }
    let name = &request.name;
    let compiler = &request.compiler;
    let init = request.init;
    let mut plan = Plan::empty();
    plan.files = vec![
        File {
            path: "flick.toml".into(),
            bytes: b"schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n"
                .to_vec(),
            executable: false,
        },
        File {
            path: "src/App/Main.fl".into(),
            bytes: format!(
                "module App.Main exposing (main)\n\nmain : Unit ->{{Host.Console; AppError}} Unit\nlet main () = Platform.stdout_line \"pairs template {name}\"\n"
            )
            .into_bytes(),
            executable: false,
        },
        File {
            path: "NOTES.md".into(),
            bytes: format!("pair-app {name} {compiler} init={init}\n").into_bytes(),
            executable: false,
        },
    ];
    plan.stdout = format!("Next: pairs template `{name}` ready\n").into_bytes();
    Ok(plan)
}
