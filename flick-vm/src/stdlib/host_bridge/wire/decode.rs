//! Iterative flat-node wire decoder driver and cause wire decoder.

use serde_json::Value as Json;

use crate::tier0::causes::{CauseData, StoredReason};
use crate::tier0::{CauseToken, Data, DefectSpec, Error, Heap, Value};

use super::decode_nodes::build_decoded_node;

/// Decodes a flat `{ "root": idx, "nodes": [...] }` payload iteratively.
/// Any mid-decode error releases every partially allocated slot cleanly.
pub(crate) fn decode_value(heap: &mut Heap, execution: usize, wire: &Json) -> Result<Value, Error> {
    let root = wire
        .get("root")
        .and_then(Json::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| Error::HostBridge("wire payload missing root index".into()))?;
    let nodes = wire
        .get("nodes")
        .and_then(Json::as_array)
        .ok_or_else(|| Error::HostBridge("wire payload missing nodes array".into()))?;
    if root >= nodes.len() {
        return Err(Error::HostBridge("wire root index out of bounds".into()));
    }

    let mut remaining_uses = vec![0usize; nodes.len()];
    remaining_uses[root] = 1;
    let mut resolved: Vec<Option<Value>> = (0..nodes.len()).map(|_| None).collect();
    let mut state = vec![0u8; nodes.len()];
    let mut stack = vec![root];

    let result = decode_loop(
        heap,
        execution,
        nodes,
        &mut remaining_uses,
        &mut resolved,
        &mut state,
        &mut stack,
    );
    match result {
        Ok(()) => {
            let root_val = resolved[root]
                .take()
                .ok_or_else(|| Error::HostBridge("missing decoded root value".into()))?;
            let leftovers: Vec<Value> = resolved.into_iter().flatten().collect();
            if !leftovers.is_empty() {
                let _ = heap.drop_values(leftovers);
            }
            Ok(root_val)
        }
        Err(err) => {
            let partial: Vec<Value> = resolved.into_iter().flatten().collect();
            let _ = heap.drop_values(partial);
            Err(err)
        }
    }
}

fn decode_loop(
    heap: &mut Heap,
    execution: usize,
    nodes: &[Json],
    uses: &mut [usize],
    resolved: &mut [Option<Value>],
    state: &mut [u8],
    stack: &mut Vec<usize>,
) -> Result<(), Error> {
    while let Some(&idx) = stack.last() {
        if state[idx] == 2 {
            stack.pop();
            continue;
        }
        let node = nodes[idx]
            .as_array()
            .ok_or_else(|| Error::HostBridge("wire node is not an array".into()))?;
        let kind = node
            .first()
            .and_then(Json::as_str)
            .ok_or_else(|| Error::HostBridge("wire node missing tag".into()))?;
        if state[idx] == 0 {
            state[idx] = 1;
            let mut pushed = false;
            push_decode_children(kind, node, nodes.len(), uses, state, stack, &mut pushed)?;
            if pushed {
                continue;
            }
        }
        stack.pop();
        state[idx] = 2;
        let val = build_decoded_node(heap, execution, kind, node, uses, resolved)?;
        resolved[idx] = Some(val);
    }
    Ok(())
}

pub(super) fn wire_idx(val: &Json, len: usize) -> Result<usize, Error> {
    let idx = val
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| Error::HostBridge("invalid wire child index".into()))?;
    if idx >= len {
        return Err(Error::HostBridge("wire child index out of bounds".into()));
    }
    Ok(idx)
}

fn push_decode_children(
    kind: &str,
    node: &[Json],
    len: usize,
    uses: &mut [usize],
    state: &[u8],
    stack: &mut Vec<usize>,
    pushed: &mut bool,
) -> Result<(), Error> {
    let mut queue_child = |c: usize| {
        uses[c] += 1;
        if state[c] == 0 {
            stack.push(c);
            *pushed = true;
        }
    };
    match kind {
        "list" | "tuple" => {
            let items = node
                .get(1)
                .and_then(Json::as_array)
                .ok_or_else(|| Error::HostBridge("list/tuple node missing items".into()))?;
            for item in items.iter().rev() {
                queue_child(wire_idx(item, len)?);
            }
        }
        "rec" | "dict" => {
            let pairs = node
                .get(1)
                .and_then(Json::as_array)
                .ok_or_else(|| Error::HostBridge("record node missing pairs".into()))?;
            for pair in pairs.iter().rev() {
                let arr = pair
                    .as_array()
                    .ok_or_else(|| Error::HostBridge("record entry is not a pair".into()))?;
                queue_child(wire_idx(
                    arr.get(1)
                        .ok_or_else(|| Error::HostBridge("record pair missing val".into()))?,
                    len,
                )?);
            }
        }
        "tag" => {
            let fields = node
                .get(2)
                .and_then(Json::as_array)
                .ok_or_else(|| Error::HostBridge("tag node missing fields".into()))?;
            for f in fields.iter().rev() {
                queue_child(wire_idx(f, len)?);
            }
        }
        "exit_ok" | "exit_fail" => {
            queue_child(wire_idx(
                node.get(1)
                    .ok_or_else(|| Error::HostBridge("exit node missing inner".into()))?,
                len,
            )?);
        }
        "cause" => {
            let reasons = node
                .get(1)
                .and_then(Json::as_array)
                .ok_or_else(|| Error::HostBridge("cause node missing reasons".into()))?;
            for r in reasons.iter().rev() {
                let arr = r
                    .as_array()
                    .ok_or_else(|| Error::HostBridge("cause reason not array".into()))?;
                if matches!(arr.first().and_then(Json::as_str), Some("fail" | "die_val")) {
                    queue_child(wire_idx(
                        arr.get(1)
                            .ok_or_else(|| Error::HostBridge("cause reason missing val".into()))?,
                        len,
                    )?);
                }
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn take_child(
    heap: &mut Heap,
    idx: usize,
    uses: &mut [usize],
    resolved: &mut [Option<Value>],
) -> Result<Value, Error> {
    if uses[idx] <= 1 {
        uses[idx] = 0;
        resolved[idx]
            .take()
            .ok_or_else(|| Error::HostBridge("missing decoded child".into()))
    } else {
        uses[idx] -= 1;
        let rep = resolved[idx]
            .as_ref()
            .ok_or_else(|| Error::HostBridge("missing decoded child".into()))?
            .representation();
        heap.dup(&rep)
    }
}

/// Decodes a `worker.mjs` `{ "reasons": [...] }` wire cause object into a `CauseToken`.
pub(crate) fn decode_cause_wire(
    heap: &mut Heap,
    execution: usize,
    cause_json: &Json,
) -> Result<CauseToken, Error> {
    let reasons = cause_json
        .get("reasons")
        .and_then(Json::as_array)
        .ok_or_else(|| Error::HostBridge("cause_wire missing reasons".into()))?;
    let mut stored = Vec::with_capacity(reasons.len());
    for r in reasons {
        let arr = r
            .as_array()
            .ok_or_else(|| Error::HostBridge("cause_wire reason not array".into()))?;
        match arr.first().and_then(Json::as_str) {
            Some("fail") => {
                let wire = arr
                    .get(1)
                    .ok_or_else(|| Error::HostBridge("cause_wire fail missing payload".into()))?;
                match decode_value(heap, execution, wire) {
                    Ok(v) => stored.push(StoredReason::Fail(v)),
                    Err(e) => {
                        let _ = heap.drop_values(
                            stored
                                .into_iter()
                                .filter_map(StoredReason::into_value)
                                .collect(),
                        );
                        return Err(e);
                    }
                }
            }
            Some("die") => {
                let msg = arr.get(1).and_then(Json::as_str).unwrap_or("host defect");
                let defect = heap.allocate(Data::Defect(DefectSpec::custom(msg)));
                stored.push(StoredReason::Die(defect));
            }
            Some("interrupt") => {
                let fid = arr
                    .get(1)
                    .and_then(Json::as_u64)
                    .and_then(|v| usize::try_from(v).ok());
                stored.push(StoredReason::Interrupt { fiber_id: fid });
            }
            _ => {
                let _ = heap.drop_values(
                    stored
                        .into_iter()
                        .filter_map(StoredReason::into_value)
                        .collect(),
                );
                return Err(Error::HostBridge("unknown cause_wire reason".into()));
            }
        }
    }
    let val = heap.allocate(Data::Cause(CauseData::plain(stored)));
    CauseToken::from_cause_value(heap, val)
}
