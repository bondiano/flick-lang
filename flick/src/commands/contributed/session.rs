//! A session command: the host owns stdio and its `Content-Length`
//! framing, hands the component one message at a time with the state of
//! the session, and answers a trap or a cancelled request itself, as
//! JSON-RPC errors, keeping the state the request started from.

use std::io::{self, BufReader};
use std::process::ExitCode;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use flick_backend::command::{Descriptor, Invocation};
use flick_backend::host::runtime::Cancellation;
use flick_core::project::Project;
use serde_json::{Value, json};

use super::Contributor;
use super::plan::{carry_out, finish};
use crate::args::GlobalArguments;
use crate::backends::Tool;
use crate::component;
use crate::lsp::protocol;

/// JSON-RPC: the request was cancelled.
const CANCELLED: i64 = -32800;
/// JSON-RPC: the server failed on the request.
const FAILED: i64 = -32603;

pub(super) fn serve(
    project: &Project,
    contributor: &dyn Contributor,
    descriptor: &Descriptor,
    invocation: &Invocation,
    tools: &[Tool],
    global: &GlobalArguments,
) -> ExitCode {
    let mut state = match contributor.open(invocation) {
        Ok(state) => state,
        Err(diagnostics) => return finish(global, invocation, &diagnostics, &[]),
    };
    let Ok(engine) = component::engine() else {
        return ExitCode::from(1);
    };
    let (messages, inbox) = mpsc::channel::<Value>();
    thread::spawn(move || {
        let mut input = BufReader::new(io::stdin().lock());
        while let Ok(Some(message)) = protocol::read(&mut input) {
            if messages.send(message).is_err() {
                break;
            }
        }
    });
    let mut stdout = io::stdout().lock();
    let mut waiting = Vec::new();
    while let Some(message) = waiting.pop().or_else(|| inbox.recv().ok()) {
        match message["method"].as_str() {
            Some("exit") => break,
            // a cancellation of a request no longer running asks nothing
            Some("$/cancelRequest") => continue,
            _ => {}
        }
        let id = message.get("id").cloned();
        // a request cancelled before it started is answered without a call
        if let Some(at) = waiting.iter().position(|m| cancels(m, id.as_ref())) {
            waiting.remove(at);
            if let Some(id) = &id {
                let reply = protocol::error(id, CANCELLED, "the request was cancelled");
                if protocol::write(&mut stdout, &reply).is_err() {
                    break;
                }
            }
            continue;
        }
        let cancel = Cancellation::new(engine);
        let answer = thread::scope(|scope| {
            let call = scope.spawn(|| {
                let request = message.to_string().into_bytes();
                contributor.handle(state.clone(), request, &cancel)
            });
            // what arrives meanwhile waits, but a cancellation of this request stops it
            while !call.is_finished() {
                match inbox.recv_timeout(Duration::from_millis(10)) {
                    Ok(next) if cancels(&next, id.as_ref()) => cancel.cancel(),
                    Ok(next) => waiting.insert(0, next),
                    Err(_) => {}
                }
            }
            call.join()
                .unwrap_or_else(|_| Err("the session panicked".into()))
        });
        let reply = match (answer, &id) {
            (Ok(handled), _) => {
                state = handled.state;
                handled
                    .response
                    .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            }
            (Err(_), Some(id)) if cancel.is_cancelled() => {
                Some(protocol::error(id, CANCELLED, "the request was cancelled"))
            }
            (Err(why), Some(id)) => Some(protocol::error(id, FAILED, &why)),
            (Err(_), None) => None,
        };
        if let Some(reply) = reply
            && protocol::write(&mut stdout, &reply).is_err()
        {
            break;
        }
    }
    let plan = contributor.close(state);
    let done = carry_out(project, contributor, descriptor, invocation, plan, tools);
    // stdout is the protocol's: what the session prints goes to stderr
    if !done.stdout.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&done.stdout));
    }
    finish(global, invocation, &done.diagnostics, &done.refused)
}

fn cancels(message: &Value, id: Option<&Value>) -> bool {
    message["method"] == json!("$/cancelRequest")
        && id.is_some_and(|id| &message["params"]["id"] == id)
}
