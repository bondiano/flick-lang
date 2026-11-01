//! The projection of a trace: what the JS and Go runs of one program must
//! agree on. Owners are named by their place in the tree of owners, each
//! among its siblings in the order they opened; for each, its kind, how
//! far its close got, how its children ended, and its finalizers: added to
//! the stack or run at once, and the order they ran in. Fiber ids, the
//! global order of steps and foreign calls (Go tracks a call only once it
//! is interrupted) are left out, and so are the interruptions of C2, which
//! reach a child only if it has not ended yet.
//!
//! shortcut: an owner opened by a fiber outside the tree of main (p = 0) is a root of its own, named by the order roots opened; name it by its spawner when such a case appears

use std::collections::BTreeMap;
use std::fmt::Write;

use super::Event;

#[derive(Default)]
struct Owner {
    name: String,
    kind: String,
    opened: usize,
    phase: String,
    spawned: usize,
    refused: usize,
    exits: BTreeMap<String, usize>,
    supervised: usize,
    added: Vec<&'static str>,
    ran: Vec<i64>,
}

/// The projection of `events`, one line per owner in the order of their
/// names, then the layers and how main ended.
pub fn project(events: &[Event]) -> String {
    let mut owners: BTreeMap<i64, Owner> = BTreeMap::new();
    let mut roots = 0;
    let mut builds: BTreeMap<i64, (usize, usize)> = BTreeMap::new();
    let mut layer_order: Vec<i64> = Vec::new();
    let mut exit = String::from("none");
    for e in events {
        match e.name.as_str() {
            "open" => {
                let name = if let Some(parent) = owners.get_mut(&e.parent) {
                    parent.opened += 1;
                    format!("{}.{}", parent.name, parent.opened)
                } else {
                    roots += 1;
                    roots.to_string()
                };
                owners.insert(
                    e.owner,
                    Owner {
                        name,
                        kind: e.kind.clone(),
                        phase: "open".into(),
                        ..Owner::default()
                    },
                );
            }
            "build" | "release" => {
                if !layer_order.contains(&e.layer) {
                    layer_order.push(e.layer);
                }
                let counts = builds.entry(e.layer).or_default();
                if e.name == "build" {
                    counts.0 += 1;
                } else {
                    counts.1 += 1;
                }
            }
            "exit" => exit.clone_from(&e.exit),
            _ => {
                if let Some(o) = owners.get_mut(&e.owner) {
                    owner_step(o, e);
                }
            }
        }
    }
    let mut lines: Vec<String> = owners.values().map(line).collect();
    lines.sort();
    for (n, l) in layer_order.iter().enumerate() {
        let (built, released) = builds[l];
        lines.push(format!(
            "layer {}: built {built}, released {released}",
            n + 1
        ));
    }
    lines.push(format!("exit {exit}"));
    lines.join("\n") + "\n"
}

fn owner_step(o: &mut Owner, e: &Event) {
    match e.name.as_str() {
        "phase" => o.phase.clone_from(&e.phase),
        "spawn" => o.spawned += 1,
        "refuse" => o.refused += 1,
        "end" => *o.exits.entry(e.exit.clone()).or_default() += 1,
        "sup" => o.supervised += 1,
        "fin" => o.added.push("fin"),
        "now" => o.added.push("now"),
        "run" => o.ran.push(e.place),
        _ => {}
    }
}

fn line(o: &Owner) -> String {
    let mut out = format!("owner {} {}: {}", o.name, o.kind, o.phase);
    if o.spawned > 0 || o.refused > 0 {
        let exits: Vec<String> = o.exits.iter().map(|(x, n)| format!("{n} {x}")).collect();
        let _ = write!(
            out,
            ", children {} ({}), supervised {}, refused {}",
            o.spawned,
            exits.join(", "),
            o.supervised,
            o.refused
        );
    }
    if !o.added.is_empty() {
        let ran: Vec<String> = o.ran.iter().map(i64::to_string).collect();
        let _ = write!(
            out,
            ", finalizers {} ran {}",
            o.added.join(" "),
            ran.join(" ")
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::parse;
    use super::*;

    #[test]
    fn owners_are_named_by_their_place_and_ids_are_left_out() {
        let js = r#"{"e":"open","o":1,"p":0,"k":"main"}
{"e":"open","o":2,"p":1,"k":"scope"}
{"e":"spawn","o":2,"f":3}
{"e":"track","o":2,"t":4}
{"e":"fin","o":2,"i":1}
{"e":"end","o":2,"f":3,"x":"ok"}
{"e":"phase","o":2,"c":"C5"}
{"e":"run","o":2,"i":1}
{"e":"exit","x":"ok"}"#;
        let go = r#"{"e":"open","o":7,"p":0,"k":"main"}
{"e":"open","o":9,"p":7,"k":"scope"}
{"e":"spawn","o":9,"f":11}
{"e":"fin","o":9,"i":1}
{"e":"end","o":9,"f":11,"x":"ok"}
{"e":"phase","o":9,"c":"C5"}
{"e":"run","o":9,"i":1}
{"e":"exit","x":"ok"}"#;
        let (js, go) = (
            project(&parse(js).expect("js")),
            project(&parse(go).expect("go")),
        );
        assert_eq!(js, go);
        assert_eq!(
            js,
            "owner 1 main: open\nowner 1.1 scope: C5, children 1 (1 ok), supervised 0, refused 0, finalizers fin ran 1\nexit ok\n"
        );
    }
}
