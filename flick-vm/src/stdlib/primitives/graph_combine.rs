//! Counted `Graph` set-combine and complement primitives (`runtime-go/flickrt/graph_combine.go`).

use std::collections::BTreeMap;

use super::graph_ops::{
    alloc_graph, drop_partial, expect_graph, expect_graph_id, graph_defect, hydrate, rows,
};
use super::list_ops::slice_reps;
use super::{Data, Error, Fuel, GraphData, GraphEdge, GraphNode, Heap, Value, charge};

struct NodeMap {
    keys: Vec<Value>,
    vals: Vec<Value>,
}

impl NodeMap {
    fn new() -> Self {
        Self {
            keys: Vec::new(),
            vals: Vec::new(),
        }
    }
    fn find(&self, heap: &Heap, k: &Value, fuel: Fuel<'_>) -> Result<Option<usize>, Error> {
        for (i, existing) in self.keys.iter().enumerate() {
            charge(fuel, 1)?;
            if super::equal(heap, existing, k)? {
                return Ok(Some(i));
            }
        }
        Ok(None)
    }
    fn set(&mut self, heap: &Heap, k: Value, v: Value, fuel: Fuel<'_>) -> Result<(), Error> {
        if let Some(i) = self.find(heap, &k, fuel)? {
            self.vals[i] = v;
        } else {
            self.keys.push(k);
            self.vals.push(v);
        }
        Ok(())
    }
}

struct EdgeId {
    source: Value,
    target: Value,
    identity: Value,
}

struct EdgeMap {
    directed: bool,
    keys: Vec<EdgeId>,
    vals: Vec<Value>,
}

impl EdgeMap {
    fn new(directed: bool) -> Self {
        Self {
            directed,
            keys: Vec::new(),
            vals: Vec::new(),
        }
    }
    fn eq_id(&self, heap: &Heap, a: &EdgeId, b: &EdgeId) -> Result<bool, Error> {
        if !super::equal(heap, &a.identity, &b.identity)? {
            return Ok(false);
        }
        let same =
            super::equal(heap, &a.source, &b.source)? && super::equal(heap, &a.target, &b.target)?;
        Ok(same
            || (!self.directed
                && super::equal(heap, &a.source, &b.target)?
                && super::equal(heap, &a.target, &b.source)?))
    }
    fn find(&self, heap: &Heap, k: &EdgeId, fuel: Fuel<'_>) -> Result<Option<usize>, Error> {
        for (i, existing) in self.keys.iter().enumerate() {
            charge(fuel, 1)?;
            if self.eq_id(heap, existing, k)? {
                return Ok(Some(i));
            }
        }
        Ok(None)
    }
    fn set(&mut self, heap: &Heap, k: EdgeId, v: Value, fuel: Fuel<'_>) -> Result<(), Error> {
        if let Some(i) = self.find(heap, &k, fuel)? {
            self.vals[i] = v;
        } else {
            self.keys.push(k);
            self.vals.push(v);
        }
        Ok(())
    }
}

fn build_node_maps(
    heap: &Heap,
    g: &GraphData,
    nids: &[Value],
    fuel: Fuel<'_>,
) -> Result<(NodeMap, BTreeMap<i32, Value>), Error> {
    let mut by_id = NodeMap::new();
    let mut by_idx = BTreeMap::new();
    for (n, id) in g.nodes.iter().zip(nids.iter()) {
        by_id.set(heap, id.representation(), n.data.representation(), fuel)?;
        by_idx.insert(n.index, id.representation());
    }
    Ok((by_id, by_idx))
}

fn build_edge_map(
    heap: &Heap,
    g: &GraphData,
    by_idx: &BTreeMap<i32, Value>,
    eids: &[Value],
    fuel: Fuel<'_>,
) -> Result<EdgeMap, Error> {
    let mut out = EdgeMap::new(g.directed);
    for (e, id) in g.edges.iter().zip(eids.iter()) {
        let key = EdgeId {
            source: by_idx[&e.source].representation(),
            target: by_idx[&e.target].representation(),
            identity: id.representation(),
        };
        out.set(heap, key, e.data.representation(), fuel)?;
    }
    Ok(out)
}

#[expect(
    clippy::too_many_lines,
    reason = "four Graph set operations share identity maps"
)]
pub(super) fn combine(
    heap: &mut Heap,
    op_val: &Value,
    self_tup: &Value,
    that_tup: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let op = heap.string_lossy(op_val)?;
    let self_g_rep = heap.field(self_tup, 0)?.representation();
    let self_node_ids = slice_reps(heap, heap.field(self_tup, 1)?)?;
    let self_edge_ids = slice_reps(heap, heap.field(self_tup, 2)?)?;
    let that_g_rep = heap.field(that_tup, 0)?.representation();
    let that_node_ids = slice_reps(heap, heap.field(that_tup, 1)?)?;
    let that_edge_ids = slice_reps(heap, heap.field(that_tup, 2)?)?;

    let (directed, out_nodes, out_edges) = {
        let a = expect_graph(heap, &self_g_rep)?;
        let b = expect_graph(heap, &that_g_rep)?;
        if a.directed != b.directed {
            return Err(graph_defect(&format!(
                "Cannot combine {} and {} graphs",
                if a.directed { "directed" } else { "undirected" },
                if b.directed { "directed" } else { "undirected" }
            )));
        }
        let (self_nodes, self_idx) = build_node_maps(heap, a, &self_node_ids, fuel)?;
        let (that_nodes, that_idx) = build_node_maps(heap, b, &that_node_ids, fuel)?;
        let mut nodes = NodeMap::new();
        let mut edge_keys: Vec<EdgeId> = Vec::new();
        let mut edge_vals: Vec<Value> = Vec::new();
        match op.as_str() {
            "compose" => {
                for (k, v) in self_nodes.keys.iter().zip(self_nodes.vals.iter()) {
                    nodes.set(heap, k.representation(), v.representation(), fuel)?;
                }
                for (k, v) in that_nodes.keys.iter().zip(that_nodes.vals.iter()) {
                    nodes.set(heap, k.representation(), v.representation(), fuel)?;
                }
                let mut edges = build_edge_map(heap, a, &self_idx, &self_edge_ids, fuel)?;
                let more = build_edge_map(heap, b, &that_idx, &that_edge_ids, fuel)?;
                for (k, v) in more.keys.into_iter().zip(more.vals) {
                    edges.set(heap, k, v, fuel)?;
                }
                edge_keys = edges.keys;
                edge_vals = edges.vals;
            }
            "intersection" => {
                let self_edges = build_edge_map(heap, a, &self_idx, &self_edge_ids, fuel)?;
                for (k, v) in self_nodes.keys.iter().zip(self_nodes.vals.iter()) {
                    if that_nodes.find(heap, k, fuel)?.is_some() {
                        nodes.set(heap, k.representation(), v.representation(), fuel)?;
                    }
                }
                let mut that_edges = EdgeMap::new(b.directed);
                for (e, eid) in b.edges.iter().zip(that_edge_ids.iter()) {
                    let s = &that_idx[&e.source];
                    let t = &that_idx[&e.target];
                    if nodes.find(heap, s, fuel)?.is_some() && nodes.find(heap, t, fuel)?.is_some()
                    {
                        that_edges.set(
                            heap,
                            EdgeId {
                                source: s.representation(),
                                target: t.representation(),
                                identity: eid.representation(),
                            },
                            e.data.representation(),
                            fuel,
                        )?;
                    }
                }
                for (k, v) in that_edges.keys.into_iter().zip(that_edges.vals) {
                    if self_edges.find(heap, &k, fuel)?.is_some() {
                        edge_keys.push(k);
                        edge_vals.push(v);
                    }
                }
            }
            "difference" => {
                nodes = self_nodes;
                let that_edges = build_edge_map(heap, b, &that_idx, &that_edge_ids, fuel)?;
                for (e, eid) in a.edges.iter().zip(self_edge_ids.iter()) {
                    let k = EdgeId {
                        source: self_idx[&e.source].representation(),
                        target: self_idx[&e.target].representation(),
                        identity: eid.representation(),
                    };
                    if that_edges.find(heap, &k, fuel)?.is_none() {
                        edge_keys.push(k);
                        edge_vals.push(e.data.representation());
                    }
                }
            }
            _ => {
                for (k, v) in self_nodes.keys.iter().zip(self_nodes.vals.iter()) {
                    nodes.set(heap, k.representation(), v.representation(), fuel)?;
                }
                for (k, v) in that_nodes.keys.iter().zip(that_nodes.vals.iter()) {
                    nodes.set(heap, k.representation(), v.representation(), fuel)?;
                }
                let self_edges = build_edge_map(heap, a, &self_idx, &self_edge_ids, fuel)?;
                let that_edges = build_edge_map(heap, b, &that_idx, &that_edge_ids, fuel)?;
                for (k, v) in self_edges.keys.iter().zip(self_edges.vals.iter()) {
                    if that_edges.find(heap, k, fuel)?.is_none() {
                        edge_keys.push(EdgeId {
                            source: k.source.representation(),
                            target: k.target.representation(),
                            identity: k.identity.representation(),
                        });
                        edge_vals.push(v.representation());
                    }
                }
                for (k, v) in that_edges.keys.iter().zip(that_edges.vals.iter()) {
                    if self_edges.find(heap, k, fuel)?.is_none() {
                        edge_keys.push(EdgeId {
                            source: k.source.representation(),
                            target: k.target.representation(),
                            identity: k.identity.representation(),
                        });
                        edge_vals.push(v.representation());
                    }
                }
            }
        }
        let mut built_edges = Vec::new();
        for (k, v) in edge_keys.iter().zip(edge_vals.iter()) {
            if let (Some(si), Some(ti)) = (
                nodes.find(heap, &k.source, fuel)?,
                nodes.find(heap, &k.target, fuel)?,
            ) {
                built_edges.push((
                    i32::try_from(si).unwrap_or(0),
                    i32::try_from(ti).unwrap_or(0),
                    v.representation(),
                ));
            }
        }
        (a.directed, nodes.vals, built_edges)
    };

    let mut final_nodes = Vec::with_capacity(out_nodes.len());
    for (i, rep) in out_nodes.into_iter().enumerate() {
        let data = match heap.dup(&rep) {
            Ok(d) => d,
            Err(err) => {
                drop_partial(heap, final_nodes, Vec::new())?;
                return Err(err);
            }
        };
        final_nodes.push(GraphNode {
            index: i32::try_from(i).unwrap_or(0),
            data,
        });
    }
    let mut final_edges = Vec::with_capacity(out_edges.len());
    for (i, (source, target, rep)) in out_edges.into_iter().enumerate() {
        let data = match heap.dup(&rep) {
            Ok(d) => d,
            Err(err) => {
                drop_partial(heap, final_nodes, final_edges)?;
                return Err(err);
            }
        };
        final_edges.push(GraphEdge {
            index: i32::try_from(i).unwrap_or(0),
            source,
            target,
            data,
        });
    }
    Ok(alloc_graph(
        heap,
        hydrate(GraphData {
            directed,
            nodes: final_nodes,
            edges: final_edges,
            adj: BTreeMap::new(),
            radj: BTreeMap::new(),
            next_node: 0,
            next_edge: 0,
        }),
    ))
}

fn missing_complement_pairs(g: &GraphData) -> Vec<(usize, usize)> {
    let n = g.nodes.len();
    let (out_rows, _) = rows(g, false);
    let mut marks = vec![0usize; n];
    let mut pairs = Vec::new();
    for (i, row) in out_rows.iter().enumerate() {
        for &j in row {
            marks[j] = i + 1;
        }
        let start = if g.directed { 0 } else { i + 1 };
        for (j, &mark) in marks.iter().enumerate().skip(start) {
            if i == j || mark == i + 1 {
                continue;
            }
            pairs.push((i, j));
        }
    }
    pairs
}

pub(super) fn complement_pairs(
    heap: &mut Heap,
    val: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let gid = expect_graph_id(heap, val)?;
    let pair_reps: Vec<(Value, Value)> = {
        let Data::Graph(g) = &heap.object(gid)?.data else {
            return Err(Error::PatternKind("Graph"));
        };
        let pairs = missing_complement_pairs(g);
        charge(fuel, pairs.len())?;
        pairs
            .into_iter()
            .map(|(i, j)| {
                (
                    g.nodes[i].data.representation(),
                    g.nodes[j].data.representation(),
                )
            })
            .collect()
    };
    let mut tuples = Vec::with_capacity(pair_reps.len());
    for (a_rep, b_rep) in pair_reps {
        let a = match heap.dup(&a_rep) {
            Ok(v) => v,
            Err(err) => {
                heap.drop_values(tuples)?;
                return Err(err);
            }
        };
        let b = match heap.dup(&b_rep) {
            Ok(v) => v,
            Err(err) => {
                heap.drop_value(a)?;
                heap.drop_values(tuples)?;
                return Err(err);
            }
        };
        tuples.push(heap.construct("$tuple", vec![a, b])?);
    }
    heap.construct("$list", tuples)
}

pub(super) fn complement_build(
    heap: &mut Heap,
    edge_data_list: &Value,
    val: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let edge_reps = slice_reps(heap, edge_data_list)?;
    let gid = expect_graph_id(heap, val)?;
    let (directed, node_reps, pairs) = {
        let Data::Graph(g) = &heap.object(gid)?.data else {
            return Err(Error::PatternKind("Graph"));
        };
        let pairs = missing_complement_pairs(g);
        charge(fuel, g.nodes.len() + pairs.len())?;
        (
            g.directed,
            g.nodes
                .iter()
                .map(|n| n.data.representation())
                .collect::<Vec<_>>(),
            pairs,
        )
    };
    let mut nodes = Vec::with_capacity(node_reps.len());
    for (i, rep) in node_reps.into_iter().enumerate() {
        let data = match heap.dup(&rep) {
            Ok(v) => v,
            Err(err) => {
                drop_partial(heap, nodes, Vec::new())?;
                return Err(err);
            }
        };
        nodes.push(GraphNode {
            index: i32::try_from(i).unwrap_or(0),
            data,
        });
    }
    let mut edges = Vec::with_capacity(pairs.len());
    for (k, ((i, j), rep)) in pairs.into_iter().zip(edge_reps.iter()).enumerate() {
        let data = match heap.dup(rep) {
            Ok(v) => v,
            Err(err) => {
                drop_partial(heap, nodes, edges)?;
                return Err(err);
            }
        };
        edges.push(GraphEdge {
            index: i32::try_from(k).unwrap_or(0),
            source: i32::try_from(i).unwrap_or(0),
            target: i32::try_from(j).unwrap_or(0),
            data,
        });
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
