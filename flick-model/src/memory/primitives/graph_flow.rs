//! Counted `Graph` flows, cuts, minimum spanning forest, and transitive reduction.

use std::collections::{BTreeSet, VecDeque};
use std::mem;

use super::graph_algos::is_acyclic;
use super::graph_ops::{
    alloc_graph, clone_graph_filtered, edge_at, endpoints, expect_graph, expect_graph_id,
    expect_int, graph_err, hydrate, index_list, missing_node_msg, node_at,
};
use super::graph_paths::read_floats;
use super::{Error, GraphData, Heap, Value, charge};

struct FlowSolution {
    value: f64,
    flows: Vec<(i32, f64)>,
    cut: Vec<i32>,
    source_side: Vec<bool>,
}

struct FlowArc {
    from: usize,
    to: usize,
    capacity: f64,
    flow: f64,
}

#[expect(clippy::too_many_lines, reason = "Edmonds-Karp residual network flow")]
fn solve_flow(
    g: &GraphData,
    s_idx: i32,
    t_idx: i32,
    capacities: &[f64],
    fuel: &mut u64,
) -> Result<Result<FlowSolution, String>, Error> {
    if !g.directed {
        return Ok(Err("Cannot compute flow of undirected graph".into()));
    }
    let Some(source) = node_at(g, s_idx) else {
        return Ok(Err(missing_node_msg(s_idx)));
    };
    let Some(target) = node_at(g, t_idx) else {
        return Ok(Err(missing_node_msg(t_idx)));
    };
    if source == target {
        return Ok(Err("Flow source and target must be different nodes".into()));
    }
    let (sources, targets) = endpoints(g);
    let n = g.nodes.len();
    let mut arcs: Vec<FlowArc> = Vec::new();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut forward = vec![None::<usize>; g.edges.len()];
    for (q, e) in g.edges.iter().enumerate() {
        let cap = capacities[q];
        if !cap.is_finite() || cap < 0.0 {
            return Ok(Err(format!(
                "Edge {} capacity must be a finite non-negative number",
                e.index
            )));
        }
        let (from, to) = (sources[q], targets[q]);
        if from == to {
            continue;
        }
        let fid = arcs.len();
        forward[q] = Some(fid);
        adj[from].push(fid);
        arcs.push(FlowArc {
            from,
            to,
            capacity: cap,
            flow: 0.0,
        });
        adj[to].push(fid + 1);
        arcs.push(FlowArc {
            from: to,
            to: from,
            capacity: 0.0,
            flow: 0.0,
        });
    }
    let residual = |stop_at: Option<usize>, arcs: &[FlowArc]| -> (Vec<bool>, Vec<usize>) {
        let mut visited = vec![false; n];
        let mut parent = vec![0usize; n];
        visited[source] = true;
        let mut queue = VecDeque::from([source]);
        while let Some(cur) = queue.pop_front() {
            if stop_at.is_some_and(|s| visited[s]) {
                break;
            }
            for &a in &adj[cur] {
                if arcs[a].capacity - arcs[a].flow > 0.0 && !visited[arcs[a].to] {
                    visited[arcs[a].to] = true;
                    parent[arcs[a].to] = a;
                    queue.push_back(arcs[a].to);
                    if stop_at == Some(arcs[a].to) {
                        break;
                    }
                }
            }
        }
        (visited, parent)
    };
    let mut value = 0.0;
    loop {
        charge(fuel, n + arcs.len())?;
        let (visited, parent) = residual(Some(target), &arcs);
        if !visited[target] {
            break;
        }
        let mut amount = f64::INFINITY;
        let mut node = target;
        while node != source {
            let a = &arcs[parent[node]];
            amount = amount.min(a.capacity - a.flow);
            node = a.from;
        }
        if !(value + amount).is_finite() {
            return Ok(Err("Maximum flow exceeds the finite number range".into()));
        }
        let mut node = target;
        while node != source {
            let aid = parent[node];
            arcs[aid].flow += amount;
            arcs[aid ^ 1].flow -= amount;
            node = arcs[aid].from;
        }
        value += amount;
    }
    let flows: Vec<(i32, f64)> = g
        .edges
        .iter()
        .enumerate()
        .map(|(q, e)| (e.index, forward[q].map_or(0.0, |fid| arcs[fid].flow)))
        .collect();
    let (source_side, _) = residual(None, &arcs);
    let cut: Vec<i32> = g
        .edges
        .iter()
        .enumerate()
        .filter(|&(q, _)| {
            sources[q] != targets[q] && source_side[sources[q]] && !source_side[targets[q]]
        })
        .map(|(_, e)| e.index)
        .collect();
    Ok(Ok(FlowSolution {
        value,
        flows,
        cut,
        source_side,
    }))
}

pub(super) fn maximum_flow(
    heap: &mut Heap,
    src_val: &Value,
    dst_val: &Value,
    caps_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let s = expect_int(src_val)?;
    let t = expect_int(dst_val)?;
    let caps = read_floats(heap, caps_val)?;
    let sol = {
        let g = expect_graph(heap, val)?;
        match solve_flow(g, s, t, &caps, fuel)? {
            Ok(sol) => sol,
            Err(m) => return graph_err(heap, &m),
        }
    };
    let mut flow_tuples = Vec::with_capacity(sol.flows.len());
    for (ei, fl) in sol.flows {
        flow_tuples.push(heap.construct("$tuple", vec![Value::Int(ei), Value::Float(fl)])?);
    }
    let flows_v = heap.construct("$list", flow_tuples)?;
    let cut_v = index_list(heap, &sol.cut)?;
    let rec = heap.record(vec![
        ("value".into(), Value::Float(sol.value)),
        ("flows".into(), flows_v),
        ("cut".into(), cut_v),
    ])?;
    heap.construct("Prelude.Result.Ok", vec![rec])
}

pub(super) fn minimum_cut(
    heap: &mut Heap,
    src_val: &Value,
    dst_val: &Value,
    caps_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let s = expect_int(src_val)?;
    let t = expect_int(dst_val)?;
    let caps = read_floats(heap, caps_val)?;
    let (value, cut, src_nodes, dst_nodes) = {
        let g = expect_graph(heap, val)?;
        let sol = match solve_flow(g, s, t, &caps, fuel)? {
            Ok(sol) => sol,
            Err(m) => return graph_err(heap, &m),
        };
        let mut sn = Vec::new();
        let mut tn = Vec::new();
        for (p, n) in g.nodes.iter().enumerate() {
            if sol.source_side[p] {
                sn.push(n.index);
            } else {
                tn.push(n.index);
            }
        }
        (sol.value, sol.cut, sn, tn)
    };
    let edges_v = index_list(heap, &cut)?;
    let source_v = index_list(heap, &src_nodes)?;
    let target_v = index_list(heap, &dst_nodes)?;
    let rec = heap.record(vec![
        ("value".into(), Value::Float(value)),
        ("edges".into(), edges_v),
        ("source".into(), source_v),
        ("target".into(), target_v),
    ])?;
    heap.construct("Prelude.Result.Ok", vec![rec])
}

pub(super) fn minimum_spanning_forest(
    heap: &mut Heap,
    weights_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let weights = read_floats(heap, weights_val)?;
    let gid = expect_graph_id(heap, val)?;
    let selected = {
        let g = expect_graph(heap, val)?;
        if g.directed {
            return graph_err(
                heap,
                "Cannot find minimum spanning forest of directed graph",
            );
        }
        charge(fuel, g.nodes.len() + g.edges.len())?;
        let mut weighted = Vec::new();
        for (q, &w) in weights.iter().enumerate() {
            if w.is_nan() || (w.is_infinite() && w.is_sign_negative()) {
                return graph_err(
                    heap,
                    "Minimum spanning forest does not support NaN or -Infinity edge weights",
                );
            }
            if !(w.is_infinite() && w.is_sign_positive()) {
                weighted.push((q, w));
            }
        }
        weighted.sort_by(|a, b| a.1.total_cmp(&b.1));
        let n = g.nodes.len();
        let mut parents: Vec<usize> = (0..n).collect();
        let mut ranks = vec![0usize; n];
        let mut sel = vec![false; g.edges.len()];
        for (q, _) in weighted {
            let e = &g.edges[q];
            let mut a = node_at(g, e.source).unwrap_or(0);
            let mut b = node_at(g, e.target).unwrap_or(0);
            while parents[a] != a {
                parents[a] = parents[parents[a]];
                a = parents[a];
            }
            while parents[b] != b {
                parents[b] = parents[parents[b]];
                b = parents[b];
            }
            if a == b {
                continue;
            }
            sel[q] = true;
            if ranks[a] < ranks[b] {
                mem::swap(&mut a, &mut b);
            }
            parents[b] = a;
            if ranks[a] == ranks[b] {
                ranks[a] += 1;
            }
        }
        sel
    };
    let out = hydrate(clone_graph_filtered(
        heap,
        gid,
        |_, _| true,
        |q, _, _, _| selected[q],
    )?);
    let gv = alloc_graph(heap, out);
    heap.construct("Prelude.Result.Ok", vec![gv])
}

pub(super) fn transitive_reduction(
    heap: &mut Heap,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let gid = expect_graph_id(heap, val)?;
    let retained = {
        let g = expect_graph(heap, val)?;
        if !g.directed {
            return graph_err(heap, "Cannot transitively reduce undirected graph");
        }
        if !is_acyclic(g) {
            return graph_err(heap, "Cannot transitively reduce cyclic graph");
        }
        charge(fuel, g.edges.len().saturating_mul(g.nodes.len().max(1)))?;
        let mut first = BTreeSet::new();
        let mut keep = vec![false; g.edges.len()];
        for (q, e) in g.edges.iter().enumerate() {
            if !first.insert((e.source, e.target)) {
                continue;
            }
            let mut visited = BTreeSet::from([e.source]);
            let mut queue = VecDeque::from([e.source]);
            let mut reachable = false;
            while let Some(cur) = queue.pop_front() {
                if reachable {
                    break;
                }
                if let Some(eis) = g.adj.get(&cur) {
                    for &ci in eis {
                        let Some(cq) = edge_at(g, ci) else {
                            continue;
                        };
                        let cand = &g.edges[cq];
                        if cur == e.source && cand.target == e.target {
                            continue;
                        }
                        if cand.target == e.target {
                            reachable = true;
                            break;
                        }
                        if visited.insert(cand.target) {
                            queue.push_back(cand.target);
                        }
                    }
                }
            }
            keep[q] = !reachable;
        }
        keep
    };
    let out = hydrate(clone_graph_filtered(
        heap,
        gid,
        |_, _| true,
        |q, _, _, _| retained[q],
    )?);
    let gv = alloc_graph(heap, out);
    heap.construct("Prelude.Result.Ok", vec![gv])
}
