//! `flick content-mapper` over its protocol, as TypeScript drives it: the
//! projection of the editor's text rather than the file's, names mapped
//! verbatim, and an exit once stdin ends.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use common::project;
use serde_json::{Value, json};

fn frame(message: &Value) -> String {
    let body = message.to_string();
    format!("Content-Length: {}\r\n\r\n{body}", body.len())
}

fn read(output: &mut impl BufRead) -> Value {
    let mut length = 0;
    loop {
        let mut line = String::new();
        output.read_line(&mut line).expect("header");
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(n) = line.strip_prefix("Content-Length: ") {
            length = n.parse().expect("length");
        }
    }
    let mut body = vec![0; length];
    output.read_exact(&mut body).expect("body");
    serde_json::from_slice(&body).expect("json")
}

#[test]
fn a_transform_projects_the_editors_text_and_maps_names_verbatim() {
    let saved =
        "module App.Main exposing (greet)\n\ngreet : String -> String\nlet greet name = name\n";
    let dir = project(
        "content_mapper_session",
        &[
            (
                "flick.toml",
                "schema-version = 1\n\n[project]\nkind = \"lib\"\nsource-dir = \"src\"\n",
            ),
            ("src/App/Main.fl", saved),
        ],
    );
    let file = dir.join("src/App/Main.fl");
    // unsaved: a second export the file on disk does not have
    let edited = format!(
        "{}\nshout : String -> String\nlet shout s = s ++ \"!\"\n",
        saved.replace("(greet)", "(greet, shout)")
    );
    let mut mapper = Command::new(env!("CARGO_BIN_EXE_flick"))
        .arg("content-mapper")
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("flick runs");
    let mut input = mapper.stdin.take().expect("stdin");
    let mut output = BufReader::new(mapper.stdout.take().expect("stdout"));
    let mut call = |id: &str, method: &str, params: Value| {
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        input.write_all(frame(&message).as_bytes()).expect("write");
        let reply = read(&mut output);
        assert_eq!(reply["id"], id, "{reply}");
        reply["result"].clone()
    };
    let init = call(
        "1",
        "initialize",
        json!({ "positionEncodings": ["utf-8", "utf-16"] }),
    );
    assert_eq!(init["positionEncoding"], "utf-8");
    let handle = json!("test:0");
    call(
        "2",
        "openProject",
        json!({ "configFileName": dir.join("tsconfig.json"), "projectHandle": handle }),
    );
    let result = call(
        "3",
        "transform",
        json!({ "fileName": file, "content": edited, "projectHandle": handle }),
    );
    let text = result["text"].as_str().expect("text");
    assert!(
        text.contains("export declare const shout: (arg: string) => string;"),
        "{text}"
    );
    assert_eq!(result["extension"], ".ts");
    let verbatim: Vec<&Value> = result["mappings"]
        .as_array()
        .expect("mappings")
        .iter()
        .filter(|m| m[4] == 0)
        .collect();
    let name = |m: &Value, from: &str, at: usize| {
        let offset = |v: &Value| usize::try_from(v.as_u64().expect("offset")).expect("small");
        let (start, len) = (offset(&m[at]), offset(&m[at + 1]));
        from[start..start + len].to_owned()
    };
    for m in &verbatim {
        assert_eq!(name(m, text, 0), name(m, &edited, 2), "{m}");
    }
    assert!(
        verbatim.iter().any(|m| name(m, &edited, 2) == "shout"),
        "{verbatim:?}"
    );
    call("4", "closeProject", json!({ "projectHandle": handle }));
    drop(input);
    assert!(mapper.wait().expect("exits").success());
    // the built-ins it refers to are real files TypeScript can resolve
    assert!(dir.join(".flick/projections/Prelude.flick.ts").is_file());
}
