//! Counted `Graph` all-pairs Floyd-Warshall, simple-path, and all-shortest-paths enumerations.

use super::graph_ops::{
    endpoints, expect_graph, expect_graph_id, expect_int, graph_err, index_list, missing_node_msg,
    node_at, opt_inner, rows,
};
use super::graph_paths::{MinHeap, overflows, path_result_rec, read_floats};
use super::{Data, Error, Heap, Value, charge};

#[derive(Clone)]
enum Witness {
    Edge {
        target: usize,
        edge: usize,
    },
    Split {
        target: usize,
        left: usize,
        right: usize,
    },
}

type FwCell = (i32, f64, Option<(Vec<i32>, Vec<usize>)>);

#[expect(
    clippy::too_many_lines,
    reason = "Floyd-Warshall all-pairs matrix construction"
)]
pub(super) fn floyd_warshall(
    heap: &mut Heap,
    weights_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let weights = read_floats(heap, weights_val)?;
    let gid = expect_graph_id(heap, val)?;
    let rows_data: Vec<(i32, Vec<FwCell>)> = {
        let g = expect_graph(heap, val)?;
        let n = g.nodes.len();
        if weights
            .iter()
            .any(|&w| w.is_nan() || (w.is_infinite() && w.is_sign_negative()))
        {
            return graph_err(
                heap,
                "Floyd-Warshall algorithm does not support NaN or -Infinity edge weights",
            );
        }
        charge(fuel, n.saturating_mul(n).saturating_mul(n).max(1))?;
        let (sources, targets) = endpoints(g);
        let mut pool: Vec<Witness> = Vec::new();
        let mut witnesses: Vec<Option<usize>> = vec![None; n * n];
        let mut dist = vec![f64::INFINITY; n * n];
        for i in 0..n {
            dist[i * n + i] = 0.0;
        }
        for (q, &w) in weights.iter().enumerate() {
            let (s, t) = (sources[q], targets[q]);
            if w < dist[s * n + t] {
                dist[s * n + t] = w;
                let id = pool.len();
                pool.push(Witness::Edge { target: t, edge: q });
                witnesses[s * n + t] = Some(id);
            }
            if !g.directed && w < dist[t * n + s] {
                dist[t * n + s] = w;
                let id = pool.len();
                pool.push(Witness::Edge { target: s, edge: q });
                witnesses[t * n + s] = Some(id);
            }
        }
        for k in 0..n {
            for i in 0..n {
                let ik = dist[i * n + k];
                if ik.is_infinite() && ik.is_sign_positive() {
                    continue;
                }
                let left = witnesses[i * n + k];
                for j in 0..n {
                    let kj = dist[k * n + j];
                    if kj.is_infinite() && kj.is_sign_positive() {
                        continue;
                    }
                    let cand = ik + kj;
                    if !cand.is_finite() {
                        return graph_err(
                            heap,
                            "Floyd-Warshall distance calculation exceeded the finite number range",
                        );
                    }
                    if cand < dist[i * n + j] {
                        dist[i * n + j] = cand;
                        let right = witnesses[k * n + j];
                        witnesses[i * n + j] = match (left, right) {
                            (None, r) => r,
                            (l, None) => l,
                            (Some(l), Some(r)) => {
                                let id = pool.len();
                                pool.push(Witness::Split {
                                    target: j,
                                    left: l,
                                    right: r,
                                });
                                Some(id)
                            }
                        };
                    }
                }
            }
        }
        for i in 0..n {
            if dist[i * n + i] < 0.0 {
                return graph_err(
                    heap,
                    &format!(
                        "Negative cycle detected involving node {}",
                        g.nodes[i].index
                    ),
                );
            }
        }
        let mut positions: Vec<Option<usize>> = vec![None; n];
        let mut all = Vec::with_capacity(n);
        for i in 0..n {
            let src = g.nodes[i].index;
            let mut row = Vec::with_capacity(n);
            for j in 0..n {
                let target_id = g.nodes[j].index;
                let d = dist[i * n + j];
                let entry = if i == j {
                    Some((vec![src], Vec::new()))
                } else if d.is_infinite() && d.is_sign_positive() {
                    None
                } else {
                    let mut path = vec![src];
                    let mut path_edges = Vec::new();
                    positions[i] = Some(0);
                    let mut nodes = vec![i];
                    let mut stack: Vec<usize> = witnesses[i * n + j].into_iter().collect();
                    while let Some(wid) = stack.pop() {
                        let target = match pool[wid] {
                            Witness::Edge { target, .. } | Witness::Split { target, .. } => target,
                        };
                        if let Some(at) = positions[target] {
                            for &p in &nodes[at + 1..] {
                                positions[p] = None;
                            }
                            nodes.truncate(at + 1);
                            path.truncate(at + 1);
                            path_edges.truncate(at);
                        } else {
                            match pool[wid] {
                                Witness::Edge { target, edge } => {
                                    positions[target] = Some(nodes.len());
                                    nodes.push(target);
                                    path.push(g.nodes[target].index);
                                    path_edges.push(edge);
                                }
                                Witness::Split { left, right, .. } => {
                                    stack.push(right);
                                    stack.push(left);
                                }
                            }
                        }
                    }
                    for p in nodes {
                        positions[p] = None;
                    }
                    Some((path, path_edges))
                };
                row.push((target_id, d, entry));
            }
            all.push((src, row));
        }
        all
    };

    let (edge_ids, edge_reps): (Vec<i32>, Vec<Value>) = {
        let Data::Graph(g) = &heap.object(gid)?.data else {
            return Err(Error::PatternKind("Graph"));
        };
        (
            g.edges.iter().map(|e| e.index).collect(),
            g.edges.iter().map(|e| e.data.representation()).collect(),
        )
    };
    let mut d_rows = Vec::with_capacity(rows_data.len());
    let mut p_rows = Vec::with_capacity(rows_data.len());
    let mut e_rows = Vec::with_capacity(rows_data.len());
    let mut c_rows = Vec::with_capacity(rows_data.len());
    for (src, row) in rows_data {
        let (mut dr, mut pr, mut er, mut cr) = (
            Vec::with_capacity(row.len()),
            Vec::with_capacity(row.len()),
            Vec::with_capacity(row.len()),
            Vec::with_capacity(row.len()),
        );
        for (dst, d, entry) in row {
            dr.push(heap.construct("$tuple", vec![Value::Int(dst), Value::Float(d)])?);
            match entry {
                None => {
                    let none = heap.construct("Prelude.Option.None", vec![])?;
                    pr.push(heap.construct("$tuple", vec![Value::Int(dst), none])?);
                    let empty_e = heap.construct("$list", vec![])?;
                    er.push(heap.construct("$tuple", vec![Value::Int(dst), empty_e])?);
                    let empty_c = heap.construct("$list", vec![])?;
                    cr.push(heap.construct("$tuple", vec![Value::Int(dst), empty_c])?);
                }
                Some((path, e_pos)) => {
                    let p_list = index_list(heap, &path)?;
                    let some_p = heap.construct("Prelude.Option.Some", vec![p_list])?;
                    pr.push(heap.construct("$tuple", vec![Value::Int(dst), some_p])?);
                    let e_ids: Vec<i32> = e_pos.iter().map(|&q| edge_ids[q]).collect();
                    let e_list = index_list(heap, &e_ids)?;
                    er.push(heap.construct("$tuple", vec![Value::Int(dst), e_list])?);
                    let mut cs = Vec::with_capacity(e_pos.len());
                    for &q in &e_pos {
                        cs.push(heap.dup(&edge_reps[q])?);
                    }
                    let c_list = heap.construct("$list", cs)?;
                    cr.push(heap.construct("$tuple", vec![Value::Int(dst), c_list])?);
                }
            }
        }
        let dr_l = heap.construct("$list", dr)?;
        let pr_l = heap.construct("$list", pr)?;
        let er_l = heap.construct("$list", er)?;
        let cr_l = heap.construct("$list", cr)?;
        d_rows.push(heap.construct("$tuple", vec![Value::Int(src), dr_l])?);
        p_rows.push(heap.construct("$tuple", vec![Value::Int(src), pr_l])?);
        e_rows.push(heap.construct("$tuple", vec![Value::Int(src), er_l])?);
        c_rows.push(heap.construct("$tuple", vec![Value::Int(src), cr_l])?);
    }
    let dl = heap.construct("$list", d_rows)?;
    let pl = heap.construct("$list", p_rows)?;
    let el = heap.construct("$list", e_rows)?;
    let cl = heap.construct("$list", c_rows)?;
    let rec = heap.record(vec![
        ("distances".into(), dl),
        ("paths".into(), pl),
        ("edges".into(), el),
        ("costs".into(), cl),
    ])?;
    heap.construct("Prelude.Result.Ok", vec![rec])
}

fn enumeration_limit(heap: &Heap, val: &Value) -> Result<Result<usize, String>, Error> {
    match opt_inner(heap, val)? {
        Some(v) => {
            let l = expect_int(v)?;
            if l < 0 {
                Ok(Err(
                    "Path enumeration limit must be a non-negative integer or Infinity".into(),
                ))
            } else {
                Ok(Ok(usize::try_from(l).unwrap_or(0)))
            }
        }
        None => Ok(Ok(usize::MAX)),
    }
}

pub(super) fn simple_paths(
    heap: &mut Heap,
    opts: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let s_idx = expect_int(heap.named_field(opts, "source")?)?;
    let t_idx = expect_int(heap.named_field(opts, "target")?)?;
    let gid = expect_graph_id(heap, val)?;
    let found = {
        let g = expect_graph(heap, val)?;
        let Some(source) = node_at(g, s_idx) else {
            return graph_err(heap, &missing_node_msg(s_idx));
        };
        let Some(target) = node_at(g, t_idx) else {
            return graph_err(heap, &missing_node_msg(t_idx));
        };
        let limit = match enumeration_limit(heap, heap.named_field(opts, "limit")?)? {
            Ok(l) => l,
            Err(m) => return graph_err(heap, &m),
        };
        let mut out = Vec::new();
        if limit > 0 {
            let (outgoing, edge_rows) = rows(g, false);
            let mut path = vec![g.nodes[source].index];
            let mut path_edges = Vec::new();
            let mut visited = vec![false; g.nodes.len()];
            visited[source] = true;
            let mut stack = vec![(source, 0usize)];
            while !stack.is_empty() && out.len() < limit {
                charge(fuel, 1)?;
                let (node, pos) = *stack.last().expect("non-empty");
                if node == target {
                    #[expect(clippy::cast_precision_loss, reason = "path length")]
                    let d = path_edges.len() as f64;
                    out.push((path.clone(), path_edges.clone(), d));
                    let (popped, _) = stack.pop().expect("non-empty");
                    if !stack.is_empty() {
                        visited[popped] = false;
                        path.pop();
                        path_edges.pop();
                    }
                    continue;
                }
                if pos >= outgoing[node].len() {
                    let (popped, _) = stack.pop().expect("non-empty");
                    if !stack.is_empty() {
                        visited[popped] = false;
                        path.pop();
                        path_edges.pop();
                    }
                    continue;
                }
                stack.last_mut().expect("non-empty").1 += 1;
                let next = outgoing[node][pos];
                if visited[next] {
                    continue;
                }
                let e = edge_rows[node][pos];
                visited[next] = true;
                path.push(g.nodes[next].index);
                path_edges.push(e);
                stack.push((next, 0));
            }
        }
        out
    };
    let mut recs = Vec::with_capacity(found.len());
    for (p, ep, d) in found {
        match path_result_rec(heap, gid, &p, &ep, d) {
            Ok(r) => recs.push(r),
            Err(err) => {
                heap.drop_values(recs)?;
                return Err(err);
            }
        }
    }
    let list = heap.construct("$list", recs)?;
    heap.construct("Prelude.Result.Ok", vec![list])
}

#[expect(
    clippy::float_cmp,
    clippy::too_many_lines,
    reason = "exact all_shortest_paths port"
)]
pub(super) fn all_shortest_paths(
    heap: &mut Heap,
    src_val: &Value,
    dst_val: &Value,
    limit_val: &Value,
    weights_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let s_idx = expect_int(src_val)?;
    let t_idx = expect_int(dst_val)?;
    let weights = read_floats(heap, weights_val)?;
    let gid = expect_graph_id(heap, val)?;
    let found = {
        let g = expect_graph(heap, val)?;
        let Some(source) = node_at(g, s_idx) else {
            return graph_err(heap, &missing_node_msg(s_idx));
        };
        let Some(target) = node_at(g, t_idx) else {
            return graph_err(heap, &missing_node_msg(t_idx));
        };
        let limit = match enumeration_limit(heap, limit_val)? {
            Ok(l) => l,
            Err(m) => return graph_err(heap, &m),
        };
        if weights.iter().any(|&w| w.is_nan() || w < 0.0) {
            return graph_err(
                heap,
                "All shortest paths requires non-negative edge weights",
            );
        }
        let mut out = Vec::new();
        if limit > 0 {
            let (outgoing, edge_rows) = rows(g, false);
            let n = g.nodes.len();
            let mut dist = vec![f64::INFINITY; n];
            dist[source] = 0.0;
            let mut previous: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
            let mut queue = MinHeap::new(n, false);
            let mut seq = 0usize;
            queue.push(source, 0.0, seq);
            seq += 1;
            while let Some((cur, d_cur)) = queue.pop() {
                charge(fuel, 1)?;
                if d_cur != dist[cur] {
                    continue;
                }
                for (k, &next) in outgoing[cur].iter().enumerate() {
                    let e = edge_rows[cur][k];
                    let d = d_cur + weights[e];
                    if overflows(weights[e], d) {
                        return graph_err(
                            heap,
                            "All shortest paths distance calculation exceeded the finite number range",
                        );
                    }
                    if d < dist[next] {
                        dist[next] = d;
                        previous[next] = vec![(cur, e)];
                        queue.push(next, d, seq);
                        seq += 1;
                    } else if d == dist[next] && !(d.is_infinite() && d.is_sign_positive()) {
                        previous[next].push((cur, e));
                    }
                }
            }
            let total_dist = dist[target];
            if !total_dist.is_infinite() {
                if source == target {
                    out.push((vec![g.nodes[source].index], Vec::new(), 0.0));
                } else {
                    let mut rev_path = vec![target];
                    let mut rev_edges = Vec::new();
                    let mut visited = vec![false; n];
                    visited[target] = true;
                    let mut stack = vec![(target, 0usize)];
                    while !stack.is_empty() && out.len() < limit {
                        charge(fuel, 1)?;
                        let (node, pos) = *stack.last().expect("non-empty");
                        if node == source {
                            let path: Vec<i32> =
                                rev_path.iter().rev().map(|&p| g.nodes[p].index).collect();
                            let edges: Vec<usize> = rev_edges.iter().rev().copied().collect();
                            out.push((path, edges, total_dist));
                            let (popped, _) = stack.pop().expect("non-empty");
                            if !stack.is_empty() {
                                visited[popped] = false;
                                rev_path.pop();
                                rev_edges.pop();
                            }
                            continue;
                        }
                        if pos >= previous[node].len() {
                            let (popped, _) = stack.pop().expect("non-empty");
                            if !stack.is_empty() {
                                visited[popped] = false;
                                rev_path.pop();
                                rev_edges.pop();
                            }
                            continue;
                        }
                        stack.last_mut().expect("non-empty").1 += 1;
                        let (p_node, p_edge) = previous[node][pos];
                        if visited[p_node] {
                            continue;
                        }
                        visited[p_node] = true;
                        rev_path.push(p_node);
                        rev_edges.push(p_edge);
                        stack.push((p_node, 0));
                    }
                }
            }
        }
        out
    };
    let mut recs = Vec::with_capacity(found.len());
    for (p, ep, d) in found {
        match path_result_rec(heap, gid, &p, &ep, d) {
            Ok(r) => recs.push(r),
            Err(err) => {
                heap.drop_values(recs)?;
                return Err(err);
            }
        }
    }
    let list = heap.construct("$list", recs)?;
    heap.construct("Prelude.Result.Ok", vec![list])
}
