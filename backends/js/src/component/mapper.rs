//! `content-mapper`: TypeScript's Content Mapper for `.fl` files, the
//! session `@flick-lang/content-mapper` starts. Each `transform` asks the
//! host for the TypeScript projection of the module, with the editor's
//! text, and answers with its text and span map. With `--declarations` it
//! writes `Name.d.fl.ts` beside each `.fl` file instead, for editors whose
//! TypeScript runs no Content Mapper, and exits.

use std::path::Path;

use flick_backend::command::{
    Answer, Descriptor, File, Handled, Invocation, Need, Opened, Parameter, Plan, Source, Style,
    TypescriptModule, ValueKind,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// `SpanMapKind` and `SpanMapFeature` of TypeScript's span maps.
const VERBATIM: u32 = 0;
const ATOM: u32 = 1;
/// Every feature but those that write text back: a type is not the text it
/// maps to, so a rename or a fix through it would garble the source.
const READ_ONLY: u32 = 1_048_575 & !(256 | 1024 | 2048 | 65_536 | 131_072);

pub(super) const NAME: &str = "content-mapper";

pub(super) fn descriptor() -> Descriptor {
    Descriptor {
        name: NAME.into(),
        summary: "TypeScript's Content Mapper for `.fl` files, on stdio".into(),
        positionals: vec![],
        options: vec![Parameter {
            name: "declarations".into(),
            kind: ValueKind::Path,
            multiple: true,
            required: false,
            help: "Write `Name.d.fl.ts` beside each `.fl` file under these paths, then exit".into(),
        }],
        inputs: vec!["$declarations".into()],
        reads: vec![],
        // a file's declaration goes beside it
        writes: vec!["$declarations".into(), "$declarations/..".into()],
        style: Style::Session,
    }
}

/// The modules `--declarations` asked for, in the order of their needs.
#[derive(Serialize, Deserialize)]
pub(super) struct Declarations {
    pub(super) declarations: Vec<String>,
}

pub(super) fn open(invocation: &Invocation) -> Opened {
    let roots: Vec<&String> = (invocation.arguments.iter())
        .filter(|a| a.name == "declarations")
        .flat_map(|a| &a.values)
        .collect();
    if roots.is_empty() {
        return Opened::Session(Vec::new());
    }
    // build output under a path holds no modules of the project
    let skipped = |path: &str| {
        roots.iter().any(|root| {
            let rest = if root.is_empty() {
                Some(path)
            } else {
                path.strip_prefix(root.as_str())
            };
            rest.is_some_and(|rest| rest.split('/').any(|p| ["dist", "target"].contains(&p)))
        })
    };
    let modules: Vec<String> = (invocation.inputs.iter())
        .map(|f| f.path.clone())
        .filter(|p| Path::new(p).extension().is_some_and(|e| e == "fl") && !skipped(p))
        .collect();
    let mut plan = Plan::empty();
    plan.needs = (modules.iter())
        .map(|path| {
            Need::Typescript(Source {
                path: path.clone(),
                text: None,
            })
        })
        .collect();
    let state = Declarations {
        declarations: modules,
    };
    plan.resume = Some(serde_json::to_vec(&state).expect("the state serializes"));
    Opened::Plan(plan)
}

/// `Name.d.fl.ts` beside each module the host projected.
pub(super) fn declare(state: &Declarations, answers: &[Answer]) -> Plan {
    let mut plan = Plan::empty();
    for (path, answer) in state.declarations.iter().zip(answers) {
        let Answer::Typescript(Some(module)) = answer else {
            continue;
        };
        let stem = path.strip_suffix(".fl").unwrap_or(path);
        plan.files.push(File {
            path: format!("{stem}.d.fl.ts"),
            bytes: module.text.clone().into_bytes(),
            executable: false,
        });
    }
    plan
}

fn reply(id: &Value, result: &Value) -> Vec<u8> {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
        .to_string()
        .into_bytes()
}

fn refuse(id: &Value, code: i64, message: &str) -> Vec<u8> {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
        .to_string()
        .into_bytes()
}

pub(super) fn handle(request: &[u8], answers: &[Answer]) -> Result<Handled, String> {
    let message: Value =
        serde_json::from_slice(request).map_err(|e| format!("not a JSON-RPC message: {e}"))?;
    let params = &message["params"];
    let answer = match message["method"].as_str().unwrap_or("") {
        "initialize" => initialize(params),
        "openProject" => Ok(json!({})),
        "closeProject" => Ok(Value::Null),
        "transform" => match answers.first() {
            Some(Answer::Typescript(module)) => Ok(transform(module.as_ref())),
            None => {
                let source = Source {
                    path: params["fileName"].as_str().unwrap_or("").to_owned(),
                    text: Some(params["content"].as_str().unwrap_or("").to_owned()),
                };
                return Ok(Handled {
                    state: Vec::new(),
                    response: None,
                    needs: vec![Need::Typescript(source)],
                });
            }
        },
        method => Err(format!("unknown method `{method}`")),
    };
    let response = message.get("id").map(|id| match answer {
        Ok(result) => reply(id, &result),
        Err(text) => refuse(id, -32601, &text),
    });
    Ok(Handled {
        state: Vec::new(),
        response,
        needs: vec![],
    })
}

/// Projections count UTF-8 bytes, which TypeScript offers.
fn initialize(params: &Value) -> Result<Value, String> {
    let offered = params["positionEncodings"]
        .as_array()
        .is_some_and(|e| e.iter().any(|e| e == "utf-8"));
    if !offered {
        return Err("flick content-mapper maps UTF-8 positions; TypeScript offered none".into());
    }
    Ok(json!({ "positionEncoding": "utf-8", "diagnosticSource": "flick" }))
}

/// The projection of a module as TypeScript's span map: a name that is the
/// same text on both sides maps verbatim, anything else as a read-only atom.
fn transform(module: Option<&TypescriptModule>) -> Value {
    let Some(module) = module else {
        // not a module of a project, as a file outside its source root
        return json!({ "text": "export {};\n", "extension": ".ts", "mappings": [] });
    };
    let slice = |text: &str, start: u32, end: u32| {
        text.get(start as usize..end as usize).map(str::to_owned)
    };
    let mappings: Vec<Value> = (module.spans.iter())
        .map(|s| {
            let same = slice(&module.text, s.ts_start, s.ts_end)
                == slice(&module.source, s.fl_start, s.fl_end);
            let (ts_len, fl_len) = (s.ts_end - s.ts_start, s.fl_end - s.fl_start);
            if s.exact && same {
                json!([s.ts_start, ts_len, s.fl_start, fl_len, VERBATIM])
            } else {
                json!([s.ts_start, ts_len, s.fl_start, fl_len, ATOM, READ_ONLY])
            }
        })
        .collect();
    json!({ "text": module.text, "extension": ".ts", "mappings": mappings })
}
