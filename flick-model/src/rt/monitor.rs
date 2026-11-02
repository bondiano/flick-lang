//! The monitor of `λRT`: replays a trace and checks, step by step, the
//! invariants and properties of docs/formal/runtime.md §4 and §6 that a
//! trace shows.
//!
//! - I1: the phases of an owner's close come once each, C0 to C6 in order.
//! - I2: children are admitted only while the owner is open (Spawn), and
//!   finalizers join its stack only before C5 (Fin-Add); later they run at
//!   once (Fin-Now).
//! - I3: a child is registered with its watcher once, then ends, its
//!   supervisor is called on a failure, and its watcher is done, once each.
//! - R1: each finalizer starts exactly once, and those of the stack before
//!   the owner's close ends.
//! - R2: the stack runs in C5, last added first, after every child and
//!   watcher of the owner ended (C3) and its foreign calls settled (C4).
//! - R3: after its close no step of the owner or its children happens.
//! - R4: a child that failed or was interrupted reaches its supervisor
//!   before its watcher is done.
//! - R6 and I6, as far as a trace without memo maps shows them: a layer's
//!   build is released at most once, and every one is released when main
//!   has ended.
//!
//! Not checked here: I4 (who acquires into an owner: the trace does not
//! name the fiber of a step), I5 (the typed head, R5: `cause_laws.fl`),
//! R7 (`Ref` is not traced), and the hooks of C1, which are not traced.

use std::collections::BTreeMap;
use std::fmt;

use super::Event;

/// The first step `λRT` does not allow: the rule, what is wrong, and the
/// events of the trace that lead to it.
#[derive(Debug, PartialEq, Eq)]
pub struct Violation {
    pub rule: &'static str,
    pub message: String,
    pub line: usize,
    pub context: Vec<String>,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{} at trace line {}: {}",
            self.rule, self.line, self.message
        )?;
        for line in &self.context {
            writeln!(f, "    {line}")?;
        }
        Ok(())
    }
}

/// The phases of a close, in order; an open owner is before C0.
const PHASES: [&str; 7] = ["C0", "C1", "C2", "C3", "C4", "C5", "C6"];

#[derive(Default)]
struct Child {
    exit: Option<String>,
    supervised: bool,
    watched: bool,
}

struct Finalizer {
    /// Added after C5: it runs at once.
    now: bool,
    ran: bool,
}

#[derive(Default)]
struct Owner {
    kind: String,
    parent: i64,
    /// How many phases it has passed: 0 while open, 7 once closed.
    phase: usize,
    children: BTreeMap<i64, Child>,
    finalizers: BTreeMap<i64, Finalizer>,
    /// The place of the last finalizer of the stack that ran in C5.
    last_run: Option<i64>,
    calls: BTreeMap<i64, bool>,
}

#[derive(Default)]
struct State {
    owners: BTreeMap<i64, Owner>,
    /// Builds of each layer not released yet.
    layers: BTreeMap<i64, i64>,
    exited: bool,
}

/// Checks a trace against `λRT`: `Ok` when every step is allowed.
pub fn check(events: &[Event]) -> Result<(), Violation> {
    let mut state = State::default();
    for (at, event) in events.iter().enumerate() {
        step(&mut state, event).map_err(|(rule, message)| violation(events, at, rule, message))?;
    }
    Ok(())
}

fn violation(events: &[Event], at: usize, rule: &'static str, message: String) -> Violation {
    let event = &events[at];
    // the steps of the same owner before it, and the step itself
    let mut context: Vec<String> = events[..at]
        .iter()
        .filter(|e| event.owner != 0 && e.owner == event.owner)
        .rev()
        .take(8)
        .map(|e| format!("{}: {}", e.line, e.text))
        .collect();
    context.reverse();
    context.push(format!("{}: {}  <-", event.line, event.text));
    Violation {
        rule,
        message,
        line: event.line,
        context,
    }
}

type Step = Result<(), (&'static str, String)>;

fn fail(rule: &'static str, message: impl Into<String>) -> Step {
    Err((rule, message.into()))
}

fn step(state: &mut State, e: &Event) -> Step {
    if state.exited {
        return fail("R1", "a step after main ended");
    }
    match e.name.as_str() {
        "open" => open(state, e),
        "phase" => phase(state, e),
        "build" => {
            *state.layers.entry(e.layer).or_default() += 1;
            Ok(())
        }
        "release" => match state.layers.get_mut(&e.layer) {
            Some(n) if *n > 0 => {
                *n -= 1;
                Ok(())
            }
            _ => fail(
                "R6",
                format!("layer {} released without a build to release", e.layer),
            ),
        },
        "exit" => exit(state),
        _ => owner_step(state, e),
    }
}

fn open(state: &mut State, e: &Event) -> Step {
    if state.owners.contains_key(&e.owner) {
        return fail("I1", format!("owner {} opens twice", e.owner));
    }
    if e.parent != 0 {
        match state.owners.get(&e.parent) {
            Some(p) if p.phase < PHASES.len() => {}
            Some(_) => {
                return fail(
                    "R3",
                    format!("an owner opens inside {}, which has closed", e.parent),
                );
            }
            None => {
                return fail(
                    "I1",
                    format!("an owner opens inside {}, which never opened", e.parent),
                );
            }
        }
    }
    let owner = Owner {
        kind: e.kind.clone(),
        parent: e.parent,
        ..Owner::default()
    };
    state.owners.insert(e.owner, owner);
    Ok(())
}

fn phase(state: &mut State, e: &Event) -> Step {
    let Some(o) = state.owners.get_mut(&e.owner) else {
        return fail(
            "I1",
            format!("a close of owner {}, which never opened", e.owner),
        );
    };
    let Some(n) = PHASES.iter().position(|p| *p == e.phase) else {
        return fail("I1", format!("no phase {}", e.phase));
    };
    if n != o.phase {
        let at = if o.phase == 0 {
            "open"
        } else {
            PHASES[o.phase - 1]
        };
        return fail(
            "I1",
            format!(
                "{} after {at}: the phases come once each, in order",
                e.phase
            ),
        );
    }
    o.phase = n + 1;
    match e.phase.as_str() {
        "C3" | "C4" | "C5" => {
            if let Some((f, _)) = o.children.iter().find(|(_, c)| !c.watched) {
                return fail(
                    "R2",
                    format!("{} while child {f} or its watcher runs", e.phase),
                );
            }
            if e.phase != "C3"
                && let Some((t, _)) = o.calls.iter().find(|(_, settled)| !**settled)
            {
                return fail(
                    "R2",
                    format!("{} while foreign call {t} has not settled", e.phase),
                );
            }
            Ok(())
        }
        "C6" => match o.finalizers.iter().find(|(_, f)| !f.now && !f.ran) {
            Some((i, _)) => fail("R1", format!("closed, but finalizer {i} never ran")),
            None => Ok(()),
        },
        _ => Ok(()),
    }
}

fn exit(state: &mut State) -> Step {
    state.exited = true;
    // the owners main waits for: its own, and every one opened inside one
    if let Some((o, owner)) = state
        .owners
        .iter()
        .find(|(_, o)| (o.parent != 0 || o.kind == "main") && o.phase < PHASES.len())
    {
        return fail(
            "R1",
            format!("main ended while {} owner {o} has not closed", owner.kind),
        );
    }
    match state.layers.iter().find(|(_, n)| **n > 0) {
        Some((l, _)) => fail("R6", format!("main ended while layer {l} is still built")),
        None => Ok(()),
    }
}

fn owner_step(state: &mut State, e: &Event) -> Step {
    let Some(o) = state.owners.get_mut(&e.owner) else {
        return fail(
            "I1",
            format!("a step of owner {}, which never opened", e.owner),
        );
    };
    let closed = o.phase == PHASES.len();
    if closed && !matches!(e.name.as_str(), "now" | "run" | "refuse") {
        return fail(
            "R3",
            format!("{} of owner {} after it closed", e.name, e.owner),
        );
    }
    match e.name.as_str() {
        "spawn" => {
            if o.phase > 0 {
                return fail("I2", "a child admitted while the owner closes");
            }
            if o.children.insert(e.fiber, Child::default()).is_some() {
                return fail("I3", format!("child {} registered twice", e.fiber));
            }
            Ok(())
        }
        "refuse" if o.phase == 0 => fail("I2", "a spawn refused while the owner is open"),
        "refuse" => Ok(()),
        "interrupt" => match o.children.get(&e.fiber) {
            _ if o.phase != 3 => fail("I1", "children are interrupted only in C2"),
            Some(c) if !c.watched => Ok(()),
            _ => fail(
                "I3",
                format!("C2 interrupts {}, no running child of the owner", e.fiber),
            ),
        },
        "end" | "sup" | "watched" => child_step(o, e),
        "fin" | "now" => {
            let now = e.name == "now";
            if now != (o.phase >= 6) {
                return fail(
                    "I2",
                    format!(
                        "`{}` in phase {}: before C5 a finalizer joins the stack, after it runs at once",
                        e.name, o.phase
                    ),
                );
            }
            if e.place != i64::try_from(o.finalizers.len()).unwrap_or(i64::MAX) + 1 {
                return fail("I2", format!("finalizer {} out of order", e.place));
            }
            o.finalizers.insert(e.place, Finalizer { now, ran: false });
            Ok(())
        }
        "run" => run(o, e),
        "track" => {
            if o.calls.insert(e.call, false).is_some() {
                return fail("R2", format!("foreign call {} tracked twice", e.call));
            }
            Ok(())
        }
        "settle" => match o.calls.get_mut(&e.call) {
            Some(settled) if !*settled => {
                *settled = true;
                Ok(())
            }
            _ => fail(
                "R2",
                format!("foreign call {} settles, but it is not pending", e.call),
            ),
        },
        other => fail("I1", format!("unknown event `{other}`")),
    }
}

fn child_step(o: &mut Owner, e: &Event) -> Step {
    let Some(c) = o.children.get_mut(&e.fiber) else {
        return fail(
            "I3",
            format!(
                "{} of child {}, which was never registered",
                e.name, e.fiber
            ),
        );
    };
    match e.name.as_str() {
        "end" if c.exit.is_some() => fail("I3", "a child ends twice"),
        "end" => {
            c.exit = Some(e.exit.clone());
            Ok(())
        }
        "sup" if c.exit.as_deref().is_none_or(|x| x == "ok") || c.supervised => {
            fail("R4", "a supervisor called without a failed child, or twice")
        }
        "sup" => {
            c.supervised = true;
            Ok(())
        }
        _ if c.exit.is_none() || c.watched => {
            fail("I3", "a watcher done before its child ended, or twice")
        }
        _ if c.exit.as_deref() != Some("ok") && !c.supervised => fail(
            "R4",
            format!(
                "child {} failed, but its watcher is done without its supervisor",
                e.fiber
            ),
        ),
        _ => {
            c.watched = true;
            Ok(())
        }
    }
}

fn run(o: &mut Owner, e: &Event) -> Step {
    let phase = o.phase;
    let Some(f) = o.finalizers.get_mut(&e.place) else {
        return fail(
            "R1",
            format!("finalizer {} runs, but it was never added", e.place),
        );
    };
    if f.ran {
        return fail("R1", format!("finalizer {} runs twice", e.place));
    }
    f.ran = true;
    if f.now {
        return Ok(());
    }
    if phase != 6 {
        return fail(
            "R2",
            format!("finalizer {} of the stack runs outside C5", e.place),
        );
    }
    if o.last_run.is_some_and(|last| e.place > last) {
        return fail(
            "R2",
            format!(
                "finalizer {} runs after {}: the stack runs last added first",
                e.place,
                o.last_run.unwrap_or(0)
            ),
        );
    }
    o.last_run = Some(e.place);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::*;

    fn checked(lines: &[&str]) -> Result<(), Violation> {
        check(&parse(&lines.join("\n")).expect("a trace"))
    }

    const CLOSE: [&str; 7] = [
        r#"{"e":"phase","o":1,"c":"C0"}"#,
        r#"{"e":"phase","o":1,"c":"C1"}"#,
        r#"{"e":"phase","o":1,"c":"C2"}"#,
        r#"{"e":"phase","o":1,"c":"C3"}"#,
        r#"{"e":"phase","o":1,"c":"C4"}"#,
        r#"{"e":"phase","o":1,"c":"C5"}"#,
        r#"{"e":"phase","o":1,"c":"C6"}"#,
    ];

    #[test]
    fn a_close_with_a_child_and_two_finalizers_is_allowed() {
        let mut trace = vec![
            r#"{"e":"open","o":1,"p":0,"k":"main"}"#,
            r#"{"e":"fin","o":1,"i":1}"#,
            r#"{"e":"spawn","o":1,"f":2}"#,
            r#"{"e":"fin","o":1,"i":2}"#,
        ];
        trace.extend(&CLOSE[..3]);
        trace.extend([
            r#"{"e":"interrupt","o":1,"f":2}"#,
            r#"{"e":"end","o":1,"f":2,"x":"int"}"#,
            r#"{"e":"sup","o":1,"f":2}"#,
            r#"{"e":"watched","o":1,"f":2}"#,
        ]);
        trace.extend(&CLOSE[3..6]);
        trace.extend([r#"{"e":"run","o":1,"i":2}"#, r#"{"e":"run","o":1,"i":1}"#]);
        trace.extend([CLOSE[6], r#"{"e":"exit","x":"ok"}"#]);
        assert_eq!(checked(&trace), Ok(()));
    }

    #[test]
    fn the_stack_runs_last_added_first() {
        let mut trace = vec![
            r#"{"e":"open","o":1,"p":0,"k":"main"}"#,
            r#"{"e":"fin","o":1,"i":1}"#,
            r#"{"e":"fin","o":1,"i":2}"#,
        ];
        trace.extend(&CLOSE[..6]);
        trace.extend([r#"{"e":"run","o":1,"i":1}"#, r#"{"e":"run","o":1,"i":2}"#]);
        assert_eq!(checked(&trace).map_err(|v| v.rule), Err("R2"));
    }

    #[test]
    fn finalizers_wait_for_the_children() {
        let mut trace = vec![
            r#"{"e":"open","o":1,"p":0,"k":"main"}"#,
            r#"{"e":"spawn","o":1,"f":2}"#,
        ];
        trace.extend(&CLOSE[..4]);
        assert_eq!(checked(&trace).map_err(|v| v.rule), Err("R2"));
    }

    #[test]
    fn a_failed_child_reaches_its_supervisor() {
        let trace = [
            r#"{"e":"open","o":1,"p":0,"k":"main"}"#,
            r#"{"e":"spawn","o":1,"f":2}"#,
            r#"{"e":"end","o":1,"f":2,"x":"fail"}"#,
            r#"{"e":"watched","o":1,"f":2}"#,
        ];
        assert_eq!(checked(&trace).map_err(|v| v.rule), Err("R4"));
    }

    #[test]
    fn a_closing_owner_admits_no_child_and_a_closed_one_no_step() {
        let mut trace = vec![r#"{"e":"open","o":1,"p":0,"k":"scope"}"#, CLOSE[0]];
        trace.push(r#"{"e":"spawn","o":1,"f":2}"#);
        assert_eq!(checked(&trace).map_err(|v| v.rule), Err("I2"));
        let mut trace = vec![r#"{"e":"open","o":1,"p":0,"k":"scope"}"#];
        trace.extend(CLOSE);
        trace.push(r#"{"e":"track","o":1,"t":3}"#);
        assert_eq!(checked(&trace).map_err(|v| v.rule), Err("R3"));
    }

    #[test]
    fn every_finalizer_runs_before_the_close_ends() {
        let mut trace = vec![
            r#"{"e":"open","o":1,"p":0,"k":"scope"}"#,
            r#"{"e":"fin","o":1,"i":1}"#,
        ];
        trace.extend(CLOSE);
        let violation = checked(&trace).expect_err("a finalizer that never ran");
        assert_eq!((violation.rule, violation.line), ("R1", 9));
    }
}
