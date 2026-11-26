//! JSON codec decode/encode primitives for counted `Graph.json`.

use std::collections::{BTreeMap, BTreeSet};

use flick_core::ir::Lit;

use super::graph_ops::{alloc_graph, drop_partial, expect_graph_id, hydrate};
use super::json_ops::schema_error_single;
use super::list_ops::slice_reps;
use super::schema_leaves::number_of;
use super::schema_ops::prefix_issues;
use super::{Data, Error, Fuel, GraphData, GraphEdge, GraphNode, Heap, Value, charge};

fn schema_invalid_at(
    heap: &mut Heap,
    section: &str,
    idx: usize,
    field: &str,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let base = schema_error_single(heap, "", "an invalid value")?;
    let Value::Object(tok) = &base else {
        return Ok(base);
    };
    let inner = match &heap.object(tok.id)?.data {
        Data::Tagged { fields, .. } if fields.len() == 1 => fields[0].representation(),
        _ => return Ok(base),
    };
    let f_seg = heap.literal(&Lit::String(field.into()))?;
    let p1 = prefix_issues(heap, &f_seg, &inner, fuel);
    heap.drop_value(f_seg)?;
    heap.drop_value(base)?;
    let p1 = p1?;
    let i_seg = Value::Int(i32::try_from(idx).unwrap_or(i32::MAX));
    let p2 = prefix_issues(heap, &i_seg, &p1, fuel);
    heap.drop_value(p1)?;
    let p2 = p2?;
    let s_seg = heap.literal(&Lit::String(section.into()))?;
    let p3 = prefix_issues(heap, &s_seg, &p2, fuel);
    heap.drop_value(s_seg)?;
    heap.drop_value(p2)?;
    heap.construct("Prelude.Result.Err", vec![p3?])
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "validated non-negative index"
)]
pub(super) fn json_decode(
    heap: &mut Heap,
    kind_val: &Value,
    rec: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let kind = heap.string_lossy(kind_val)?;
    let directed = kind == "directed";
    let node_reps = slice_reps(heap, heap.named_field(rec, "nodes")?)?;
    let edge_reps = slice_reps(heap, heap.named_field(rec, "edges")?)?;
    charge(fuel, node_reps.len() + edge_reps.len())?;
    let mut known = BTreeSet::new();
    let mut prev = -1i32;
    let mut nodes = Vec::with_capacity(node_reps.len());
    for (i, n) in node_reps.iter().enumerate() {
        let idx = number_of(heap.named_field(n, "index")?).unwrap_or(-1.0) as i32;
        if idx <= prev {
            drop_partial(heap, nodes, Vec::new())?;
            return schema_invalid_at(heap, "nodes", i, "index", fuel);
        }
        prev = idx;
        known.insert(idx);
        let data_rep = heap.named_field(n, "data")?.representation();
        match heap.dup(&data_rep) {
            Ok(data) => nodes.push(GraphNode { index: idx, data }),
            Err(err) => {
                drop_partial(heap, nodes, Vec::new())?;
                return Err(err);
            }
        }
    }
    let mut prev_e = -1i32;
    let mut edges = Vec::with_capacity(edge_reps.len());
    for (i, e) in edge_reps.iter().enumerate() {
        let idx = number_of(heap.named_field(e, "index")?).unwrap_or(-1.0) as i32;
        let src = number_of(heap.named_field(e, "source")?).unwrap_or(-1.0) as i32;
        let dst = number_of(heap.named_field(e, "target")?).unwrap_or(-1.0) as i32;
        if idx <= prev_e {
            drop_partial(heap, nodes, edges)?;
            return schema_invalid_at(heap, "edges", i, "index", fuel);
        }
        if !known.contains(&src) {
            drop_partial(heap, nodes, edges)?;
            return schema_invalid_at(heap, "edges", i, "source", fuel);
        }
        if !known.contains(&dst) {
            drop_partial(heap, nodes, edges)?;
            return schema_invalid_at(heap, "edges", i, "target", fuel);
        }
        prev_e = idx;
        let data_rep = heap.named_field(e, "data")?.representation();
        match heap.dup(&data_rep) {
            Ok(data) => edges.push(GraphEdge {
                index: idx,
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

pub(super) fn json_encode(
    heap: &mut Heap,
    kind_val: &Value,
    val: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let want = heap.string_lossy(kind_val)?;
    let Ok(gid) = expect_graph_id(heap, val) else {
        return schema_error_single(heap, "", "an invalid value");
    };
    let (directed, node_reps, edge_reps) = {
        let Data::Graph(g) = &heap.object(gid)?.data else {
            return schema_error_single(heap, "", "an invalid value");
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
    let actual_kind = if directed { "directed" } else { "undirected" };
    if actual_kind != want {
        return schema_error_single(heap, "", "an invalid value");
    }
    charge(fuel, node_reps.len() + edge_reps.len())?;
    let mut encoded_nodes = Vec::with_capacity(node_reps.len());
    for (idx, rep) in node_reps {
        let data = heap.dup(&rep)?;
        encoded_nodes.push(heap.record(vec![
            ("index".into(), Value::Float(f64::from(idx))),
            ("data".into(), data),
        ])?);
    }
    let nodes_list = heap.construct("$list", encoded_nodes)?;
    let mut encoded_edges = Vec::with_capacity(edge_reps.len());
    for (idx, src, dst, rep) in edge_reps {
        let data = match heap.dup(&rep) {
            Ok(d) => d,
            Err(err) => {
                heap.drop_value(nodes_list)?;
                heap.drop_values(encoded_edges)?;
                return Err(err);
            }
        };
        encoded_edges.push(heap.record(vec![
            ("index".into(), Value::Float(f64::from(idx))),
            ("source".into(), Value::Float(f64::from(src))),
            ("target".into(), Value::Float(f64::from(dst))),
            ("data".into(), data),
        ])?);
    }
    let edges_list = heap.construct("$list", encoded_edges)?;
    let kind_s = heap.literal(&Lit::String(actual_kind.into()))?;
    let rec = heap.record(vec![
        ("type".into(), kind_s),
        ("nodes".into(), nodes_list),
        ("edges".into(), edges_list),
    ])?;
    heap.construct("Prelude.Result.Ok", vec![rec])
}
