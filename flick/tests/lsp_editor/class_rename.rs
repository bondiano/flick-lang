use std::collections::BTreeMap;
use std::path::PathBuf;

use super::*;

const WORKSPACE: &str = "schema-version = 1\n[workspace]\nmembers = [\"library\", \"client\"]\n";
const CLIENT: &str = "schema-version = 1\n[project]\nkind = \"lib\"\n[dependencies]\nlibrary = { path = \"../library\" }\n";

const LABEL: &str = "module Lib.Label exposing (Label, render_item)
class Label a =
  render_item : a -> String
  render_item : Show a => a -> String
  let render_item x = show x
instance Label Int
";

const FACADE: &str = "module Lib.Facade exposing (Label, render_item)
import Lib.Label exposing (Label, render_item)
";

const OTHER: &str = "module App.Other exposing (Other, render_item)
class Other a =
  render_item : a -> String
instance Other Bool =
  let render_item _ = \"other\"
";

const MAIN: &str = "module App.Main exposing (..)
import Lib.Facade as L exposing (Label, render_item)
import App.Other as O
type Local = Local String
instance Label Local =
  render_item : Local -> String
  let render_item (Local x) = x
generic : Label a => a -> String
let generic x = render_item x
let concrete = \"ёж 🦔\" ++ L.render_item 1
let local_concrete = render_item (Local \"value\")
let other = O.render_item true
local : (Int -> String) -> Int -> String
let local render_item = render_item
let record = { render_item = \"field\" }
let field = record.render_item
";

fn workspace(name: &str, main: &str) -> PathBuf {
    project(
        name,
        &[
            ("flick.toml", WORKSPACE),
            ("library/flick.toml", LIB),
            ("library/src/Lib/Label.fl", LABEL),
            ("library/src/Lib/Facade.fl", FACADE),
            ("client/flick.toml", CLIENT),
            ("client/src/App/Other.fl", OTHER),
            ("client/src/App/Main.fl", main),
        ],
    )
}

fn open(lsp: &mut Lsp, file: &Path, text: &str) {
    lsp.notify(
        "textDocument/didOpen",
        &json!({ "textDocument": { "uri": uri(file), "languageId": "flick", "version": 1, "text": text } }),
    );
}

fn rename(lsp: &mut Lsp, file: &Path, text: &str, needle: &str, name: &str) -> Value {
    lsp.request(
        "textDocument/rename",
        &json!({ "textDocument": { "uri": uri(file) }, "position": at(text, needle), "newName": name }),
    )
}

/// The independent expected ranges: the first `count` method spellings in
/// these fixtures precede all deliberately unrelated names.
fn expected_edits(text: &str, old: &str, new: &str, count: usize) -> Value {
    let edits: Vec<_> = text
        .match_indices(old)
        .take(count)
        .map(|(start, _)| {
            json!({
                "range": { "start": position(text, start), "end": position(text, start + old.len()) },
                "newText": new,
            })
        })
        .collect();
    assert_eq!(edits.len(), count);
    json!(edits)
}

fn assert_changes(response: &Value, files: &[(&Path, &str, usize)], old: &str, new: &str) {
    let expected: serde_json::Map<String, Value> = files
        .iter()
        .map(|&(path, text, count)| (uri(path), expected_edits(text, old, new, count)))
        .collect();
    assert_eq!(response["result"]["changes"], json!(expected), "{response}");
}

/// Convert returned UTF-16 positions independently of the server's mapper.
fn byte_offset(text: &str, at: &Value) -> usize {
    let line = usize::try_from(at["line"].as_u64().expect("line")).expect("small line");
    let character =
        usize::try_from(at["character"].as_u64().expect("character")).expect("small character");
    let start: usize = text.split_inclusive('\n').take(line).map(str::len).sum();
    let mut units = 0;
    for (byte, c) in text[start..].char_indices() {
        if units == character {
            return start + byte;
        }
        assert_ne!(c, '\n', "position beyond its line");
        units += c.len_utf16();
    }
    assert_eq!(units, character);
    text.len()
}

fn apply_edits(text: &str, edits: &Value, old: &str) -> String {
    let mut edits: Vec<_> = edits
        .as_array()
        .expect("edits")
        .iter()
        .map(|edit| {
            let start = byte_offset(text, &edit["range"]["start"]);
            let end = byte_offset(text, &edit["range"]["end"]);
            assert_eq!(&text[start..end], old);
            (start, end, edit["newText"].as_str().expect("replacement"))
        })
        .collect();
    edits.sort_by_key(|&(start, _, _)| start);
    assert!(edits.windows(2).all(|pair| pair[0].1 <= pair[1].0));
    let mut result = text.to_owned();
    for (start, end, new) in edits.into_iter().rev() {
        result.replace_range(start..end, new);
    }
    result
}

fn assert_latest_diagnostics_clean(lsp: &mut Lsp) {
    // Earlier publications may describe the intermediate state while the
    // client applies a multi-file workspace edit one document at a time.
    let mut latest = BTreeMap::new();
    for publication in lsp.publications() {
        latest.insert(
            publication["uri"].to_string(),
            publication["diagnostics"].clone(),
        );
    }
    assert!(!latest.is_empty(), "expected fresh diagnostics");
    for (file, diagnostics) in latest {
        assert!(
            diagnostics
                .as_array()
                .expect("diagnostics")
                .iter()
                .all(|d| d["severity"] != 1),
            "{file}: {diagnostics}"
        );
    }
}

#[test]
fn class_rename_reaches_workspace_instances_and_calls_and_requeries_unsaved_utf16_edits() {
    let dir = workspace("lsp_class_rename_workspace", MAIN);
    let label = dir.join("library/src/Lib/Label.fl");
    let facade = dir.join("library/src/Lib/Facade.fl");
    let main = dir.join("client/src/App/Main.fl");
    let files = [
        (label.as_path(), LABEL, 4),
        (facade.as_path(), FACADE, 2),
        (main.as_path(), MAIN, 6),
    ];
    let mut lsp = Lsp::start(&dir.join("library"));
    for &(file, text, _) in &files {
        open(&mut lsp, file, text);
    }
    assert_latest_diagnostics_clean(&mut lsp);

    let mut edits = Value::Null;
    for (file, text, needle) in [
        (label.as_path(), LABEL, "render_item :"),
        (main.as_path(), MAIN, "render_item :"),
        (main.as_path(), MAIN, "render_item (Local x) ="),
        (main.as_path(), MAIN, "render_item x\n"),
        (main.as_path(), MAIN, "render_item 1"),
        (main.as_path(), MAIN, "render_item (Local \"value\")"),
    ] {
        let prepared = lsp.request(
            "textDocument/prepareRename",
            &json!({ "textDocument": { "uri": uri(file) }, "position": at(text, needle) }),
        );
        assert!(prepared["error"].is_null(), "{prepared}");
        assert!(!prepared["result"].is_null(), "{prepared}");
        edits = rename(&mut lsp, file, text, needle, "display_item");
        assert_changes(&edits, &files, "render_item", "display_item");
    }

    let updated: Vec<_> = files
        .iter()
        .map(|&(file, text, count)| {
            let updated = apply_edits(text, &edits["result"]["changes"][uri(file)], "render_item");
            assert_eq!(updated, text.replacen("render_item", "display_item", count));
            lsp.notify(
                "textDocument/didChange",
                &json!({ "textDocument": { "uri": uri(file), "version": 2 }, "contentChanges": [{ "text": updated }] }),
            );
            (file, updated, count)
        })
        .collect();
    assert_latest_diagnostics_clean(&mut lsp);
    let updated_files: Vec<_> = updated
        .iter()
        .map(|(file, text, count)| (*file, text.as_str(), *count))
        .collect();
    let repeated = rename(
        &mut lsp,
        &main,
        &updated[2].1,
        "display_item 1",
        "show_item",
    );
    assert_changes(&repeated, &updated_files, "display_item", "show_item");
    // No workspace edit was saved: close must restore the original disk view.
    for &(file, _, _) in &files {
        lsp.notify(
            "textDocument/didClose",
            &json!({ "textDocument": { "uri": uri(file) } }),
        );
    }
    let restored = rename(&mut lsp, &label, LABEL, "render_item :", "display_item");
    assert_changes(&restored, &files, "render_item", "display_item");
    lsp.close();
}

#[test]
fn class_rename_refuses_ambiguity_in_a_dependent_member_and_recovers_after_editing() {
    let broken = "module App.Main exposing (..)\nimport Lib.Facade exposing (Label, render_item)\nimport App.Other exposing (Other, render_item)\nlet result = render_item true\n";
    let dir = workspace("lsp_class_rename_ambiguous", broken);
    let label = dir.join("library/src/Lib/Label.fl");
    let main = dir.join("client/src/App/Main.fl");
    let mut lsp = Lsp::start(&dir.join("library"));
    open(&mut lsp, &label, LABEL);
    assert_latest_diagnostics_clean(&mut lsp);
    // The library analysis is clean; only the unopened dependent is ambiguous.
    let refused = rename(&mut lsp, &label, LABEL, "render_item :", "display_item");
    assert!(refused["error"].is_object(), "{refused}");
    assert!(refused["result"].is_null(), "no partial edits: {refused}");

    open(&mut lsp, &main, broken);
    let diagnostics = lsp.diagnostics(&uri(&main));
    assert!(
        diagnostics.iter().any(|d| d["code"] == "FLICK-R010"),
        "{diagnostics:?}"
    );
    let ambiguous_call = rename(&mut lsp, &main, broken, "render_item true", "display_item");
    assert!(ambiguous_call["error"].is_object(), "{ambiguous_call}");

    lsp.notify(
        "textDocument/didChange",
        &json!({ "textDocument": { "uri": uri(&main), "version": 2 }, "contentChanges": [{ "text": MAIN }] }),
    );
    assert_latest_diagnostics_clean(&mut lsp);
    let recovered = rename(&mut lsp, &label, LABEL, "render_item :", "display_item");
    let facade = dir.join("library/src/Lib/Facade.fl");
    assert_changes(
        &recovered,
        &[
            (label.as_path(), LABEL, 4),
            (facade.as_path(), FACADE, 2),
            (main.as_path(), MAIN, 6),
        ],
        "render_item",
        "display_item",
    );
    lsp.close();
}
