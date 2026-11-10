use std::sync::{Arc, Mutex};

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
        snapshots: HashMap::new(),
        latest: None,
        caches: Caches::new(),
        published: HashMap::new(),
        out: Box::new(io::sink()),
        registered: None,
        requests: 0,
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

/// What a server writes, kept for the test to read.
#[derive(Clone, Default)]
struct Written(Arc<Mutex<Vec<u8>>>);

impl Write for Written {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .expect("not poisoned")
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn the_extensions_of_loaded_files_are_registered_once_where_the_client_lets_it() {
    let written = Written::default();
    let mut server = Server {
        docs: HashMap::new(),
        snapshots: HashMap::new(),
        latest: None,
        caches: Caches::new(),
        published: HashMap::new(),
        out: Box::new(written.clone()),
        registered: None,
        requests: 0,
    };
    let loaded = [
        Path::new("/p/src/App/Main.fl"),
        Path::new("/p/src/App/Words.pairs"),
    ];
    // a client that registers nothing later gets no request
    server.register_loaded(loaded.into_iter());
    assert!(written.0.lock().expect("not poisoned").is_empty());
    let initialize = json!({ "capabilities": { "textDocument": { "synchronization": { "dynamicRegistration": true } } } });
    server
        .request("initialize", &initialize)
        .expect("initialized");
    server.register_loaded(loaded.into_iter());
    server.register_loaded(loaded.into_iter());
    let out = String::from_utf8(written.0.lock().expect("not poisoned").clone()).expect("UTF-8");
    assert_eq!(out.matches("client/registerCapability").count(), 1, "{out}");
    assert!(
        out.contains("**/*.pairs") && !out.contains("**/*.fl"),
        "{out}"
    );
}
