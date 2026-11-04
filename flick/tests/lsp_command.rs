//! `flick lsp --stdio` driven over its protocol: diagnostics like `flick
//! check`, hover, definition across modules and into built-in modules,
//! completion, rename, formatting, and a stdout of protocol only.

mod common;
#[path = "lsp_command/conduit.rs"]
mod conduit;
#[path = "lsp_command/isolation.rs"]
mod isolation;
#[path = "lsp_command/logging.rs"]
mod logging;
mod lsp_editor;

use std::fmt::Write as _;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use common::{flick_in, project};
use serde_json::{Value, json};

const LIB: &str = "schema-version = 1\n\n[project]\nkind = \"lib\"\n";

const DOMAIN: &str = "module App.Domain exposing (User, greet)

/// Someone to greet.
type alias User = { name : String; age : Float }

/// The greeting of a user.
greet : User -> String
let greet user = \"Привет, \" ++ user.name
";

const MAIN: &str = "module App.Main exposing (run)

import App.Domain exposing (User, greet)

run : User -> String
let run user =
  let count = List.length [user.age]
  \"𝄞π\" ++ greet user ++ show (count + \"x\")
";

struct Lsp {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next: u64,
}

impl Lsp {
    fn start(dir: &Path) -> Lsp {
        let mut child = Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(["lsp", "--stdio"])
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("flick lsp");
        let input = child.stdin.take().expect("stdin");
        let output = BufReader::new(child.stdout.take().expect("stdout"));
        let mut lsp = Lsp {
            child,
            input,
            output,
            next: 0,
        };
        lsp.request("initialize", &json!({ "capabilities": {} }));
        lsp.notify("initialized", &json!({}));
        lsp
    }

    fn send(&mut self, message: &Value) {
        let body = message.to_string();
        write!(self.input, "Content-Length: {}\r\n\r\n{body}", body.len()).expect("write");
        self.input.flush().expect("flush");
    }

    /// The next message; stdout holds nothing but framed JSON.
    fn receive(&mut self) -> Value {
        let mut length = 0;
        loop {
            let mut line = String::new();
            assert!(
                self.output.read_line(&mut line).expect("read") > 0,
                "the server ended"
            );
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            let n = line
                .strip_prefix("Content-Length: ")
                .unwrap_or_else(|| panic!("not a header: {line:?}"));
            length = n.parse().expect("a length");
        }
        let mut body = vec![0; length];
        self.output.read_exact(&mut body).expect("body");
        serde_json::from_slice(&body).expect("JSON")
    }

    fn notify(&mut self, method: &str, params: &Value) {
        self.send(&json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    fn request(&mut self, method: &str, params: &Value) -> Value {
        self.request_with_notifications(method, params).0
    }

    fn request_with_notifications(&mut self, method: &str, params: &Value) -> (Value, Vec<Value>) {
        self.next += 1;
        let id = self.next;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        let mut notifications = Vec::new();
        loop {
            let message = self.receive();
            if message["id"] == json!(id) {
                return (message, notifications);
            }
            notifications.push(message);
        }
    }

    /// A request is a barrier for all notifications sent before it.
    fn publications(&mut self) -> Vec<Value> {
        self.request_with_notifications("workspace/symbol", &json!({}))
            .1
            .into_iter()
            .filter(|m| m["method"] == "textDocument/publishDiagnostics")
            .map(|m| m["params"].clone())
            .collect()
    }

    /// The diagnostics published for `uri` next.
    fn diagnostics(&mut self, uri: &str) -> Vec<Value> {
        loop {
            let m = self.receive();
            if m["method"] == "textDocument/publishDiagnostics" && m["params"]["uri"] == uri {
                return m["params"]["diagnostics"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
            }
        }
    }

    fn close(mut self) {
        self.request("shutdown", &Value::Null);
        self.notify("exit", &Value::Null);
        assert_eq!(self.child.wait().expect("exit").code(), Some(0));
    }
}

fn uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

/// The LSP position of a byte offset: UTF-16 units within the line.
fn position(text: &str, offset: usize) -> Value {
    let line = text[..offset].matches('\n').count();
    let start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let character: usize = text[start..offset].chars().map(char::len_utf16).sum();
    json!({ "line": line, "character": character })
}

fn at(text: &str, needle: &str) -> Value {
    position(text, text.find(needle).expect("occurs") + 1)
}

#[test]
fn diagnostics_match_flick_check_in_utf16() {
    let dir = project(
        "lsp_diagnostics",
        &[
            ("flick.toml", LIB),
            ("src/App/Domain.fl", DOMAIN),
            ("src/App/Main.fl", MAIN),
        ],
    );
    let main = dir.join("src/App/Main.fl");
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&main), "languageId": "flick", "version": 1, "text": MAIN } }));
    let published = lsp.diagnostics(&uri(&main));
    let check = flick_in(&dir, &["check", "--diagnostics", "json"]);
    let expected: Vec<(String, Value)> = String::from_utf8_lossy(&check.stderr)
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).expect("NDJSON"))
        .filter(|e| e["path"].as_str().is_some_and(|p| p.ends_with("Main.fl")))
        .map(|e| {
            let (s, t) = (
                e["range"]["start"]
                    .as_u64()
                    .map(|n| usize::try_from(n).expect("small"))
                    .expect("start"),
                e["range"]["end"]
                    .as_u64()
                    .map(|n| usize::try_from(n).expect("small"))
                    .expect("end"),
            );
            (
                e["code"].as_str().expect("code").to_owned(),
                json!({ "start": position(MAIN, s), "end": position(MAIN, t) }),
            )
        })
        .collect();
    let actual: Vec<(String, Value)> = published
        .iter()
        .map(|d| {
            (
                d["code"].as_str().expect("code").to_owned(),
                d["range"].clone(),
            )
        })
        .collect();
    assert!(!expected.is_empty());
    assert_eq!(actual, expected);
    lsp.close();
}

#[test]
fn restoring_a_signature_clears_diagnostics_without_saving_or_restarting() {
    let full = "module App.MaybeT exposing (MaybeT(..), run)

type MaybeT (m : Type -> Type) a = MaybeT (m (Option a))

run : forall c m a. (MaybeT m a)^c -> (m (Option a))^c
let run (MaybeT action) = action
";
    let signature = "run : forall c m a. (MaybeT m a)^c -> (m (Option a))^c\n";
    let dir = project(
        "lsp_signature_recovery",
        &[("flick.toml", LIB), ("src/App/MaybeT.fl", full)],
    );
    let file = dir.join("src/App/MaybeT.fl");
    let file_uri = uri(&file);
    let mut lsp = Lsp::start(&dir);
    lsp.notify(
        "textDocument/didOpen",
        &json!({
            "textDocument": { "uri": file_uri, "languageId": "flick", "version": 1, "text": full }
        }),
    );
    assert!(lsp.diagnostics(&file_uri).is_empty());

    lsp.notify(
        "textDocument/didChange",
        &json!({
            "textDocument": { "uri": file_uri, "version": 2 },
            "contentChanges": [{ "text": full.replace(signature, "") }]
        }),
    );
    let broken = lsp.diagnostics(&file_uri);
    assert!(
        broken.iter().any(|d| d["code"] == "FLICK-T004"),
        "{broken:?}"
    );

    lsp.notify(
        "textDocument/didChange",
        &json!({
            "textDocument": { "uri": file_uri, "version": 3 },
            "contentChanges": [{ "text": full }]
        }),
    );
    assert!(lsp.diagnostics(&file_uri).is_empty());
    lsp.close();
}

#[test]
fn hover_definition_completion_rename_and_formatting() {
    let fixed = MAIN.replace("(count + \"x\")", "count");
    let dir = project(
        "lsp_features",
        &[
            ("flick.toml", LIB),
            ("src/App/Domain.fl", DOMAIN),
            ("src/App/Main.fl", &fixed),
        ],
    );
    let main = dir.join("src/App/Main.fl");
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&main), "languageId": "flick", "version": 1, "text": fixed } }));
    assert!(lsp.diagnostics(&uri(&main)).is_empty());
    let doc = json!({ "uri": uri(&main) });
    let hover = lsp.request(
        "textDocument/hover",
        &json!({ "textDocument": doc, "position": at(&fixed, "greet user") }),
    );
    assert_eq!(
        hover["result"]["contents"]["value"],
        "```flick\ngreet : User -> String\n```\n\nThe greeting of a user.\n\nfrom `App.Domain`"
    );
    let local = lsp.request(
        "textDocument/hover",
        &json!({ "textDocument": doc, "position": at(&fixed, "count\n") }),
    );
    assert!(
        local["result"]["contents"]["value"]
            .as_str()
            .expect("hover")
            .contains("count : Int"),
        "{local}"
    );

    let definition = lsp.request(
        "textDocument/definition",
        &json!({ "textDocument": doc, "position": at(&fixed, "greet user") }),
    );
    assert_eq!(
        definition["result"]["uri"],
        uri(&dir.join("src/App/Domain.fl"))
    );
    assert_eq!(
        definition["result"]["range"]["start"],
        json!({ "line": 6, "character": 0 })
    );

    // `user.` completes the fields of the record
    let typed = fixed.replace("[user.age]", "[user.]");
    lsp.notify("textDocument/didChange", &json!({ "textDocument": { "uri": uri(&main), "version": 2 }, "contentChanges": [{ "text": typed }] }));
    let completion = lsp.request("textDocument/completion", &json!({ "textDocument": doc, "position": position(&typed, typed.find("user.]").expect("dot") + 5) }));
    let labels: Vec<&str> = completion["result"]
        .as_array()
        .expect("items")
        .iter()
        .filter_map(|i| i["label"].as_str())
        .collect();
    assert!(
        labels.contains(&"age") && labels.contains(&"name"),
        "{labels:?}"
    );
    lsp.notify("textDocument/didChange", &json!({ "textDocument": { "uri": uri(&main), "version": 3 }, "contentChanges": [{ "text": fixed }] }));

    let rename = lsp.request(
        "textDocument/rename",
        &json!({ "textDocument": doc, "position": at(&fixed, "greet user"), "newName": "welcome" }),
    );
    let changes = rename["result"]["changes"].as_object().expect("changes");
    assert_eq!(changes.len(), 2, "both modules: {rename}");
    let refused = lsp.request(
        "textDocument/rename",
        &json!({ "textDocument": doc, "position": at(&fixed, "length"), "newName": "size" }),
    );
    assert!(refused["error"]["message"].is_string(), "{refused}");

    let messy = fixed.replace("let run user =", "let run   user =");
    lsp.notify("textDocument/didChange", &json!({ "textDocument": { "uri": uri(&main), "version": 4 }, "contentChanges": [{ "text": messy }] }));
    let format = lsp.request(
        "textDocument/formatting",
        &json!({ "textDocument": doc, "options": { "tabSize": 2, "insertSpaces": true } }),
    );
    assert_eq!(format["result"][0]["newText"], fixed);
    lsp.close();
}

#[test]
fn hover_and_completion_answer_quickly_on_a_2000_line_module() {
    let mut big =
        String::from("module App.Big exposing (run)\n\nrun : Float -> Float\nlet run x0 =\n");
    for i in 1..2000 {
        let _ = writeln!(big, "  let x{i} = x{} + {i}", i - 1);
    }
    big += "  x1999\n";
    // one function of 2000 lines, longer than `max-function-lines`
    let config = format!("{LIB}\n[lint]\nallow = [\"FLICK-T036\"]\n");
    let dir = project(
        "lsp_big",
        &[("flick.toml", &config), ("src/App/Big.fl", &big)],
    );
    let file = dir.join("src/App/Big.fl");
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": big } }));
    assert!(lsp.diagnostics(&uri(&file)).is_empty());
    let doc = json!({ "uri": uri(&file) });
    let started = Instant::now();
    let hover = lsp.request(
        "textDocument/hover",
        &json!({ "textDocument": doc, "position": at(&big, "x1999\n") }),
    );
    assert!(
        hover["result"]["contents"]["value"]
            .as_str()
            .expect("hover")
            .contains("x1999 : Float")
    );
    let completion = lsp.request(
        "textDocument/completion",
        &json!({ "textDocument": doc, "position": position(&big, big.len() - 1) }),
    );
    assert!(completion["result"].as_array().expect("items").len() > 1000);
    let elapsed = started.elapsed();
    assert!(elapsed < Duration::from_secs(5), "{elapsed:?}");
    lsp.close();
}

/// Copies the files under `from` into `to`.
fn copy_tree(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).expect("dir").flatten() {
        let path = entry.path();
        let target = to.join(entry.file_name());
        if path.is_dir() {
            copy_tree(&path, &target);
        } else {
            fs::create_dir_all(to).expect("dir");
            fs::copy(&path, &target).expect("copy");
        }
    }
}

#[test]
fn definition_reaches_into_installed_packages() {
    let source = "module Lib.Greeting exposing (greet)\n\n/// Says hello.\ngreet : String -> String\nlet greet name = \"Hello, \" ++ name\n";
    let lib = project(
        "lsp_package_lib",
        &[
            ("flick.toml", LIB),
            (
                "package.json",
                "{ \"name\": \"@acme/greeting\", \"version\": \"1.0.0\" }",
            ),
            ("src/Lib/Greeting.fl", source),
        ],
    );
    assert_eq!(flick_in(&lib, &["build"]).status.code(), Some(0));
    let main = "module App.Main exposing (run)\n\nimport Lib.Greeting exposing (greet)\n\nrun : String\nlet run = greet \"Ada\"\n";
    let app = project(
        "lsp_package_app",
        &[
            ("flick.toml", LIB),
            (
                "package.json",
                "{ \"name\": \"app\", \"dependencies\": { \"@acme/greeting\": \"1.0.0\" } }",
            ),
            ("src/App/Main.fl", main),
            (
                "node_modules/@acme/greeting/package.json",
                "{ \"name\": \"@acme/greeting\", \"version\": \"1.0.0\", \"flick\": { \"source-dir\": \"src\" } }",
            ),
        ],
    );
    let package = app.join("node_modules/@acme/greeting");
    copy_tree(&lib.join("src"), &package.join("src"));
    let file = app.join("src/App/Main.fl");
    let doc = json!({ "uri": uri(&file) });
    let go = |lsp: &mut Lsp| {
        lsp.request(
            "textDocument/definition",
            &json!({ "textDocument": doc, "position": at(main, "greet \"") }),
        )["result"]
            .clone()
    };
    let mut lsp = Lsp::start(&app);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": main } }));
    assert!(lsp.diagnostics(&uri(&file)).is_empty());
    let published = go(&mut lsp);
    assert_eq!(published["uri"], uri(&package.join("src/Lib/Greeting.fl")));
    assert_eq!(
        published["range"]["start"],
        json!({ "line": 3, "character": 0 })
    );
    lsp.close();
}
