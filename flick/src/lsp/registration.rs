//! Files of other languages a project loads, as a plugin's `load` gives
//! their code: the client sends their edits once the server registers their
//! extensions, which it does when the client lets it.

use std::collections::HashSet;
use std::path::Path;

use serde_json::{Value, json};

use super::Server;

impl Server {
    /// Registers, once each, the extensions of the files of `loaded` that are
    /// not Flick, so the client sends the server their edits.
    pub(super) fn register_loaded<'p>(&mut self, loaded: impl Iterator<Item = &'p Path>) {
        let Some(done) = &mut self.registered else {
            return;
        };
        let fresh: Vec<String> = loaded
            .filter_map(|p| p.extension()?.to_str())
            .filter(|e| *e != "fl")
            .filter(|e| done.insert((*e).to_owned()))
            .map(str::to_owned)
            .collect();
        if fresh.is_empty() {
            return;
        }
        let registrations: Vec<Value> = (fresh.iter())
            .flat_map(|ext| {
                let selector = json!([{ "pattern": format!("**/*.{ext}") }]);
                [
                    "textDocument/didOpen",
                    "textDocument/didChange",
                    "textDocument/didClose",
                ]
                .map(|method| {
                    json!({
                        "id": format!("flick-load-{ext}-{method}"),
                        "method": method,
                        "registerOptions": { "documentSelector": selector, "syncKind": 1 },
                    })
                })
            })
            .collect();
        self.requests += 1;
        let request = json!({
            "jsonrpc": "2.0",
            "id": format!("flick-{}", self.requests),
            "method": "client/registerCapability",
            "params": { "registrations": registrations },
        });
        self.send(&request);
    }
}

/// What the server registers later, when the client lets it register the
/// sync of documents: nothing yet.
pub(super) fn registrations(initialize: &Value) -> Option<HashSet<String>> {
    let sync = &initialize["capabilities"]["textDocument"]["synchronization"];
    (sync["dynamicRegistration"].as_bool() == Some(true)).then(HashSet::new)
}
