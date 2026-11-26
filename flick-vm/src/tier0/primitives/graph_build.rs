//! Counted `Graph` constructors, snapshots, and node/edge mutation primitives.

use std::collections::{BTreeMap, BTreeSet};
use std::mem;

use flick_core::ir::Lit;

use super::graph_ops::{
    alloc_graph, clone_graph_filtered, drop_partial, edge_at, expect_graph, expect_graph_id,
    expect_int, graph_err, hydrate, missing_node_msg, new_graph, node_at,
};
use super::list_ops::slice_reps;
use super::{Data, Error, GraphData, GraphEdge, GraphNode, Heap, Value, charge};

pub(super) fn make(heap: &mut Heap, kind: &Value) -> Result<Value, Error> {
    let s = heap.string_lossy(kind)?;
    Ok(alloc_graph(heap, new_graph(s == "directed")))
}

pub(super) fn from_snapshot(heap: &mut Heap, snap: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let kind = heap.string_lossy(heap.named_field(snap, "graph_type")?)?;
    if kind != "directed" && kind != "undirected" {
        return graph_err(heap, "Snapshot type must be directed or undirected");
    }
    let node_reps = slice_reps(heap, heap.named_field(snap, "nodes")?)?;
    let edge_reps = slice_reps(heap, heap.named_field(snap, "edges")?)?;
    charge(fuel, node_reps.len() + edge_reps.len())?;
    let mut prev = -1i32;
    let mut known = BTreeSet::new();
    let mut nodes = Vec::with_capacity(node_reps.len());
    for (i, n) in node_reps.iter().enumerate() {
        let at = expect_int(heap.named_field(n, "index")?)?;
        if at < 0 {
            drop_partial(heap, nodes, Vec::new())?;
            return graph_err(
                heap,
                &format!("Node index at position {i} must be a non-negative safe integer"),
            );
        }
        if at <= prev {
            drop_partial(heap, nodes, Vec::new())?;
            return graph_err(heap, "Node indexes must be strictly increasing");
        }
        prev = at;
        known.insert(at);
        let data_rep = heap.named_field(n, "data")?.representation();
        match heap.dup(&data_rep) {
            Ok(data) => nodes.push(GraphNode { index: at, data }),
            Err(err) => {
                drop_partial(heap, nodes, Vec::new())?;
                return Err(err);
            }
        }
    }
    let mut prev_e = -1i32;
    let mut edges = Vec::with_capacity(edge_reps.len());
    for (i, e) in edge_reps.iter().enumerate() {
        let at = expect_int(heap.named_field(e, "index")?)?;
        let src = expect_int(heap.named_field(e, "source")?)?;
        let dst = expect_int(heap.named_field(e, "target")?)?;
        let bad = if at < 0 {
            Some(format!(
                "Edge index at position {i} must be a non-negative safe integer"
            ))
        } else if at <= prev_e {
            Some("Edge indexes must be strictly increasing".into())
        } else if src < 0 {
            Some(format!(
                "Edge source at position {i} must be a non-negative safe integer"
            ))
        } else if !known.contains(&src) {
            Some(format!("Edge source {src} does not reference a node"))
        } else if dst < 0 {
            Some(format!(
                "Edge target at position {i} must be a non-negative safe integer"
            ))
        } else if !known.contains(&dst) {
            Some(format!("Edge target {dst} does not reference a node"))
        } else {
            None
        };
        if let Some(msg) = bad {
            drop_partial(heap, nodes, edges)?;
            return graph_err(heap, &msg);
        }
        prev_e = at;
        let data_rep = heap.named_field(e, "data")?.representation();
        match heap.dup(&data_rep) {
            Ok(data) => edges.push(GraphEdge {
                index: at,
                source: src,
                target: dst,
                data,
            }),
            Err(err) => {
                drop_partial(heap, nodes, edges)?;
                return Err(err);
            }
        }
    }
    let g = alloc_graph(
        heap,
        hydrate(GraphData {
            directed: kind == "directed",
            nodes,
            edges,
            adj: BTreeMap::new(),
            radj: BTreeMap::new(),
            next_node: 0,
            next_edge: 0,
        }),
    );
    heap.construct("Prelude.Result.Ok", vec![g])
}

pub(super) fn to_snapshot(heap: &mut Heap, val: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let gid = expect_graph_id(heap, val)?;
    let (directed, node_reps, edge_reps) = {
        let Data::Graph(g) = &heap.object(gid)?.data else {
            return Err(Error::PatternKind("Graph"));
        };
        let nr: Vec<(i32, Value)> = g
            .nodes
            .iter()
            .map(|n| (n.index, n.data.representation()))
            .collect();
        let er: Vec<(i32, i32, i32, Value)> = g
            .edges
            .iter()
            .map(|e| (e.index, e.source, e.target, e.data.representation()))
            .collect();
        (g.directed, nr, er)
    };
    charge(fuel, node_reps.len() + edge_reps.len())?;
    let mut snap_nodes = Vec::with_capacity(node_reps.len());
    for (index, rep) in node_reps {
        let data = heap.dup(&rep)?;
        snap_nodes.push(heap.record(vec![
            ("index".into(), Value::Int(index)),
            ("data".into(), data),
        ])?);
    }
    let nodes = heap.construct("$list", snap_nodes)?;
    let mut snap_edges = Vec::with_capacity(edge_reps.len());
    for (index, source, target, rep) in edge_reps {
        let data = match heap.dup(&rep) {
            Ok(d) => d,
            Err(err) => {
                heap.drop_value(nodes)?;
                heap.drop_values(snap_edges)?;
                return Err(err);
            }
        };
        snap_edges.push(heap.record(vec![
            ("index".into(), Value::Int(index)),
            ("source".into(), Value::Int(source)),
            ("target".into(), Value::Int(target)),
            ("data".into(), data),
        ])?);
    }
    let edges = heap.construct("$list", snap_edges)?;
    let kind = heap.literal(&Lit::String(
        if directed { "directed" } else { "undirected" }.into(),
    ))?;
    heap.record(vec![
        ("graph_type".into(), kind),
        ("nodes".into(), nodes),
        ("edges".into(), edges),
    ])
}

pub(super) fn add_node(
    heap: &mut Heap,
    data: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let gid = expect_graph_id(heap, val)?;
    let g = expect_graph(heap, val)?;
    charge(fuel, g.nodes.len() + g.edges.len())?;
    let mut next = clone_graph_filtered(heap, gid, |_, _| true, |_, _, _, _| true)?;
    let idx = next.next_node;
    let owned = match heap.dup(data) {
        Ok(v) => v,
        Err(err) => {
            drop_partial(heap, next.nodes, next.edges)?;
            return Err(err);
        }
    };
    next.nodes.push(GraphNode {
        index: idx,
        data: owned,
    });
    next.adj.insert(idx, Vec::new());
    next.radj.insert(idx, Vec::new());
    next.next_node += 1;
    let gv = alloc_graph(heap, next);
    heap.construct("$tuple", vec![Value::Int(idx), gv])
}

pub(super) fn add_edge(
    heap: &mut Heap,
    src_val: &Value,
    dst_val: &Value,
    data: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let src = expect_int(src_val)?;
    let dst = expect_int(dst_val)?;
    let gid = expect_graph_id(heap, val)?;
    let g = expect_graph(heap, val)?;
    if node_at(g, src).is_none() {
        return graph_err(heap, &missing_node_msg(src));
    }
    if node_at(g, dst).is_none() {
        return graph_err(heap, &missing_node_msg(dst));
    }
    charge(fuel, g.nodes.len() + g.edges.len())?;
    let mut next = clone_graph_filtered(heap, gid, |_, _| true, |_, _, _, _| true)?;
    let idx = next.next_edge;
    let owned = match heap.dup(data) {
        Ok(v) => v,
        Err(err) => {
            drop_partial(heap, next.nodes, next.edges)?;
            return Err(err);
        }
    };
    next.edges.push(GraphEdge {
        index: idx,
        source: src,
        target: dst,
        data: owned,
    });
    next.adj.entry(src).or_default().push(idx);
    next.radj.entry(dst).or_default().push(idx);
    if !next.directed {
        next.adj.entry(dst).or_default().push(idx);
        next.radj.entry(src).or_default().push(idx);
    }
    next.next_edge += 1;
    let gv = alloc_graph(heap, next);
    let pair = heap.construct("$tuple", vec![Value::Int(idx), gv])?;
    heap.construct("Prelude.Result.Ok", vec![pair])
}

pub(super) fn remove_node(
    heap: &mut Heap,
    idx_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let idx = expect_int(idx_val)?;
    let gid = expect_graph_id(heap, val)?;
    let g = expect_graph(heap, val)?;
    charge(fuel, g.nodes.len() + g.edges.len())?;
    let out = clone_graph_filtered(
        heap,
        gid,
        |_, n| n != idx,
        |_, _, s, t| s != idx && t != idx,
    )?;
    Ok(alloc_graph(heap, out))
}

pub(super) fn remove_nodes(
    heap: &mut Heap,
    list_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let reps = slice_reps(heap, list_val)?;
    let mut drop_set = BTreeSet::new();
    for r in &reps {
        drop_set.insert(expect_int(r)?);
    }
    let gid = expect_graph_id(heap, val)?;
    let g = expect_graph(heap, val)?;
    charge(fuel, reps.len() + g.nodes.len() + g.edges.len())?;
    let out = clone_graph_filtered(
        heap,
        gid,
        |_, n| !drop_set.contains(&n),
        |_, _, s, t| !drop_set.contains(&s) && !drop_set.contains(&t),
    )?;
    Ok(alloc_graph(heap, out))
}

pub(super) fn remove_edge(
    heap: &mut Heap,
    idx_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let idx = expect_int(idx_val)?;
    let gid = expect_graph_id(heap, val)?;
    let g = expect_graph(heap, val)?;
    charge(fuel, g.nodes.len() + g.edges.len())?;
    let out = clone_graph_filtered(heap, gid, |_, _| true, |_, e, _, _| e != idx)?;
    Ok(alloc_graph(heap, out))
}

pub(super) fn remove_edges(
    heap: &mut Heap,
    list_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let reps = slice_reps(heap, list_val)?;
    let mut drop_set = BTreeSet::new();
    for r in &reps {
        drop_set.insert(expect_int(r)?);
    }
    let gid = expect_graph_id(heap, val)?;
    let g = expect_graph(heap, val)?;
    charge(fuel, reps.len() + g.nodes.len() + g.edges.len())?;
    let out = clone_graph_filtered(heap, gid, |_, _| true, |_, e, _, _| !drop_set.contains(&e))?;
    Ok(alloc_graph(heap, out))
}

pub(super) fn update_node_data(
    heap: &mut Heap,
    idx_val: &Value,
    new_data: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let idx = expect_int(idx_val)?;
    let gid = expect_graph_id(heap, val)?;
    let g = expect_graph(heap, val)?;
    charge(fuel, g.nodes.len() + g.edges.len())?;
    let mut out = clone_graph_filtered(heap, gid, |_, _| true, |_, _, _, _| true)?;
    if let Some(p) = node_at(&out, idx) {
        let replacement = match heap.dup(new_data) {
            Ok(v) => v,
            Err(err) => {
                drop_partial(heap, out.nodes, out.edges)?;
                return Err(err);
            }
        };
        let old = mem::replace(&mut out.nodes[p].data, replacement);
        heap.drop_value(old)?;
    }
    Ok(alloc_graph(heap, out))
}

pub(super) fn update_edge_data(
    heap: &mut Heap,
    idx_val: &Value,
    new_data: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let idx = expect_int(idx_val)?;
    let gid = expect_graph_id(heap, val)?;
    let g = expect_graph(heap, val)?;
    charge(fuel, g.nodes.len() + g.edges.len())?;
    let mut out = clone_graph_filtered(heap, gid, |_, _| true, |_, _, _, _| true)?;
    if let Some(p) = edge_at(&out, idx) {
        let replacement = match heap.dup(new_data) {
            Ok(v) => v,
            Err(err) => {
                drop_partial(heap, out.nodes, out.edges)?;
                return Err(err);
            }
        };
        let old = mem::replace(&mut out.edges[p].data, replacement);
        heap.drop_value(old)?;
    }
    Ok(alloc_graph(heap, out))
}

pub(super) fn replace_nodes(
    heap: &mut Heap,
    data_list: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let new_reps = slice_reps(heap, data_list)?;
    let gid = expect_graph_id(heap, val)?;
    let (directed, next_node, next_edge, node_idxs, edge_reps) = {
        let Data::Graph(g) = &heap.object(gid)?.data else {
            return Err(Error::PatternKind("Graph"));
        };
        (
            g.directed,
            g.next_node,
            g.next_edge,
            g.nodes.iter().map(|n| n.index).collect::<Vec<_>>(),
            g.edges
                .iter()
                .map(|e| (e.index, e.source, e.target, e.data.representation()))
                .collect::<Vec<_>>(),
        )
    };
    charge(fuel, node_idxs.len() + edge_reps.len())?;
    let mut nodes = Vec::with_capacity(node_idxs.len());
    for (idx, rep) in node_idxs.into_iter().zip(new_reps.iter()) {
        match heap.dup(rep) {
            Ok(data) => nodes.push(GraphNode { index: idx, data }),
            Err(err) => {
                drop_partial(heap, nodes, Vec::new())?;
                return Err(err);
            }
        }
    }
    let mut edges = Vec::with_capacity(edge_reps.len());
    for (index, source, target, rep) in edge_reps {
        match heap.dup(&rep) {
            Ok(data) => edges.push(GraphEdge {
                index,
                source,
                target,
                data,
            }),
            Err(err) => {
                drop_partial(heap, nodes, edges)?;
                return Err(err);
            }
        }
    }
    let mut out = hydrate(GraphData {
        directed,
        nodes,
        edges,
        adj: BTreeMap::new(),
        radj: BTreeMap::new(),
        next_node,
        next_edge,
    });
    out.next_node = out.next_node.max(next_node);
    out.next_edge = out.next_edge.max(next_edge);
    Ok(alloc_graph(heap, out))
}

pub(super) fn replace_edges(
    heap: &mut Heap,
    data_list: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let new_reps = slice_reps(heap, data_list)?;
    let gid = expect_graph_id(heap, val)?;
    let (directed, next_node, next_edge, node_reps, edge_meta) = {
        let Data::Graph(g) = &heap.object(gid)?.data else {
            return Err(Error::PatternKind("Graph"));
        };
        (
            g.directed,
            g.next_node,
            g.next_edge,
            g.nodes
                .iter()
                .map(|n| (n.index, n.data.representation()))
                .collect::<Vec<_>>(),
            g.edges
                .iter()
                .map(|e| (e.index, e.source, e.target))
                .collect::<Vec<_>>(),
        )
    };
    charge(fuel, node_reps.len() + edge_meta.len())?;
    let mut nodes = Vec::with_capacity(node_reps.len());
    for (index, rep) in node_reps {
        match heap.dup(&rep) {
            Ok(data) => nodes.push(GraphNode { index, data }),
            Err(err) => {
                drop_partial(heap, nodes, Vec::new())?;
                return Err(err);
            }
        }
    }
    let mut edges = Vec::with_capacity(edge_meta.len());
    for ((index, source, target), rep) in edge_meta.into_iter().zip(new_reps.iter()) {
        match heap.dup(rep) {
            Ok(data) => edges.push(GraphEdge {
                index,
                source,
                target,
                data,
            }),
            Err(err) => {
                drop_partial(heap, nodes, edges)?;
                return Err(err);
            }
        }
    }
    let mut out = hydrate(GraphData {
        directed,
        nodes,
        edges,
        adj: BTreeMap::new(),
        radj: BTreeMap::new(),
        next_node,
        next_edge,
    });
    out.next_node = out.next_node.max(next_node);
    out.next_edge = out.next_edge.max(next_edge);
    Ok(alloc_graph(heap, out))
}
