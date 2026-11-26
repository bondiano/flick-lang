//! Counted `Graph` filter/map, reverse, disjoint sum, neighborhood, and induced subgraph ops.

use std::collections::{BTreeMap, BTreeSet};
use std::mem;

use super::graph_ops::{
    alloc_graph, clone_graph_filtered, direction_of, drop_partial, expect_graph, expect_graph_id,
    expect_int, graph_defect, graph_err, hydrate, missing_node_msg, node_at, opt_inner,
};
use super::graph_walk::{bfs_positions, traversal_radius};
use super::list_ops::slice_reps;
use super::{Data, Error, Fuel, GraphData, GraphEdge, GraphNode, Heap, Value, charge};

pub(super) fn filter_map_nodes_apply(
    heap: &mut Heap,
    opts_list: &Value,
    val: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let opt_reps = slice_reps(heap, opts_list)?;
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
    let mut kept = BTreeSet::new();
    let mut nodes = Vec::new();
    for (idx, opt_v) in node_idxs.into_iter().zip(opt_reps.iter()) {
        if let Some(inner) = opt_inner(heap, opt_v)? {
            let rep = inner.representation();
            match heap.dup(&rep) {
                Ok(data) => {
                    kept.insert(idx);
                    nodes.push(GraphNode { index: idx, data });
                }
                Err(err) => {
                    drop_partial(heap, nodes, Vec::new())?;
                    return Err(err);
                }
            }
        }
    }
    let mut edges = Vec::new();
    for (index, source, target, rep) in edge_reps {
        if kept.contains(&source) && kept.contains(&target) {
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

pub(super) fn filter_map_edges_apply(
    heap: &mut Heap,
    opts_list: &Value,
    val: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let opt_reps = slice_reps(heap, opts_list)?;
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
    let mut edges = Vec::new();
    for ((index, source, target), opt_v) in edge_meta.into_iter().zip(opt_reps.iter()) {
        if let Some(inner) = opt_inner(heap, opt_v)? {
            let rep = inner.representation();
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

pub(super) fn reverse(heap: &mut Heap, val: &Value, fuel: Fuel<'_>) -> Result<Value, Error> {
    let gid = expect_graph_id(heap, val)?;
    let g = expect_graph(heap, val)?;
    charge(fuel, g.nodes.len() + g.edges.len())?;
    let mut out = clone_graph_filtered(heap, gid, |_, _| true, |_, _, _, _| true)?;
    if out.directed {
        for e in &mut out.edges {
            mem::swap(&mut e.source, &mut e.target);
        }
        mem::swap(&mut out.adj, &mut out.radj);
    }
    Ok(alloc_graph(heap, out))
}

pub(super) fn sum(
    heap: &mut Heap,
    that_val: &Value,
    self_val: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let a_id = expect_graph_id(heap, self_val)?;
    let b_id = expect_graph_id(heap, that_val)?;
    let directed = {
        let a = expect_graph(heap, self_val)?;
        let b = expect_graph(heap, that_val)?;
        if a.directed != b.directed {
            return Err(graph_defect(&format!(
                "Cannot combine {} and {} graphs",
                if a.directed { "directed" } else { "undirected" },
                if b.directed { "directed" } else { "undirected" }
            )));
        }
        charge(
            fuel,
            a.nodes.len() + a.edges.len() + b.nodes.len() + b.edges.len(),
        )?;
        a.directed
    };
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut next_node = 0i32;
    let mut next_edge = 0i32;
    for gid in [a_id, b_id] {
        let (nr, er) = {
            let Data::Graph(g) = &heap.object(gid)?.data else {
                return Err(Error::PatternKind("Graph"));
            };
            (
                g.nodes
                    .iter()
                    .map(|n| (n.index, n.data.representation()))
                    .collect::<Vec<_>>(),
                g.edges
                    .iter()
                    .map(|e| (e.source, e.target, e.data.representation()))
                    .collect::<Vec<_>>(),
            )
        };
        let mut moved = BTreeMap::new();
        for (old, rep) in nr {
            let data = match heap.dup(&rep) {
                Ok(d) => d,
                Err(err) => {
                    drop_partial(heap, nodes, edges)?;
                    return Err(err);
                }
            };
            let idx = next_node;
            next_node += 1;
            moved.insert(old, idx);
            nodes.push(GraphNode { index: idx, data });
        }
        for (s, t, rep) in er {
            let data = match heap.dup(&rep) {
                Ok(d) => d,
                Err(err) => {
                    drop_partial(heap, nodes, edges)?;
                    return Err(err);
                }
            };
            let idx = next_edge;
            next_edge += 1;
            edges.push(GraphEdge {
                index: idx,
                source: moved[&s],
                target: moved[&t],
                data,
            });
        }
    }
    Ok(alloc_graph(
        heap,
        hydrate(GraphData {
            directed,
            nodes,
            edges,
            adj: BTreeMap::new(),
            radj: BTreeMap::new(),
            next_node: 0,
            next_edge: 0,
        }),
    ))
}

pub(super) fn neighborhood(
    heap: &mut Heap,
    idx_val: &Value,
    opts: &Value,
    val: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let start = expect_int(idx_val)?;
    let radius = match traversal_radius(heap, heap.named_field(opts, "radius")?, 1.0)? {
        Ok(r) => r,
        Err(msg) => return graph_err(heap, &msg),
    };
    let dir = direction_of(heap, heap.named_field(opts, "direction")?)?;
    let gid = expect_graph_id(heap, val)?;
    let (directed, node_reps, edge_reps) = {
        let Data::Graph(g) = &heap.object(gid)?.data else {
            return Err(Error::PatternKind("Graph"));
        };
        let Some(sp) = node_at(g, start) else {
            return graph_err(heap, &missing_node_msg(start));
        };
        charge(fuel, g.nodes.len() + g.edges.len())?;
        let reached = bfs_positions(g, &[sp], &dir, radius);
        let mut moved = BTreeMap::new();
        let mut nr = Vec::with_capacity(reached.len());
        for (new_idx, p) in reached.into_iter().enumerate() {
            let ni = i32::try_from(new_idx).unwrap_or(0);
            moved.insert(g.nodes[p].index, ni);
            nr.push((ni, g.nodes[p].data.representation()));
        }
        let mut er = Vec::new();
        for e in &g.edges {
            if let (Some(&s), Some(&t)) = (moved.get(&e.source), moved.get(&e.target)) {
                er.push((s, t, e.data.representation()));
            }
        }
        (g.directed, nr, er)
    };
    let mut nodes = Vec::with_capacity(node_reps.len());
    for (index, rep) in node_reps {
        let data = match heap.dup(&rep) {
            Ok(d) => d,
            Err(err) => {
                drop_partial(heap, nodes, Vec::new())?;
                return Err(err);
            }
        };
        nodes.push(GraphNode { index, data });
    }
    let mut edges = Vec::with_capacity(edge_reps.len());
    for (i, (source, target, rep)) in edge_reps.into_iter().enumerate() {
        let data = match heap.dup(&rep) {
            Ok(d) => d,
            Err(err) => {
                drop_partial(heap, nodes, edges)?;
                return Err(err);
            }
        };
        edges.push(GraphEdge {
            index: i32::try_from(i).unwrap_or(0),
            source,
            target,
            data,
        });
    }
    let g = alloc_graph(
        heap,
        hydrate(GraphData {
            directed,
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

pub(super) fn induced_subgraph(
    heap: &mut Heap,
    list_val: &Value,
    val: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let reps = slice_reps(heap, list_val)?;
    let gid = expect_graph_id(heap, val)?;
    let mut selected = BTreeSet::new();
    {
        let g = expect_graph(heap, val)?;
        for r in &reps {
            let idx = expect_int(r)?;
            if node_at(g, idx).is_none() {
                return graph_err(heap, &missing_node_msg(idx));
            }
            selected.insert(idx);
        }
        charge(fuel, reps.len() + g.nodes.len() + g.edges.len())?;
    }
    let out = clone_graph_filtered(
        heap,
        gid,
        |_, n| selected.contains(&n),
        |_, _, s, t| selected.contains(&s) && selected.contains(&t),
    )?;
    let out = hydrate(out);
    let gv = alloc_graph(heap, out);
    heap.construct("Prelude.Result.Ok", vec![gv])
}
