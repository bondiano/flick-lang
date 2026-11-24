//! The cursor's proof context, published from the same revision as diagnostics.

use flick_core::ide::ProofView;
use serde_json::{Value, json};

use super::{Context, INVALID_PARAMS, Reply, Server, ide, protocol};

impl Server {
    pub(in crate::lsp) fn proof_at(&mut self, params: &Value) -> Reply {
        let uri = params["textDocument"]["uri"].as_str().unwrap_or("");
        let path =
            protocol::path_of(uri).ok_or((INVALID_PARAMS, format!("not a file: `{uri}`")))?;
        let position = &params["position"];
        self.proof_watch = true;
        let version = self.versions.get(&path).copied();
        let context = Context::of(&self.docs, &path);
        let mut pending = false;
        let mut proof = None;
        let mut snapshot_generation = None;
        if let Some(text) = self.docs.get(&path) {
            if self.pending.contains_key(&context) {
                pending = true;
            } else if let Some(snapshot) = self.snapshots.get(&context) {
                snapshot_generation = Some(snapshot.generation);
                let offset = protocol::offset(text, position);
                proof = snapshot.file(&path).and_then(|file| {
                    ide(snapshot)
                        .proof_at(file, offset)
                        .map(|view| proof_json(text, &view))
                });
            } else {
                self.refresh(&path);
                pending = true;
            }
        }
        Ok(json!({
            "generation": self.generation,
            "version": version,
            "snapshotGeneration": snapshot_generation,
            "position": position,
            "pending": pending,
            "proof": proof,
        }))
    }
}

fn proof_json(text: &str, view: &ProofView<'_>) -> Value {
    let range = |(start, end)| protocol::range(text, start, end);
    let mut proof = json!(view);
    proof["range"] = range(view.range);
    proof["goalRange"] = json!(view.goal_range.map(range));
    proof["failureRange"] = json!(view.failure_range.map(range));
    for (i, obligation) in view.obligations.iter().enumerate() {
        proof["obligations"][i]["range"] = json!(obligation.range.map(range));
    }
    proof
}
