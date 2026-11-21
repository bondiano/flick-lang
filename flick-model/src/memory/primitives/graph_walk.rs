//! Counted `Graph.Walk` traversals and walker materializers (`dfs`, `bfs`, `topo`, `nodes`, `edges`, `externals`).

use std::collections::VecDeque;

use super::graph_algos::is_acyclic;
use super::graph_ops::{
    adjacencies, build_node_walker, direction_of, expect_graph, expect_graph_id, expect_int,
    graph_err, missing_node_msg, node_at, opt_inner, rows,
};
use super::list_ops::slice_reps;
use super::{Data, Error, GraphData, GraphWalkerData, Heap, Value, charge};

pub(super) fn traversal_radius(
    heap: &Heap,
    val: &Value,
    fallback: f64,
) -> Result<Result<f64, String>, Error> {
    match opt_inner(heap, val)? {
        Some(v) => {
            let r = expect_int(v)?;
            if r < 0 {
                Ok(Err(
                    "Traversal radius must be a non-negative integer or Infinity".into(),
                ))
            } else {
                Ok(Ok(f64::from(r)))
            }
        }
        None => Ok(Ok(fallback)),
    }
}

fn starts_of(heap: &Heap, g: &GraphData, val: &Value) -> Result<Result<Vec<usize>, String>, Error> {
    let mut out = Vec::new();
    if let Some(list) = opt_inner(heap, val)? {
        for r in &slice_reps(heap, list)? {
            let idx = expect_int(r)?;
            let Some(p) = node_at(g, idx) else {
                return Ok(Err(missing_node_msg(idx)));
            };
            out.push(p);
        }
    }
    Ok(Ok(out))
}

fn reached_within(
    n: usize,
    starts: &[usize],
    primary: &[Vec<usize>],
    secondary: Option<&[Vec<usize>]>,
    radius: f64,
) -> Vec<bool> {
    let mut reached = vec![false; n];
    let mut queue = VecDeque::new();
    for &p in starts {
        if !reached[p] {
            reached[p] = true;
            queue.push_back((p, 0.0));
        }
    }
    while let Some((cur, depth)) = queue.pop_front() {
        if depth >= radius {
            continue;
        }
        for rows in [Some(primary), secondary].into_iter().flatten() {
            for &next in &rows[cur] {
                if !reached[next] {
                    reached[next] = true;
                    queue.push_back((next, depth + 1.0));
                }
            }
        }
    }
    reached
}

pub(super) fn bfs_positions(
    g: &GraphData,
    starts: &[usize],
    direction: &str,
    radius: f64,
) -> Vec<usize> {
    let (primary, secondary) = adjacencies(g, direction);
    let mut discovered = vec![false; g.nodes.len()];
    let mut order = Vec::new();
    let mut depths = Vec::new();
    for &p in starts {
        if !discovered[p] {
            discovered[p] = true;
            order.push(p);
            depths.push(0.0);
        }
    }
    let mut head = 0;
    while head < order.len() {
        let (cur, depth) = (order[head], depths[head]);
        head += 1;
        if depth >= radius {
            continue;
        }
        for rows in [Some(primary.as_slice()), secondary.as_deref()]
            .into_iter()
            .flatten()
        {
            for &next in &rows[cur] {
                if !discovered[next] {
                    discovered[next] = true;
                    order.push(next);
                    depths.push(depth + 1.0);
                }
            }
        }
    }
    order
}

fn dfs_positions(g: &GraphData, starts: &[usize], direction: &str, radius: f64) -> Vec<usize> {
    let (primary, secondary) = adjacencies(g, direction);
    let reached = if radius.is_infinite() {
        None
    } else {
        Some(reached_within(
            g.nodes.len(),
            starts,
            &primary,
            secondary.as_deref(),
            radius,
        ))
    };
    let mut yielded = vec![false; g.nodes.len()];
    let mut stack: Vec<usize> = starts.iter().rev().copied().collect();
    let mut out = Vec::new();
    while let Some(cur) = stack.pop() {
        if yielded[cur] {
            continue;
        }
        if let Some(sec) = &secondary {
            for &next in sec[cur].iter().rev() {
                if reached.as_ref().is_none_or(|r| r[next]) && !yielded[next] {
                    stack.push(next);
                }
            }
        }
        for &next in primary[cur].iter().rev() {
            if reached.as_ref().is_none_or(|r| r[next]) && !yielded[next] {
                stack.push(next);
            }
        }
        yielded[cur] = true;
        out.push(cur);
    }
    out
}

fn dfs_post_positions(g: &GraphData, starts: &[usize], direction: &str, radius: f64) -> Vec<usize> {
    let (primary, secondary) = adjacencies(g, direction);
    let reached = if radius.is_infinite() {
        None
    } else {
        Some(reached_within(
            g.nodes.len(),
            starts,
            &primary,
            secondary.as_deref(),
            radius,
        ))
    };
    let mut discovered = vec![false; g.nodes.len()];
    let mut stack: Vec<(usize, usize, usize)> = Vec::new();
    let mut push = |node: usize, stack: &mut Vec<(usize, usize, usize)>| {
        if reached.as_ref().is_none_or(|r| r[node]) && !discovered[node] {
            discovered[node] = true;
            stack.push((node, 0, 0));
        }
    };
    let mut out = Vec::new();
    let mut next_start = 0;
    loop {
        while stack.is_empty() && next_start < starts.len() {
            push(starts[next_start], &mut stack);
            next_start += 1;
        }
        let Some(top) = stack.last_mut() else {
            return out;
        };
        if top.1 < primary[top.0].len() {
            let child = primary[top.0][top.1];
            top.1 += 1;
            push(child, &mut stack);
            continue;
        }
        if let Some(sec) = &secondary
            && top.2 < sec[top.0].len()
        {
            let child = sec[top.0][top.2];
            top.2 += 1;
            push(child, &mut stack);
            continue;
        }
        let (node, _, _) = stack.pop().expect("non-empty");
        out.push(node);
    }
}

fn search_op(
    heap: &mut Heap,
    opts: &Value,
    val: &Value,
    fuel: &mut u64,
    walk: fn(&GraphData, &[usize], &str, f64) -> Vec<usize>,
) -> Result<Value, Error> {
    let radius = match traversal_radius(heap, heap.named_field(opts, "radius")?, f64::INFINITY)? {
        Ok(r) => r,
        Err(msg) => return graph_err(heap, &msg),
    };
    let dir = direction_of(heap, heap.named_field(opts, "direction")?)?;
    let gid = expect_graph_id(heap, val)?;
    let positions = {
        let g = expect_graph(heap, val)?;
        let starts = match starts_of(heap, g, heap.named_field(opts, "start")?)? {
            Ok(s) => s,
            Err(msg) => return graph_err(heap, &msg),
        };
        charge(fuel, g.nodes.len() + g.edges.len())?;
        walk(g, &starts, &dir, radius)
    };
    let w = build_node_walker(heap, gid, &positions)?;
    heap.construct("Prelude.Result.Ok", vec![w])
}

pub(super) fn dfs_op(
    heap: &mut Heap,
    opts: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    search_op(heap, opts, val, fuel, dfs_positions)
}

pub(super) fn bfs_op(
    heap: &mut Heap,
    opts: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    search_op(heap, opts, val, fuel, bfs_positions)
}

pub(super) fn dfs_post_order_op(
    heap: &mut Heap,
    opts: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    search_op(heap, opts, val, fuel, dfs_post_positions)
}

pub(super) fn topo_op(
    heap: &mut Heap,
    opts: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let gid = expect_graph_id(heap, val)?;
    let out = {
        let g = expect_graph(heap, val)?;
        if !g.directed {
            return graph_err(heap, "Cannot perform topological sort on undirected graph");
        }
        if !is_acyclic(g) {
            return graph_err(heap, "Cannot perform topological sort on cyclic graph");
        }
        let initials = match starts_of(heap, g, heap.named_field(opts, "initials")?)? {
            Ok(s) => s,
            Err(msg) => return graph_err(heap, &msg),
        };
        charge(fuel, g.nodes.len() + g.edges.len())?;
        let outgoing = rows(g, false).0;
        let incoming = rows(g, true).0;
        let n = g.nodes.len();
        let mut in_deg: Vec<usize> = (0..n).map(|p| incoming[p].len()).collect();
        let mut is_init = vec![false; n];
        let mut queue = Vec::new();
        for p in initials {
            if in_deg[p] != 0 {
                return graph_err(
                    heap,
                    &format!("Initial node {} has incoming edges", g.nodes[p].index),
                );
            }
            is_init[p] = true;
            queue.push(p);
        }
        for p in 0..n {
            if in_deg[p] == 0 && !is_init[p] {
                queue.push(p);
            }
        }
        let mut done = vec![false; n];
        let mut order = Vec::with_capacity(n);
        let mut q_idx = 0;
        while q_idx < queue.len() {
            let cur = queue[q_idx];
            q_idx += 1;
            if done[cur] {
                continue;
            }
            done[cur] = true;
            for &next in &outgoing[cur] {
                if !done[next] {
                    in_deg[next] -= 1;
                    if in_deg[next] == 0 {
                        queue.push(next);
                    }
                }
            }
            order.push(cur);
        }
        if order.len() < n {
            return graph_err(heap, "Cannot perform topological sort on cyclic graph");
        }
        order
    };
    let w = build_node_walker(heap, gid, &out)?;
    heap.construct("Prelude.Result.Ok", vec![w])
}

pub(super) fn nodes_walker(heap: &mut Heap, val: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let gid = expect_graph_id(heap, val)?;
    let n = expect_graph(heap, val)?.nodes.len();
    charge(fuel, n)?;
    let positions: Vec<usize> = (0..n).collect();
    build_node_walker(heap, gid, &positions)
}

pub(super) fn edges_walker(heap: &mut Heap, val: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let er: Vec<(i32, i32, i32, Value)> = {
        let g = expect_graph(heap, val)?;
        charge(fuel, g.edges.len())?;
        g.edges
            .iter()
            .map(|e| (e.index, e.source, e.target, e.data.representation()))
            .collect()
    };
    let mut indices = Vec::with_capacity(er.len());
    let mut values = Vec::with_capacity(er.len());
    for (idx, src, dst, rep) in er {
        let data = match heap.dup(&rep) {
            Ok(d) => d,
            Err(err) => {
                heap.drop_values(values)?;
                return Err(err);
            }
        };
        let rec = match heap.record(vec![
            ("source".into(), Value::Int(src)),
            ("target".into(), Value::Int(dst)),
            ("data".into(), data),
        ]) {
            Ok(r) => r,
            Err(err) => {
                heap.drop_values(values)?;
                return Err(err);
            }
        };
        indices.push(Value::Int(idx));
        values.push(rec);
    }
    Ok(heap.allocate(Data::GraphWalker(GraphWalkerData { indices, values })))
}

pub(super) fn externals_walker(
    heap: &mut Heap,
    opts: &Value,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let dir = direction_of(heap, heap.named_field(opts, "direction")?)?;
    let gid = expect_graph_id(heap, val)?;
    let positions = {
        let g = expect_graph(heap, val)?;
        charge(fuel, g.nodes.len())?;
        let adj = if dir == "incoming" { &g.radj } else { &g.adj };
        g.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| adj.get(&n.index).is_none_or(Vec::is_empty))
            .map(|(p, _)| p)
            .collect::<Vec<_>>()
    };
    build_node_walker(heap, gid, &positions)
}

pub(super) fn walker_indices(heap: &mut Heap, val: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let reps = {
        let Value::Object(tok) = val else {
            return Err(Error::PatternKind("GraphWalker"));
        };
        let Data::GraphWalker(w) = &heap.object(tok.id)?.data else {
            return Err(Error::PatternKind("GraphWalker"));
        };
        charge(fuel, w.indices.len())?;
        w.indices
            .iter()
            .map(Value::representation)
            .collect::<Vec<_>>()
    };
    let mut out = Vec::with_capacity(reps.len());
    for r in reps {
        match heap.dup(&r) {
            Ok(v) => out.push(v),
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", out)
}

pub(super) fn walker_values(heap: &mut Heap, val: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let reps = {
        let Value::Object(tok) = val else {
            return Err(Error::PatternKind("GraphWalker"));
        };
        let Data::GraphWalker(w) = &heap.object(tok.id)?.data else {
            return Err(Error::PatternKind("GraphWalker"));
        };
        charge(fuel, w.values.len())?;
        w.values
            .iter()
            .map(Value::representation)
            .collect::<Vec<_>>()
    };
    let mut out = Vec::with_capacity(reps.len());
    for r in reps {
        match heap.dup(&r) {
            Ok(v) => out.push(v),
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        }
    }
    heap.construct("$list", out)
}

pub(super) fn walker_entries(heap: &mut Heap, val: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let pairs = {
        let Value::Object(tok) = val else {
            return Err(Error::PatternKind("GraphWalker"));
        };
        let Data::GraphWalker(w) = &heap.object(tok.id)?.data else {
            return Err(Error::PatternKind("GraphWalker"));
        };
        charge(fuel, w.indices.len())?;
        w.indices
            .iter()
            .zip(w.values.iter())
            .map(|(k, v)| (k.representation(), v.representation()))
            .collect::<Vec<_>>()
    };
    let mut out = Vec::with_capacity(pairs.len());
    for (k_rep, v_rep) in pairs {
        let k = match heap.dup(&k_rep) {
            Ok(v) => v,
            Err(err) => {
                heap.drop_values(out)?;
                return Err(err);
            }
        };
        let v = match heap.dup(&v_rep) {
            Ok(v) => v,
            Err(err) => {
                heap.drop_value(k)?;
                heap.drop_values(out)?;
                return Err(err);
            }
        };
        out.push(heap.construct("$tuple", vec![k, v])?);
    }
    heap.construct("$list", out)
}
