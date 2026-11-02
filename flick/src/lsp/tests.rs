use super::*;
use serde_json::json;

fn document(path: &Path) -> Value {
    json!({ "textDocument": { "uri": protocol::uri_of(path), "languageId": "flick", "version": 1,
        "text": "module A exposing (x)\n\nx : Number\nlet x = 1\n" } })
}

#[test]
fn a_closed_standalone_file_leaves_no_cache_behind() {
    let mut server = Server {
        docs: HashMap::new(),
        snapshot: None,
        caches: Caches::new(),
        published: HashMap::new(),
        out: Box::new(io::sink()),
    };
    let dir = env::temp_dir().join(format!("flick-lsp-caches-{}", process::id()));
    for name in ["A.fl", "B.fl", "C.fl"] {
        let params = document(&dir.join(name));
        server.notify("textDocument/didOpen", &params);
        assert_eq!(server.caches.len(), 1, "{name} opened");
        server.notify("textDocument/didClose", &params);
        assert!(server.caches.is_empty(), "{name} closed");
    }
}
