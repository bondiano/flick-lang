//! End-to-end tests for backend-declared foreign adapter words and type-form
//! attributes (#664): official component descriptor sync, Go `@ts` gating,
//! custom `test/literal` `slot` + `@lit` validation in `flick check`,
//! `flick build`, and `flick lsp --stdio`.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use flick_backend::host::{Budget, WasmBackend, runtime};
use flick_backend::{Backend as _, BackendId, BackendPin};
use flick_core::project::official_backends;
use serde_json::{Value, json};

use super::common::{flick_in, project};
use super::pinned;

fn fixture_backend_bytes() -> Vec<u8> {
    let component = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_backend.wasm");
    fs::read(component).expect("just wasm-backends builds flick_test_backend.wasm")
}

fn literal_project_config(entry: &str, sha256: &str) -> String {
    format!(
        "{}\n[target.literal]\ncase = \"foreign\"\n",
        pinned()
            .replace("entry = \"App\"", &format!("entry = \"{entry}\""))
            .replace(&"a".repeat(64), sha256)
    )
}

#[test]
fn official_wasm_components_match_core_official_backends() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let engine = runtime::engine(None).expect("wasmtime engine");
    let core_backends = official_backends();

    for (id, dir) in [("flick/js", "js"), ("flick/go", "go")] {
        let bytes = fs::read(root.join(format!("backends/{dir}/package/component.wasm")))
            .expect("distributed component");
        let pin = BackendPin {
            identity: BackendId {
                id: id.into(),
                version: "0.1.0".into(),
            },
            sha256: flick_backend::sha256(&bytes),
        };
        let backend =
            WasmBackend::load(&engine, pin, &bytes, Budget::default()).expect("load component");
        let desc = backend.describe();
        assert_eq!(flick_backend::validate_tags(&desc), Ok(()), "{id}");
        assert_eq!(
            flick_backend::validate_foreign_metadata(&desc),
            Ok(()),
            "{id}"
        );
        let core = core_backends
            .iter()
            .find(|b| b.id == id)
            .expect("official backend in core");
        let words: Vec<&str> = desc.adapter_words.iter().map(|w| w.name.as_str()).collect();
        let core_words: Vec<&str> = core
            .foreign_meta
            .adapter_words
            .iter()
            .map(|w| w.name.as_str())
            .collect();
        assert_eq!(words, core_words, "{id} adapter words");
        let attrs: Vec<&str> = desc
            .type_form_attributes
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        let core_attrs: Vec<&str> = core
            .foreign_meta
            .type_form_attributes
            .iter()
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(attrs, core_attrs, "{id} type-form attributes");
    }
}

#[test]
fn go_project_rejects_ts_attribute_outside_when_js_and_validates_owned() {
    let root = project(
        "foreign_meta_go_ts_and_owned",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"lib\"\n[build]\ntarget = \"go\"\n",
            ),
            (
                "src/JsOnly.fl",
                "@when \"js\"\nmodule JsOnly exposing (JsHandle)\n\
                 @ts \"Date\"\ntype JsHandle\n",
            ),
            (
                "src/Good.fl",
                concat!(
                    "module Good exposing (consume)\n",
                    "consume : String ->{} Unit\n",
                    "foreign consume from \"./good.mjs\" \"consume\"\n",
                    "  arity 1\n",
                    "  owned 0\n",
                ),
            ),
        ],
    );
    let ok = flick_in(&root, &["check"]);
    assert!(
        ok.status.success(),
        "valid Go owned + @when js @ts should pass: {}",
        String::from_utf8_lossy(&ok.stderr)
    );

    // `@ts` on a non-JS module in a Go project must fail with FLICK-S014.
    fs::write(
        root.join("src/BadAttr.fl"),
        "module BadAttr exposing (Bad)\n@ts \"number\"\ntype Bad\n",
    )
    .expect("write BadAttr");
    let bad_attr = flick_in(&root, &["check", "--diagnostics", "json"]);
    assert!(!bad_attr.status.success());
    let err = String::from_utf8_lossy(&bad_attr.stderr);
    assert!(err.contains("FLICK-S014"), "{err}");
    assert!(err.contains("@ts"), "{err}");
    fs::remove_file(root.join("src/BadAttr.fl")).expect("remove BadAttr");

    // Out-of-bounds `owned 5` must fail with FLICK-F001 at the exact span of `owned 5`.
    let bad_owned_src = concat!(
        "module BadOwned exposing (take)\n",
        "take : String ->{} Unit\n",
        "foreign take from \"./good.mjs\" \"take\"\n",
        "  arity 1\n",
        "  owned 5\n",
    );
    fs::write(root.join("src/BadOwned.fl"), bad_owned_src).expect("write BadOwned");
    let bad_owned = flick_in(&root, &["check", "--diagnostics", "json"]);
    assert!(!bad_owned.status.success());
    let diags: Vec<Value> = String::from_utf8(bad_owned.stderr)
        .expect("utf8")
        .lines()
        .map(|l| serde_json::from_str(l).expect("json diag"))
        .collect();
    let d = diags
        .iter()
        .find(|d| d["code"] == "FLICK-F001")
        .expect("FLICK-F001");
    let start = usize::try_from(d["range"]["start"].as_u64().expect("start")).expect("fits");
    let end = usize::try_from(d["range"]["end"].as_u64().expect("end")).expect("fits");
    assert_eq!(&bad_owned_src[start..end], "owned 5");
}

#[test]
#[ignore = "requires just wasm-backends"]
#[allow(clippy::too_many_lines)]
fn literal_backend_validates_custom_slot_word_and_lit_attribute_with_exact_spans() {
    let bytes = fixture_backend_bytes();
    let sha = flick_backend::sha256(&bytes);
    let config = literal_project_config("Fixture.Foreign", &sha);
    let valid_source = concat!(
        "module Fixture.Foreign exposing (main, call_two, Handle)\n",
        "// π keeps UTF-8 byte offsets non-trivial\n",
        "@lit \"i32\"\n",
        "type Handle\n\n",
        "call_two : Handle -> String ->{} Unit\n",
        "foreign call_two from \"./lit.js\" \"callTwo\"\n",
        "  arity 2\n",
        "  owned 0\n",
        "  slot 0\n",
        "  slot 1\n\n",
        "main : Unit -> Unit\n",
        "let main () = ()\n",
    );

    let root = project(
        "backend_foreign_meta_literal",
        &[
            ("flick.toml", &config),
            ("src/Fixture/Foreign.fl", valid_source),
        ],
    );
    fs::write(root.join("backend.wasm"), &bytes).expect("component");

    // 1. Valid custom word `slot 0` + `slot 1`, core `owned 0`, and `@lit "i32"` pass `check` and `build`.
    let checked = flick_in(&root, &["check"]);
    assert!(
        checked.status.success(),
        "flick check failed on valid custom adapter word: {}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let built = flick_in(&root, &["build", "--offline"]);
    assert!(
        built.status.success(),
        "flick build failed on valid custom adapter word: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    let report: Value = serde_json::from_str(
        &fs::read_to_string(root.join("dist/foreign-report.json")).expect("foreign-report.json"),
    )
    .expect("valid json");
    assert_eq!(report["ir_version"], "0.15.0");
    let foreigns = report["foreigns"].as_array().expect("foreigns");
    assert_eq!(foreigns.len(), 1, "{report}");
    assert_eq!(foreigns[0]["name"], "call_two");
    assert_eq!(foreigns[0]["owned"], json!([0]));
    let words = foreigns[0]["words"].as_array().expect("words");
    assert_eq!(words.len(), 2, "{report}");
    assert_eq!(words[0]["name"], "slot");
    assert_eq!(words[0]["args"], json!([{ "index": 0 }]));
    assert_eq!(words[1]["name"], "slot");
    assert_eq!(words[1]["args"], json!([{ "index": 1 }]));

    // 2. Semantic misuse `slot 5` (out of range for arity 2) is rejected by the backend's
    // `check-target` at the exact UTF-8 byte span of `slot 5` in BOTH `check` and `build`.
    let bad_slot = valid_source.replace("  slot 1\n", "  slot 5\n");
    fs::write(root.join("src/Fixture/Foreign.fl"), &bad_slot).expect("write bad slot");
    for args in [
        &["check", "--diagnostics", "json"][..],
        &["build", "--offline", "--diagnostics", "json"][..],
    ] {
        let out = flick_in(&root, args);
        assert!(!out.status.success(), "{args:?} unexpectedly succeeded");
        let stderr = String::from_utf8(out.stderr).expect("utf8");
        let diags: Vec<Value> = stderr
            .lines()
            .map(|l| serde_json::from_str(l).expect("json diag"))
            .collect();
        let d = diags
            .iter()
            .find(|d| d["code"] == "FLICK-F001")
            .unwrap_or_else(|| panic!("{args:?}: expected FLICK-F001 in {stderr}"));
        assert_eq!(d["path"], "src/Fixture/Foreign.fl");
        let start = usize::try_from(d["range"]["start"].as_u64().expect("start")).expect("fits");
        let end = usize::try_from(d["range"]["end"].as_u64().expect("end")).expect("fits");
        assert_eq!(&bad_slot[start..end], "slot 5", "{args:?}: {d}");
        assert!(
            d["message"]
                .as_str()
                .expect("message")
                .contains("out of range for arity 2"),
            "{args:?}: {d}"
        );
        assert!(
            d["message"]
                .as_str()
                .expect("message")
                .contains("test/literal@1.0.0#"),
            "{args:?}: {d}"
        );
    }

    // 3. Wrong argument shape `slot nope` is rejected with FLICK-F001 at exact span.
    let bad_arg = valid_source.replace("  slot 1\n", "  slot nope\n");
    fs::write(root.join("src/Fixture/Foreign.fl"), &bad_arg).expect("write bad arg");
    let out = flick_in(&root, &["check", "--diagnostics", "json"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8(out.stderr).expect("utf8");
    let diags: Vec<Value> = stderr
        .lines()
        .map(|l| serde_json::from_str(l).expect("json diag"))
        .collect();
    let d = diags
        .iter()
        .find(|d| d["code"] == "FLICK-F001")
        .unwrap_or_else(|| panic!("expected FLICK-F001 in {stderr}"));
    let start = usize::try_from(d["range"]["start"].as_u64().expect("start")).expect("fits");
    let end = usize::try_from(d["range"]["end"].as_u64().expect("end")).expect("fits");
    assert_eq!(&bad_arg[start..end], "slot nope");

    // 4. JS-only word `this` in a `test/literal` project fails with FLICK-S011 listing `slot` (not `this`).
    let js_word = valid_source.replace("  slot 1\n", "  this\n");
    fs::write(root.join("src/Fixture/Foreign.fl"), &js_word).expect("write js word");
    let out = flick_in(&root, &["check", "--diagnostics", "json"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8(out.stderr).expect("utf8");
    let diags: Vec<Value> = stderr
        .lines()
        .map(|l| serde_json::from_str(l).expect("json diag"))
        .collect();
    let d = diags
        .iter()
        .find(|d| d["code"] == "FLICK-S011")
        .unwrap_or_else(|| panic!("expected FLICK-S011 in {stderr}"));
    let start = usize::try_from(d["range"]["start"].as_u64().expect("start")).expect("fits");
    let end = usize::try_from(d["range"]["end"].as_u64().expect("end")).expect("fits");
    assert_eq!(&js_word[start..end], "this");
    let msg = d["message"].as_str().expect("message");
    assert!(msg.contains("slot"), "{msg}");
    assert!(
        !msg.contains(
            "expected one of: arity, catch, retention, nullable, optional, reviewed, owned, this"
        ),
        "{msg}"
    );

    // 5. JS-only `@ts` in a `test/literal` project fails with FLICK-S014.
    let js_attr = valid_source.replace("@lit \"i32\"", "@ts \"number\"");
    fs::write(root.join("src/Fixture/Foreign.fl"), &js_attr).expect("write @ts");
    let out = flick_in(&root, &["check", "--diagnostics", "json"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("FLICK-S014"), "{stderr}");
}

struct Lsp {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next: u64,
}

impl Lsp {
    fn start(dir: &Path) -> Self {
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
        let mut lsp = Self {
            child,
            input,
            output,
            next: 0,
        };
        let _ = lsp.request("initialize", &json!({ "capabilities": {} }));
        lsp.notify("initialized", &json!({}));
        lsp
    }

    fn send(&mut self, msg: &Value) {
        let body = msg.to_string();
        write!(self.input, "Content-Length: {}\r\n\r\n{body}", body.len()).expect("write");
        self.input.flush().expect("flush");
    }

    fn receive(&mut self) -> Value {
        let mut length = 0usize;
        loop {
            let mut line = String::new();
            assert!(
                self.output.read_line(&mut line).expect("read") > 0,
                "server ended"
            );
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some(n) = line.strip_prefix("Content-Length: ") {
                length = n.parse().expect("length");
            }
        }
        let mut body = vec![0; length];
        self.output.read_exact(&mut body).expect("body");
        serde_json::from_slice(&body).expect("json")
    }

    fn notify(&mut self, method: &str, params: &Value) {
        self.send(&json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    fn request(&mut self, method: &str, params: &Value) -> Value {
        self.next += 1;
        let id = self.next;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let msg = self.receive();
            if msg.get("id") == Some(&json!(id)) {
                return msg["result"].clone();
            }
        }
    }

    fn next_diagnostics(&mut self, uri: &str) -> Vec<Value> {
        loop {
            let msg = self.receive();
            if msg["method"] == "textDocument/publishDiagnostics" && msg["params"]["uri"] == uri {
                return msg["params"]["diagnostics"]
                    .as_array()
                    .expect("diagnostics")
                    .clone();
            }
        }
    }

    fn stop(mut self) {
        let _ = self.request("shutdown", &json!(null));
        self.notify("exit", &json!(null));
        let _ = self.child.wait();
    }
}

fn file_uri(path: &Path) -> String {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let s = abs.to_string_lossy().replace('\\', "/");
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        format!("file:///{s}")
    }
}

#[test]
#[ignore = "requires just wasm-backends"]
#[allow(clippy::too_many_lines)]
fn lsp_completion_hover_and_check_target_follow_literal_backend() {
    let bytes = fixture_backend_bytes();
    let sha = flick_backend::sha256(&bytes);
    let config = literal_project_config("Fixture.Foreign", &sha);
    let bad_source = concat!(
        "module Fixture.Foreign exposing (main, call_two, Handle)\n",
        "@lit \"i32\"\n",
        "type Handle\n",
        "call_two : Handle -> String ->{} Unit\n",
        "foreign call_two from \"./lit.js\" \"callTwo\"\n",
        "  arity 2\n",
        "  slot 5\n",
        "main : Unit -> Unit\n",
        "let main () = ()\n",
    );

    let root = project(
        "backend_foreign_meta_lsp",
        &[
            ("flick.toml", &config),
            ("src/Fixture/Foreign.fl", bad_source),
        ],
    );
    fs::write(root.join("backend.wasm"), &bytes).expect("component");

    let file_path = root.join("src/Fixture/Foreign.fl");
    let uri = file_uri(&file_path);
    let mut lsp = Lsp::start(&root);

    // 1. Opening with `slot 5` publishes backend `FLICK-F001` at line 6, cols 2..8 (`slot 5`).
    lsp.notify(
        "textDocument/didOpen",
        &json!({
            "textDocument": {
                "uri": uri,
                "languageId": "flick",
                "version": 1,
                "text": bad_source,
            }
        }),
    );
    let diags = lsp.next_diagnostics(&uri);
    let f001 = diags
        .iter()
        .find(|d| d["code"] == "FLICK-F001")
        .unwrap_or_else(|| panic!("expected FLICK-F001 from LSP check-target: {diags:?}"));
    assert_eq!(f001["range"]["start"]["line"], 6, "{f001}");
    assert_eq!(f001["range"]["start"]["character"], 2, "{f001}");
    assert_eq!(f001["range"]["end"]["line"], 6, "{f001}");
    assert_eq!(f001["range"]["end"]["character"], 8, "{f001}");

    // 2. Fixing to `slot 0` and adding a blank indented line below it clears diagnostics
    // and lets completion offer ONLY core words + `slot` (never `this`/`cancel`/`variadic`).
    let edit_source = concat!(
        "module Fixture.Foreign exposing (main, call_two, Handle)\n",
        "@lit \"i32\"\n",
        "type Handle\n",
        "call_two : Handle -> String ->{} Unit\n",
        "foreign call_two from \"./lit.js\" \"callTwo\"\n",
        "  arity 2\n",
        "  slot 0\n",
        "  \n",
        "main : Unit -> Unit\n",
        "let main () = ()\n",
    );
    lsp.notify(
        "textDocument/didChange",
        &json!({
            "textDocument": { "uri": uri, "version": 2 },
            "contentChanges": [{ "text": edit_source }],
        }),
    );
    let cleared = lsp.next_diagnostics(&uri);
    assert!(
        cleared.is_empty(),
        "expected clean diagnostics: {cleared:?}"
    );

    let completions = lsp.request(
        "textDocument/completion",
        &json!({
            "textDocument": { "uri": uri },
            "position": { "line": 7, "character": 2 },
        }),
    );
    let labels: Vec<&str> = completions
        .as_array()
        .expect("completion array")
        .iter()
        .filter_map(|item| item["label"].as_str())
        .collect();
    assert!(labels.contains(&"slot"), "{labels:?}");
    assert!(labels.contains(&"owned"), "{labels:?}");
    assert!(labels.contains(&"arity"), "{labels:?}");
    assert!(!labels.contains(&"this"), "{labels:?}");
    assert!(!labels.contains(&"cancel"), "{labels:?}");
    assert!(!labels.contains(&"variadic"), "{labels:?}");

    // 3. Hover on `slot` returns the backend's declared help text.
    let hover = lsp.request(
        "textDocument/hover",
        &json!({
            "textDocument": { "uri": uri },
            "position": { "line": 6, "character": 3 },
        }),
    );
    let hover_text = hover["contents"]["value"].as_str().expect("hover markdown");
    assert!(
        hover_text.contains("literal target calling slot"),
        "{hover_text}"
    );

    lsp.stop();
}
