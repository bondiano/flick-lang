use flick_core::ffi::import;
use flick_core::ffi::{Policy, Surface};

use super::*;

const LOOKUP: &str = include_str!("../../../flick-core/src/ffi/fixtures/lookup.json");
/// A `.d.ts` reader that answers every request with the fixture surface.
const READER: &str = "import { readFileSync } from \"node:fs\";\n\
    readFileSync(0);\n\
    process.stdout.write(readFileSync(new URL(\"lookup.json\", import.meta.url)));\n";
const GENERATED: &str = "src/Ffi/LookupLib/Generated.fl";

fn fresh(select: Option<&[String]>) -> String {
    let surface = Surface::parse(LOOKUP).expect("fixture parses");
    import::generate(
        &surface,
        &Policy::default(),
        select,
        "Ffi.LookupLib.Generated",
    )
    .text
}

/// The actions offered on the first line of `text`, opened as `file`.
fn header_actions(lsp: &mut Lsp, file: &Path, text: &str, trigger: u64) -> Vec<Value> {
    let actions = lsp.request(
        "textDocument/codeAction",
        &json!({
            "textDocument": { "uri": uri(file) },
            "range": { "start": position(text, 0), "end": position(text, 0) },
            "context": { "diagnostics": [], "triggerKind": trigger },
        }),
    );
    actions["result"]
        .as_array()
        .expect("actions")
        .iter()
        .filter(|a| {
            a["title"]
                .as_str()
                .is_some_and(|t| t.starts_with("Regenerate"))
        })
        .cloned()
        .collect()
}

#[test]
fn header_regenerates_the_imported_module() {
    let dir = project(
        "lsp_ffi_regenerate",
        &[
            ("flick.toml", LIB),
            ("node_modules/@flick-lang/dts-importer/index.mjs", READER),
            ("node_modules/@flick-lang/dts-importer/lookup.json", LOOKUP),
        ],
    );
    let file = dir.join(GENERATED);
    fs::create_dir_all(file.parent().expect("dir")).expect("dir");
    let mut lsp = Lsp::start(&dir);
    let select = vec!["nextId".to_owned()];
    for (header, select) in [
        ("// flick ffi import lookup-lib\n", None),
        (
            "// flick ffi import lookup-lib --select nextId\n",
            Some(select.as_slice()),
        ),
    ] {
        fs::write(&file, header).expect("stub");
        lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": header } }));
        // the cursor moving onto the header reads no declarations
        assert_eq!(
            header_actions(&mut lsp, &file, header, 2),
            Vec::<Value>::new()
        );
        let actions = header_actions(&mut lsp, &file, header, 1);
        let [action] = actions.as_slice() else {
            panic!("one regenerate action: {actions:?}");
        };
        assert_eq!(action["title"], "Regenerate from lookup-lib@1.0.0");
        assert_eq!(action["kind"], "refactor.rewrite");
        let edits = action["edit"]["changes"][uri(&file)]
            .as_array()
            .expect("edits");
        let [edit] = edits.as_slice() else {
            panic!("one edit: {edits:?}");
        };
        assert_eq!(
            edit["range"],
            json!({ "start": position(header, 0), "end": position(header, header.len()) })
        );
        let text = edit["newText"].as_str().expect("text");
        assert_eq!(text, fresh(select), "{header}");
        assert_eq!(text.contains("unsupported:"), select.is_none(), "{text}");
        // the regenerated module offers nothing more
        lsp.notify("textDocument/didChange", &json!({ "textDocument": { "uri": uri(&file), "version": 2 }, "contentChanges": [{ "text": text }] }));
        assert_eq!(
            header_actions(&mut lsp, &file, text, 1),
            Vec::<Value>::new()
        );
        lsp.notify(
            "textDocument/didClose",
            &json!({ "textDocument": { "uri": uri(&file) } }),
        );
    }
    lsp.close();
}

#[test]
fn header_without_a_reader_disables_the_action() {
    let header = "// flick ffi import lookup-lib\n";
    let dir = project(
        "lsp_ffi_no_reader",
        &[("flick.toml", LIB), (GENERATED, header)],
    );
    let file = dir.join(GENERATED);
    let mut lsp = Lsp::start(&dir);
    lsp.notify("textDocument/didOpen", &json!({ "textDocument": { "uri": uri(&file), "languageId": "flick", "version": 1, "text": header } }));
    let actions = header_actions(&mut lsp, &file, header, 1);
    let [action] = actions.as_slice() else {
        panic!("one regenerate action: {actions:?}");
    };
    assert_eq!(action["title"], "Regenerate from lookup-lib");
    assert!(action.get("edit").is_none(), "{action}");
    assert!(
        action["disabled"]["reason"]
            .as_str()
            .is_some_and(|r| r.contains("no `.d.ts` reader")),
        "{action}"
    );
    lsp.close();
}
