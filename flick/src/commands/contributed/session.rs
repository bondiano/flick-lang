//! A session command: the host owns stdio and its `Content-Length`
//! framing, hands the component one message at a time with the state of
//! the session, and answers a trap or a cancelled request itself, as
//! JSON-RPC errors, keeping the state the request started from.

use std::io::{self, BufReader, Write};
use std::process::ExitCode;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use flick_backend::command::{Descriptor, Handled, Invocation, Opened, Plan};
use flick_backend::host::runtime::Cancellation;
use flick_core::project::Project;
use flick_core::source::SourceMap;
use serde_json::{Value, json};

use super::Contributor;
use super::plan::{InPlaceMode, carry_out, finish};
use super::projection::Projections;
use crate::args::GlobalArguments;
use crate::backends::Tool;
use crate::component;
use crate::lsp::protocol;

/// JSON-RPC: the request was cancelled.
const CANCELLED: i64 = -32800;
/// JSON-RPC: the server failed on the request.
const FAILED: i64 = -32603;
/// How many calls one request may take, answering what each needs.
const ROUNDS: usize = 16;

pub(super) fn serve(
    project: &Project,
    contributor: &dyn Contributor,
    descriptor: &Descriptor,
    invocation: &Invocation,
    tools: &[Tool],
    global: &GlobalArguments,
) -> ExitCode {
    let mut state = match contributor.open(invocation) {
        Ok(Opened::Session(state)) => state,
        // it runs once instead, as a plan command does
        Ok(Opened::Plan(plan)) => {
            return finish_plan(
                project,
                contributor,
                descriptor,
                invocation,
                plan,
                tools,
                global,
                false,
            );
        }
        Err(diagnostics) => {
            return finish(global, invocation, SourceMap::default(), &diagnostics, &[]);
        }
    };
    let mut projections = Projections::default();
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
        let request = message.to_string().into_bytes();
        let (mut current, mut answers) = (state.clone(), Vec::new());
        let mut answer = Err(format!(
            "the session needed more than {ROUNDS} calls for a request"
        ));
        // a call that needs something gets it, answered, in the next one
        for _ in 0..ROUNDS {
            let call = || contributor.handle(current.clone(), request.clone(), &answers, &cancel);
            match call_cancellable(call, &inbox, &mut waiting, &cancel, id.as_ref()) {
                // the projections keep their caches on this thread
                Ok(handled) if !handled.needs.is_empty() => {
                    answers = projections.answer(&project.root, &handled.needs);
                    current = handled.state;
                }
                done => {
                    answer = done;
                    break;
                }
            }
        }
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
    // stdout is the protocol's: what the session prints goes to stderr
    finish_plan(
        project,
        contributor,
        descriptor,
        invocation,
        plan,
        tools,
        global,
        true,
    )
}

#[allow(
    clippy::too_many_arguments,
    reason = "shared plan completion for session open/close"
)]
fn finish_plan(
    project: &Project,
    contributor: &dyn Contributor,
    descriptor: &Descriptor,
    invocation: &Invocation,
    plan: Plan,
    tools: &[Tool],
    global: &GlobalArguments,
    to_stderr: bool,
) -> ExitCode {
    let done = carry_out(
        project,
        contributor,
        descriptor,
        invocation,
        plan,
        tools,
        InPlaceMode::Tool,
        Some(global),
    );
    if !done.stdout.is_empty() {
        if to_stderr {
            eprint!("{}", String::from_utf8_lossy(&done.stdout));
        } else {
            let _ = io::stdout().write_all(&done.stdout);
        }
    }
    finish(
        global,
        invocation,
        done.sources,
        &done.diagnostics,
        &done.refused,
    )
}

/// The result of `call`, on a thread of its own: what arrives meanwhile
/// waits, but a cancellation of the request `id` stops it.
fn call_cancellable(
    call: impl FnOnce() -> Result<Handled, String> + Send,
    inbox: &mpsc::Receiver<Value>,
    waiting: &mut Vec<Value>,
    cancel: &Cancellation,
    id: Option<&Value>,
) -> Result<Handled, String> {
    thread::scope(|scope| {
        let call = scope.spawn(call);
        while !call.is_finished() {
            match inbox.recv_timeout(Duration::from_millis(10)) {
                Ok(next) if cancels(&next, id) => cancel.cancel(),
                Ok(next) => waiting.insert(0, next),
                Err(_) => {}
            }
        }
        call.join()
            .unwrap_or_else(|_| Err("the session panicked".into()))
    })
}

fn cancels(message: &Value, id: Option<&Value>) -> bool {
    message["method"] == json!("$/cancelRequest")
        && id.is_some_and(|id| &message["params"]["id"] == id)
}
