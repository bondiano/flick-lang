//! `λRT` on traces (docs/formal/runtime.md). With `FLICK_TRACE` set, both
//! runtimes write the steps of the owner contract as JSON lines
//! (`runtime-go/flickrt/trace.go` lists the events, `runtime/src/trace.ts`
//! writes the same). [`monitor::check`] replays a trace and reports the
//! first step `λRT` does not allow; [`project::project`] is what of a trace
//! must agree between the JS and Go runs of one program: per owner, not
//! the ids of fibers or the global order of steps.

pub mod monitor;
pub mod project;

use serde_json::Value;

/// One step of a trace: its line, its name and its fields; a field the
/// event does not have is 0 or empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Event {
    pub line: usize,
    pub name: String,
    /// The owner (`o`), the one it opened in (`p`) and its kind (`k`).
    pub owner: i64,
    pub parent: i64,
    pub kind: String,
    /// A phase of a close, C0…C6 (`c`).
    pub phase: String,
    /// A child (`f`), a finalizer's place (`i`), a layer (`l`), a foreign
    /// call (`t`) and how a fiber ended (`x`).
    pub fiber: i64,
    pub place: i64,
    pub layer: i64,
    pub call: i64,
    pub exit: String,
    /// The line as written.
    pub text: String,
}

/// The events of a trace, or the line that is not one.
pub fn parse(trace: &str) -> Result<Vec<Event>, String> {
    let mut out = Vec::new();
    for (n, text) in trace.lines().enumerate() {
        if text.trim().is_empty() {
            continue;
        }
        let line = n + 1;
        let value: Value =
            serde_json::from_str(text).map_err(|e| format!("trace line {line}: {e}: {text}"))?;
        let int = |key: &str| value.get(key).and_then(Value::as_i64).unwrap_or(0);
        let string = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned()
        };
        let name = string("e");
        if name.is_empty() {
            return Err(format!("trace line {line}: no event name: {text}"));
        }
        out.push(Event {
            line,
            name,
            owner: int("o"),
            parent: int("p"),
            kind: string("k"),
            phase: string("c"),
            fiber: int("f"),
            place: int("i"),
            layer: int("l"),
            call: int("t"),
            exit: string("x"),
            text: text.to_owned(),
        });
    }
    Ok(out)
}
