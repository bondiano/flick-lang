//! Counted `Graph` single-pair shortest paths: `MinHeap`, Dijkstra, A*, and Bellman-Ford.

use super::graph_ops::{
    endpoints, expect_graph, expect_graph_id, expect_int, graph_err, index_list, missing_node_msg,
    node_at, rows,
};
use super::list_ops::slice_reps;
use super::{Data, Error, GraphData, Heap, ObjectId, Value, charge};

pub(super) struct MinHeap {
    nodes: Vec<usize>,
    priorities: Vec<f64>,
    sequences: Vec<usize>,
    positions: Option<Vec<Option<usize>>>,
}

impl MinHeap {
    pub(super) fn new(n: usize, indexed: bool) -> Self {
        Self {
            nodes: Vec::new(),
            priorities: Vec::new(),
            sequences: Vec::new(),
            positions: indexed.then(|| vec![None; n]),
        }
    }
    #[expect(clippy::float_cmp, reason = "exact tie-breaking by sequence")]
    fn before(p: f64, s: usize, q: f64, t: usize) -> bool {
        p < q || (p == q && s < t)
    }
    fn set(&mut self, i: usize, node: usize, priority: f64, seq: usize) {
        self.nodes[i] = node;
        self.priorities[i] = priority;
        self.sequences[i] = seq;
        if let Some(pos) = &mut self.positions {
            pos[node] = Some(i);
        }
    }
    pub(super) fn push(&mut self, node: usize, priority: f64, seq: usize) {
        let mut i = self
            .positions
            .as_ref()
            .and_then(|pos| pos[node])
            .unwrap_or_else(|| {
                let idx = self.nodes.len();
                self.nodes.push(0);
                self.priorities.push(0.0);
                self.sequences.push(0);
                idx
            });
        while i > 0 {
            let parent = (i - 1) / 2;
            if !Self::before(
                priority,
                seq,
                self.priorities[parent],
                self.sequences[parent],
            ) {
                break;
            }
            let (pn, pp, ps) = (
                self.nodes[parent],
                self.priorities[parent],
                self.sequences[parent],
            );
            self.set(i, pn, pp, ps);
            i = parent;
        }
        self.set(i, node, priority, seq);
    }
    pub(super) fn pop(&mut self) -> Option<(usize, f64)> {
        if self.nodes.is_empty() {
            return None;
        }
        let (node, priority) = (self.nodes[0], self.priorities[0]);
        if let Some(pos) = &mut self.positions {
            pos[node] = None;
        }
        let n = self.nodes.pop().expect("non-empty");
        let p = self.priorities.pop().expect("non-empty");
        let s = self.sequences.pop().expect("non-empty");
        let len = self.nodes.len();
        if len == 0 {
            return Some((node, priority));
        }
        let mut i = 0;
        loop {
            let mut child = i * 2 + 1;
            if child >= len {
                break;
            }
            let right = child + 1;
            if right < len
                && Self::before(
                    self.priorities[right],
                    self.sequences[right],
                    self.priorities[child],
                    self.sequences[child],
                )
            {
                child = right;
            }
            if !Self::before(self.priorities[child], self.sequences[child], p, s) {
                break;
            }
            let (cn, cp, cs) = (
                self.nodes[child],
                self.priorities[child],
                self.sequences[child],
            );
            self.set(i, cn, cp, cs);
            i = child;
        }
        self.set(i, n, p, s);
        Some((node, priority))
    }
}

pub(super) fn overflows(w: f64, sum: f64) -> bool {
    !(sum.is_finite() || (w.is_infinite() && w.is_sign_positive()))
}

pub(super) fn read_floats(heap: &Heap, val: &Value) -> Result<Vec<f64>, Error> {
    let reps = slice_reps(heap, val)?;
    let mut out = Vec::with_capacity(reps.len());
    for r in &reps {
        match *r {
            Value::Float(f) => out.push(f),
            Value::Int(i) => out.push(f64::from(i)),
            _ => return Err(Error::ExpectedFloat),
        }
    }
    Ok(out)
}

pub(super) fn path_result_rec(
    heap: &mut Heap,
    gid: ObjectId,
    path: &[i32],
    edge_positions: &[usize],
    distance: f64,
) -> Result<Value, Error> {
    let (edge_ids, cost_reps): (Vec<i32>, Vec<Value>) = {
        let Data::Graph(g) = &heap.object(gid)?.data else {
            return Err(Error::PatternKind("Graph"));
        };
        (
            edge_positions.iter().map(|&q| g.edges[q].index).collect(),
            edge_positions
                .iter()
                .map(|&q| g.edges[q].data.representation())
                .collect(),
        )
    };
    let path_v = index_list(heap, path)?;
    let edges_v = index_list(heap, &edge_ids)?;
    let mut costs = Vec::with_capacity(cost_reps.len());
    for r in cost_reps {
        match heap.dup(&r) {
            Ok(v) => costs.push(v),
            Err(err) => {
                heap.drop_value(path_v)?;
                heap.drop_value(edges_v)?;
                heap.drop_values(costs)?;
                return Err(err);
            }
        }
    }
    let costs_v = heap.construct("$list", costs)?;
    heap.record(vec![
        ("path".into(), path_v),
        ("edges".into(), edges_v),
        ("distance".into(), Value::Float(distance)),
        ("costs".into(), costs_v),
    ])
}

fn trace_path_positions(
    g: &GraphData,
    target: usize,
    prev_node: &[Option<usize>],
    prev_edge: &[Option<usize>],
) -> (Vec<i32>, Vec<usize>) {
    let mut path = Vec::new();
    let mut edges = Vec::new();
    let mut cur = Some(target);
    while let Some(p) = cur {
        path.push(g.nodes[p].index);
        if let Some(e) = prev_edge[p] {
            edges.push(e);
        }
        cur = prev_node[p];
    }
    path.reverse();
    edges.reverse();
    (path, edges)
}

fn ok_some_path(
    heap: &mut Heap,
    gid: ObjectId,
    found: Option<(Vec<i32>, Vec<usize>, f64)>,
) -> Result<Value, Error> {
    let opt = match found {
        Some((path, edge_pos, dist)) => {
            let rec = path_result_rec(heap, gid, &path, &edge_pos, dist)?;
            heap.construct("Prelude.Option.Some", vec![rec])?
        }
        None => heap.construct("Prelude.Option.None", vec![])?,
    };
    heap.construct("Prelude.Result.Ok", vec![opt])
}

pub(super) fn dijkstra(
    heap: &mut Heap,
    src_val: &Value,
    dst_val: &Value,
    weights_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let s_idx = expect_int(src_val)?;
    let t_idx = expect_int(dst_val)?;
    let weights = read_floats(heap, weights_val)?;
    let gid = expect_graph_id(heap, val)?;
    let outcome = {
        let g = expect_graph(heap, val)?;
        let Some(source) = node_at(g, s_idx) else {
            return graph_err(heap, &missing_node_msg(s_idx));
        };
        let Some(target) = node_at(g, t_idx) else {
            return graph_err(heap, &missing_node_msg(t_idx));
        };
        if weights.iter().any(|&w| w.is_nan() || w < 0.0) {
            return graph_err(
                heap,
                "Dijkstra's algorithm requires non-negative edge weights",
            );
        }
        charge(fuel, g.nodes.len() + g.edges.len())?;
        if source == target {
            Some((vec![g.nodes[source].index], Vec::new(), 0.0))
        } else {
            let (outgoing, edge_rows) = rows(g, false);
            let n = g.nodes.len();
            let mut dist = vec![f64::INFINITY; n];
            let mut prev_node = vec![None; n];
            let mut prev_edge = vec![None; n];
            let mut visited = vec![false; n];
            dist[source] = 0.0;
            let mut pq = MinHeap::new(n, true);
            let mut seq = 0usize;
            pq.push(source, 0.0, seq);
            seq += 1;
            while let Some((cur, d_cur)) = pq.pop() {
                if visited[cur] {
                    continue;
                }
                visited[cur] = true;
                if cur == target {
                    break;
                }
                for (k, &next) in outgoing[cur].iter().enumerate() {
                    let e = edge_rows[cur][k];
                    let d = d_cur + weights[e];
                    if overflows(weights[e], d) {
                        return graph_err(
                            heap,
                            "Dijkstra distance calculation exceeded the finite number range",
                        );
                    }
                    if d < dist[next] {
                        dist[next] = d;
                        prev_node[next] = Some(cur);
                        prev_edge[next] = Some(e);
                        if !visited[next] {
                            pq.push(next, d, seq);
                            seq += 1;
                        }
                    }
                }
            }
            if dist[target].is_infinite() {
                None
            } else {
                let (p, e) = trace_path_positions(g, target, &prev_node, &prev_edge);
                Some((p, e, dist[target]))
            }
        }
    };
    ok_some_path(heap, gid, outcome)
}

pub(super) fn astar(
    heap: &mut Heap,
    src_val: &Value,
    dst_val: &Value,
    weights_val: &Value,
    heur_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let s_idx = expect_int(src_val)?;
    let t_idx = expect_int(dst_val)?;
    let weights = read_floats(heap, weights_val)?;
    let heuristics = read_floats(heap, heur_val)?;
    let gid = expect_graph_id(heap, val)?;
    let outcome = {
        let g = expect_graph(heap, val)?;
        let Some(source) = node_at(g, s_idx) else {
            return graph_err(heap, &missing_node_msg(s_idx));
        };
        let Some(target) = node_at(g, t_idx) else {
            return graph_err(heap, &missing_node_msg(t_idx));
        };
        if weights.iter().any(|&w| w.is_nan() || w < 0.0) {
            return graph_err(heap, "A* algorithm requires non-negative edge weights");
        }
        charge(fuel, g.nodes.len() + g.edges.len())?;
        let check_h = |p: usize| -> Result<f64, &'static str> {
            let h = heuristics[p];
            if h.is_finite() {
                Ok(h)
            } else {
                Err("A* algorithm requires finite heuristic values")
            }
        };
        if source == target {
            if let Err(msg) = check_h(source) {
                return graph_err(heap, msg);
            }
            Some((vec![g.nodes[source].index], Vec::new(), 0.0))
        } else {
            let (outgoing, edge_rows) = rows(g, false);
            let n = g.nodes.len();
            let mut scores = vec![f64::INFINITY; n];
            let mut prev_node = vec![None; n];
            let mut prev_edge = vec![None; n];
            let mut visited = vec![false; n];
            scores[source] = 0.0;
            let mut open = MinHeap::new(n, false);
            let mut seq = 0usize;
            let h0 = match check_h(source) {
                Ok(h) => h,
                Err(msg) => return graph_err(heap, msg),
            };
            open.push(source, h0, seq);
            seq += 1;
            while let Some((cur, _)) = open.pop() {
                if visited[cur] {
                    continue;
                }
                visited[cur] = true;
                if cur == target {
                    break;
                }
                for (k, &next) in outgoing[cur].iter().enumerate() {
                    if visited[next] {
                        continue;
                    }
                    let e = edge_rows[cur][k];
                    let score = scores[cur] + weights[e];
                    if overflows(weights[e], score) {
                        return graph_err(
                            heap,
                            "A* distance calculation exceeded the finite number range",
                        );
                    }
                    if score < scores[next] {
                        scores[next] = score;
                        prev_node[next] = Some(cur);
                        prev_edge[next] = Some(e);
                        let h = match check_h(next) {
                            Ok(h) => h,
                            Err(msg) => return graph_err(heap, msg),
                        };
                        let priority = score + h;
                        if !priority.is_finite() {
                            return graph_err(
                                heap,
                                "A* priority calculation exceeded the finite number range",
                            );
                        }
                        open.push(next, priority, seq);
                        seq += 1;
                    }
                }
            }
            if scores[target].is_infinite() {
                None
            } else {
                let (p, e) = trace_path_positions(g, target, &prev_node, &prev_edge);
                Some((p, e, scores[target]))
            }
        }
    };
    ok_some_path(heap, gid, outcome)
}

#[expect(
    clippy::too_many_lines,
    reason = "Bellman-Ford relaxation and negative-cycle reachability"
)]
pub(super) fn bellman_ford(
    heap: &mut Heap,
    src_val: &Value,
    dst_val: &Value,
    weights_val: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let s_idx = expect_int(src_val)?;
    let t_idx = expect_int(dst_val)?;
    let weights = read_floats(heap, weights_val)?;
    let gid = expect_graph_id(heap, val)?;
    let outcome = {
        let g = expect_graph(heap, val)?;
        let Some(source) = node_at(g, s_idx) else {
            return graph_err(heap, &missing_node_msg(s_idx));
        };
        let Some(target) = node_at(g, t_idx) else {
            return graph_err(heap, &missing_node_msg(t_idx));
        };
        if weights
            .iter()
            .any(|&w| w.is_nan() || (w.is_infinite() && w.is_sign_negative()))
        {
            return graph_err(
                heap,
                "Bellman-Ford algorithm does not support NaN or -Infinity edge weights",
            );
        }
        let n = g.nodes.len();
        charge(fuel, n.saturating_mul(g.edges.len().max(1)))?;
        let (sources, targets) = endpoints(g);
        let (outgoing, edge_rows) = rows(g, false);
        let add_dist = |d: f64, w: f64| -> Result<f64, &'static str> {
            if (d.is_infinite() && d.is_sign_positive())
                || (w.is_infinite() && w.is_sign_positive())
            {
                return Ok(f64::INFINITY);
            }
            let sum = d + w;
            if !sum.is_finite() {
                return Err("Bellman-Ford distance calculation exceeded the finite number range");
            }
            Ok(sum)
        };
        let mut dist = vec![f64::INFINITY; n];
        let mut prev_node = vec![None; n];
        let mut prev_edge = vec![None; n];
        dist[source] = 0.0;
        let undirected = !g.directed;
        for _ in 0..n.saturating_sub(1) {
            let mut updated = false;
            for q in 0..g.edges.len() {
                let (s, t) = (sources[q], targets[q]);
                let d = match add_dist(dist[s], weights[q]) {
                    Ok(v) => v,
                    Err(m) => return graph_err(heap, m),
                };
                if d < dist[t] {
                    dist[t] = d;
                    prev_node[t] = Some(s);
                    prev_edge[t] = Some(q);
                    updated = true;
                }
                if undirected && s != t {
                    let d2 = match add_dist(dist[t], weights[q]) {
                        Ok(v) => v,
                        Err(m) => return graph_err(heap, m),
                    };
                    if d2 < dist[s] {
                        dist[s] = d2;
                        prev_node[s] = Some(t);
                        prev_edge[s] = Some(q);
                        updated = true;
                    }
                }
            }
            if !updated {
                break;
            }
        }
        let mut affected = vec![false; n];
        let mut queue = Vec::new();
        for q in 0..g.edges.len() {
            let (s, t) = (sources[q], targets[q]);
            let d = match add_dist(dist[s], weights[q]) {
                Ok(v) => v,
                Err(m) => return graph_err(heap, m),
            };
            if d < dist[t] && !affected[t] {
                affected[t] = true;
                queue.push(t);
            }
            if undirected && s != t {
                let d2 = match add_dist(dist[t], weights[q]) {
                    Ok(v) => v,
                    Err(m) => return graph_err(heap, m),
                };
                if d2 < dist[s] && !affected[s] {
                    affected[s] = true;
                    queue.push(s);
                }
            }
        }
        let mut q_idx = 0;
        while q_idx < queue.len() {
            let cur = queue[q_idx];
            q_idx += 1;
            for (k, &next) in outgoing[cur].iter().enumerate() {
                let w = weights[edge_rows[cur][k]];
                if !(affected[next] || (w.is_infinite() && w.is_sign_positive())) {
                    affected[next] = true;
                    queue.push(next);
                }
            }
        }
        let cycle_msg = format!(
            "Negative cycle affects path to node {}",
            g.nodes[target].index
        );
        if affected[target] {
            return graph_err(heap, &cycle_msg);
        }
        if dist[target].is_infinite() {
            None
        } else {
            let mut steps = 0usize;
            let mut cur = Some(target);
            while let Some(p) = cur {
                steps += 1;
                if steps > n {
                    return graph_err(heap, &cycle_msg);
                }
                cur = prev_node[p];
            }
            let (p, e) = trace_path_positions(g, target, &prev_node, &prev_edge);
            Some((p, e, dist[target]))
        }
    };
    ok_some_path(heap, gid, outcome)
}
