//! One explicit proof search at a time, with the latest request queued when
//! a cancelled search is still finishing. Normal analysis has its own worker.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

use serde_json::{Value, json};

use super::deferred::{INTERNAL_ERROR, REQUEST_CANCELLED};
use super::requests::{INVALID_PARAMS, REQUEST_FAILED, Reply};
use super::snapshot::{ProofSearchResult, search_proof};
use super::{Server, protocol};

#[derive(Default)]
pub(super) struct Search {
    active: Option<Active>,
    queued: Option<Job>,
}

struct Request {
    id: Value,
    path: PathBuf,
    position: Value,
    generation: u64,
    version: Option<i64>,
}

struct Job {
    request: Request,
    offset: u32,
}

struct Active {
    // Taken as soon as a cancellation reply is sent. The worker still owns
    // its buffers until it returns.
    request: Option<Request>,
    cancelled: Arc<AtomicBool>,
    thread: JoinHandle<ProofSearchResult>,
}

impl Search {
    /// An ID cancels only that request. An input change clears both slots
    /// and lets the running worker reach its next stop check.
    fn cancel(&mut self, id: Option<&Value>) -> [Option<Value>; 2] {
        let matches = |request: &Request| id.is_none_or(|id| request.id == *id);
        let active = self.active.as_mut().and_then(|active| {
            let request = active.request.take_if(|request| matches(request))?;
            active.cancelled.store(true, Ordering::Relaxed);
            Some(request.cancel(id.is_some()))
        });
        let queued = self
            .queued
            .take_if(|queued| matches(&queued.request))
            .map(|job| job.request.cancel(id.is_some()));
        [active, queued]
    }
}

impl Drop for Search {
    fn drop(&mut self) {
        if let Some(active) = self.active.take() {
            active.cancelled.store(true, Ordering::Relaxed);
            let _ = active.thread.join();
        }
    }
}

impl Request {
    fn reply(&self, status: &str, suggestions: &[Value]) -> Value {
        protocol::response(
            &self.id,
            &json!({
                "generation": self.generation,
                "version": self.version,
                "position": self.position,
                "status": status,
                "suggestions": suggestions,
            }),
        )
    }

    fn cancel(&self, requested: bool) -> Value {
        if requested {
            protocol::error(&self.id, REQUEST_CANCELLED, "request cancelled")
        } else {
            self.reply("cancelled", &[])
        }
    }
}

fn path(params: &Value) -> Result<PathBuf, (i64, String)> {
    let uri = params["textDocument"]["uri"].as_str().unwrap_or("");
    protocol::path_of(uri).ok_or((INVALID_PARAMS, format!("not a file: `{uri}`")))
}

impl Server {
    pub(super) fn start_proof_search(&mut self, id: &Value, params: &Value) {
        let path = match path(params) {
            Ok(path) => path,
            Err((code, text)) => return self.send(&protocol::error(id, code, &text)),
        };
        let position = &params["position"];
        if position["line"].as_u64().is_none() || position["character"].as_u64().is_none() {
            return self.send(&protocol::error(id, INVALID_PARAMS, "expected a position"));
        }
        self.proof_watch = true;
        let version = self.versions.get(&path).copied();
        let request = Request {
            id: id.clone(),
            path,
            position: position.clone(),
            generation: self.generation,
            version,
        };
        let Some(text) = self.docs.get(&request.path) else {
            return self.send(&request.reply("no-candidate", &[]));
        };
        let offset = protocol::offset(text, position);
        self.cancel_proof_search(None);
        self.search.queued = Some(Job { request, offset });
        self.poll_proof_search();
    }

    pub(super) fn validate_proof_search(&self, params: &Value) -> Reply {
        let path = path(params)?;
        let current = params["generation"].as_u64() == Some(self.generation)
            && params["version"].as_i64().is_some_and(|version| {
                self.docs.contains_key(&path) && self.versions.get(&path) == Some(&version)
            });
        Ok(json!(current))
    }

    pub(super) fn cancel_proof_search(&mut self, id: Option<&Value>) {
        for reply in self.search.cancel(id).into_iter().flatten() {
            self.send(&reply);
        }
    }

    pub(super) fn poll_proof_search(&mut self) {
        if let Some(active) = self
            .search
            .active
            .take_if(|active| active.thread.is_finished())
        {
            let result = active.thread.join();
            if let Some(request) = active.request {
                let reply = if let Ok(result) = result {
                    self.proof_search_reply(&request, result)
                } else {
                    protocol::error(&request.id, INTERNAL_ERROR, "proof search failed")
                };
                self.send(&reply);
            }
        }
        if self.search.active.is_none()
            && let Some(job) = self.search.queued.take()
        {
            let cancelled = Arc::new(AtomicBool::new(false));
            let stop = Arc::clone(&cancelled);
            let path = job.request.path.clone();
            let docs = self.docs.clone();
            let thread = thread::Builder::new()
                .name("flick-proof-search".into())
                .spawn(move || search_proof(&docs, &path, job.offset, &stop));
            match thread {
                Ok(thread) => {
                    self.search.active = Some(Active {
                        request: Some(job.request),
                        cancelled,
                        thread,
                    });
                }
                Err(err) => self.send(&protocol::error(
                    &job.request.id,
                    REQUEST_FAILED,
                    &err.to_string(),
                )),
            }
        }
    }

    fn proof_search_reply(&self, request: &Request, result: ProofSearchResult) -> Value {
        let current = self.generation == request.generation
            && self.versions.get(&request.path).copied() == request.version;
        let Some(text) = self.docs.get(&request.path).filter(|_| current) else {
            return request.reply("cancelled", &[]);
        };
        let suggestions: Option<Vec<Value>> = result
            .suggestions
            .into_iter()
            .map(|(suggestion, (start, end))| {
                text.get(start as usize..end as usize)?;
                Some(json!({ "text": suggestion, "range": protocol::range(text, start, end) }))
            })
            .collect();
        match suggestions {
            Some(suggestions) => request.reply(result.status, &suggestions),
            None => protocol::error(
                &request.id,
                INTERNAL_ERROR,
                "invalid proof suggestion range",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(id: &str) -> Request {
        Request {
            id: json!(id),
            path: PathBuf::from("Proofs.fl"),
            position: json!({ "line": 1, "character": 0 }),
            generation: 1,
            version: Some(1),
        }
    }

    #[test]
    fn a_cancelled_request_replies_once_and_releases_the_worker_receipt() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut search = Search {
            active: Some(Active {
                request: Some(request("running")),
                cancelled: Arc::clone(&cancelled),
                thread: thread::spawn(|| ProofSearchResult {
                    status: "found",
                    suggestions: vec![("refl".into(), (0, 0))],
                }),
            }),
            queued: Some(Job {
                request: request("queued"),
                offset: 0,
            }),
        };
        assert_eq!(search.cancel(Some(&json!("other"))), [None, None]);
        let [active, queued] = search.cancel(Some(&json!("running")));
        let active = active.expect("request cancellation response");
        assert_eq!(active["id"], "running");
        assert_eq!(active["error"]["code"], REQUEST_CANCELLED);
        assert!(queued.is_none());
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(
            search
                .active
                .as_ref()
                .expect("unfinished record")
                .request
                .is_none()
        );

        let [active, queued] = search.cancel(None);
        assert!(active.is_none(), "the running request was already settled");
        let queued = queued.expect("change superseded the queued request");
        assert_eq!(queued["id"], "queued");
        assert_eq!(queued["result"]["status"], "cancelled");
        assert_eq!(search.cancel(None), [None, None]);
    }
}
