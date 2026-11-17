//! The requests held while a document's background analysis runs: when a
//! request must wait for it, how the answer follows the published analysis,
//! and how the client's cancellation drops the held request.

use std::mem;

use serde_json::Value;
use tracing::warn;

use super::Server;
use super::protocol;
use super::snapshot::Context;

/// A request waiting for the analysis of a document it reads.
pub(super) struct Deferred {
    pub(super) id: Value,
    pub(super) method: String,
    pub(super) params: Value,
}

/// The JSON-RPC code of a request the client cancelled.
const REQUEST_CANCELLED: i64 = -32800;

/// The JSON-RPC code of a request the server failed on itself.
const INTERNAL_ERROR: i64 = -32603;

impl Server {
    /// Answers the held requests whose analysis has been published, in order.
    pub(super) fn resume(&mut self) {
        let mut waiting = Vec::new();
        for deferred in mem::take(&mut self.deferred) {
            if self.should_defer(&deferred.method, &deferred.params) {
                waiting.push(deferred);
            } else {
                self.answer(&deferred.id, &deferred.method, &deferred.params);
            }
        }
        self.deferred = waiting;
    }

    /// Whether the request reads a context whose analysis is still running.
    pub(super) fn should_defer(&self, method: &str, params: &Value) -> bool {
        if self.pending.is_empty() || matches!(method, "initialize" | "shutdown") {
            return false;
        }
        if method == "workspace/symbol" {
            return true;
        }
        let Some(path) = params["textDocument"]["uri"]
            .as_str()
            .and_then(protocol::path_of)
        else {
            return false;
        };
        let context = Context::of(&self.docs, &path);
        self.pending
            .keys()
            .any(|queued| Context::of(&self.docs, queued) == context)
    }

    pub(super) fn defer(&mut self, id: Value, method: &str, params: Value) {
        self.deferred.push(Deferred {
            id,
            method: method.to_owned(),
            params,
        });
    }

    /// Forgets a held request, as the client no longer wants its answer.
    pub(super) fn cancel_request(&mut self, params: &Value) {
        let Some(id) = params["id"].as_i64() else {
            return;
        };
        let id = Value::from(id);
        if let Some(at) = self.deferred.iter().position(|d| d.id == id) {
            self.deferred.remove(at);
            self.send(&protocol::error(
                &id,
                REQUEST_CANCELLED,
                "request cancelled",
            ));
        }
    }

    pub(super) fn answer(&mut self, id: &Value, method: &str, params: &Value) {
        let reply = match self.guarded(|s| s.request(method, params)) {
            Some(Ok(result)) => protocol::response(id, &result),
            Some(Err((code, text))) => {
                warn!(code, error = %text, "request failed");
                protocol::error(id, code, &text)
            }
            None => protocol::error(
                id,
                INTERNAL_ERROR,
                "internal error in the language server; its log has the details",
            ),
        };
        self.send(&reply);
    }
}
