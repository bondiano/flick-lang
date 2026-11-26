//! Counted `Graph` cycle detection, bipartiteness, Hopcroft-Karp matching, and reachability.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::mem;

use super::graph_ops::{
    adjacencies, direction_of, edge_at, endpoints, expect_graph, expect_int, graph_defect,
    graph_err, index_list, missing_node_msg, neighbor_of, node_at, rows,
};
use super::{Error, Fuel, GraphData, Heap, Value, charge};

pub(super) fn find_cycle(heap: &mut Heap, val: &Value, fuel: Fuel<'_>) -> Result<Value, Error> {
    let found = {
        let g = expect_graph(heap, val)?;
        charge(fuel, g.nodes.len() + g.edges.len())?;
        let mut colors: BTreeMap<i32, u8> = BTreeMap::new();
        let mut parent_node: BTreeMap<i32, i32> = BTreeMap::new();
        let mut parent_edge: BTreeMap<i32, i32> = BTreeMap::new();
        let mut res = None;
        'outer: for start in &g.nodes {
            if *colors.get(&start.index).unwrap_or(&0) != 0 {
                continue;
            }
            colors.insert(start.index, 1);
            let mut stack = vec![(start.index, 0usize)];
            while let Some(top) = stack.last_mut() {
                let adj = g.adj.get(&top.0).map_or(&[][..], Vec::as_slice);
                if top.1 >= adj.len() {
                    colors.insert(top.0, 2);
                    stack.pop();
                    continue;
                }
                let ei = adj[top.1];
                top.1 += 1;
                if !g.directed && parent_edge.get(&top.0) == Some(&ei) {
                    continue;
                }
                let node = top.0;
                let Some(q) = edge_at(g, ei) else {
                    continue;
                };
                let next = neighbor_of(g, node, &g.edges[q]);
                match *colors.get(&next).unwrap_or(&0) {
                    1 => {
                        let mut path = vec![node];
                        let mut edges = Vec::new();
                        let mut cursor = node;
                        while cursor != next {
                            edges.push(parent_edge[&cursor]);
                            cursor = parent_node[&cursor];
                            path.push(cursor);
                        }
                        path.reverse();
                        edges.reverse();
                        path.push(next);
                        edges.push(ei);
                        res = Some((path, edges));
                        break 'outer;
                    }
                    0 => {
                        colors.insert(next, 1);
                        parent_node.insert(next, node);
                        parent_edge.insert(next, ei);
                        stack.push((next, 0));
                    }
                    _ => {}
                }
            }
        }
        res
    };
    match found {
        Some((path, edges)) => {
            let p_val = index_list(heap, &path)?;
            let e_val = index_list(heap, &edges)?;
            let rec = heap.record(vec![("path".into(), p_val), ("edges".into(), e_val)])?;
            heap.construct("Prelude.Option.Some", vec![rec])
        }
        None => heap.construct("Prelude.Option.None", vec![]),
    }
}

pub(super) fn is_acyclic(g: &GraphData) -> bool {
    let (outgoing, edge_rows) = rows(g, false);
    let n = g.nodes.len();
    if !g.directed {
        let mut visited = vec![false; n];
        for start in 0..n {
            if visited[start] {
                continue;
            }
            visited[start] = true;
            let mut stack = vec![(start, None::<usize>)];
            while let Some((node, parent)) = stack.pop() {
                for (k, &next) in outgoing[node].iter().enumerate() {
                    if parent == Some(edge_rows[node][k]) {
                        continue;
                    }
                    if visited[next] {
                        return false;
                    }
                    visited[next] = true;
                    stack.push((next, Some(edge_rows[node][k])));
                }
            }
        }
        return true;
    }
    let mut colors = vec![0u8; n];
    for start in 0..n {
        if colors[start] != 0 {
            continue;
        }
        colors[start] = 1;
        let mut stack = vec![(start, 0usize)];
        while let Some(top) = stack.last_mut() {
            let node = top.0;
            if top.1 < outgoing[node].len() {
                let next = outgoing[node][top.1];
                top.1 += 1;
                if colors[next] == 1 {
                    return false;
                }
                if colors[next] == 0 {
                    colors[next] = 1;
                    stack.push((next, 0));
                }
            } else {
                colors[node] = 2;
                stack.pop();
            }
        }
    }
    true
}

fn two_colors(g: &GraphData) -> Option<Vec<i8>> {
    let outgoing = rows(g, false).0;
    let n = g.nodes.len();
    let mut colors = vec![-1i8; n];
    for start in 0..n {
        if colors[start] != -1 {
            continue;
        }
        colors[start] = 0;
        let mut queue = VecDeque::from([start]);
        while let Some(cur) = queue.pop_front() {
            for &next in &outgoing[cur] {
                if colors[next] == -1 {
                    colors[next] = 1 - colors[cur];
                    queue.push_back(next);
                } else if colors[next] == colors[cur] {
                    return None;
                }
            }
        }
    }
    Some(colors)
}

pub(super) fn is_bipartite(heap: &Heap, val: &Value) -> Result<Value, Error> {
    let g = expect_graph(heap, val)?;
    if g.directed {
        return Err(graph_defect(
            "Cannot determine bipartite status of directed graph",
        ));
    }
    Ok(Value::Bool(two_colors(g).is_some()))
}

#[expect(clippy::too_many_lines, reason = "Hopcroft-Karp bipartite matching")]
pub(super) fn maximum_bipartite_matching(
    heap: &mut Heap,
    val: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let matches = {
        let g = expect_graph(heap, val)?;
        if g.directed {
            return graph_err(heap, "Cannot find bipartite matching of directed graph");
        }
        let Some(colors) = two_colors(g) else {
            return graph_err(
                heap,
                "Cannot find bipartite matching of non-bipartite graph",
            );
        };
        let n = g.nodes.len();
        charge(fuel, n + g.edges.len())?;
        let (sources, targets) = endpoints(g);
        let mut adj: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
        let mut seen: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
        for q in 0..g.edges.len() {
            let (mut left, mut right) = (sources[q], targets[q]);
            if colors[left] != 0 {
                mem::swap(&mut left, &mut right);
            }
            if seen[left].insert(right) {
                adj[left].push((right, q));
            }
        }
        let mut match_left = vec![None::<usize>; n];
        let mut match_right = vec![None::<usize>; n];
        let mut match_edge = vec![None::<usize>; n];
        let mut distance = vec![usize::MAX; n];
        loop {
            let mut queue = VecDeque::new();
            let mut shortest = usize::MAX;
            for left in 0..n {
                if colors[left] != 0 {
                    continue;
                }
                if match_left[left].is_none() {
                    distance[left] = 0;
                    queue.push_back(left);
                } else {
                    distance[left] = usize::MAX;
                }
            }
            while let Some(left) = queue.pop_front() {
                if distance[left] >= shortest {
                    continue;
                }
                for &(right, _) in &adj[left] {
                    match match_right[right] {
                        None => shortest = distance[left] + 1,
                        Some(next) if distance[next] == usize::MAX => {
                            distance[next] = distance[left] + 1;
                            queue.push_back(next);
                        }
                        _ => {}
                    }
                }
            }
            if shortest == usize::MAX {
                break;
            }
            for start in 0..n {
                if colors[start] != 0 || match_left[start].is_some() {
                    continue;
                }
                let mut stack = vec![(start, 0usize, 0usize, 0usize)];
                while let Some(top) = stack.last_mut() {
                    let left = top.0;
                    if top.1 >= adj[left].len() {
                        distance[left] = usize::MAX;
                        stack.pop();
                        continue;
                    }
                    let (right, edge) = adj[left][top.1];
                    top.1 += 1;
                    match match_right[right] {
                        None if distance[left] + 1 == shortest => {
                            match_left[left] = Some(right);
                            match_right[right] = Some(left);
                            match_edge[left] = Some(edge);
                            for i in (1..stack.len()).rev() {
                                let (_, _, via_r, via_e) = stack[i];
                                let parent_l = stack[i - 1].0;
                                match_left[parent_l] = Some(via_r);
                                match_right[via_r] = Some(parent_l);
                                match_edge[parent_l] = Some(via_e);
                            }
                            break;
                        }
                        Some(next) if distance[next] == distance[left] + 1 => {
                            stack.push((next, 0, right, edge));
                        }
                        _ => {}
                    }
                }
            }
        }
        let mut out = Vec::new();
        for left in 0..n {
            if let (Some(r), Some(e)) = (match_left[left], match_edge[left]) {
                out.push((g.nodes[left].index, g.nodes[r].index, g.edges[e].index));
            }
        }
        out
    };
    let mut recs = Vec::with_capacity(matches.len());
    for (left, right, edge) in matches {
        recs.push(heap.record(vec![
            ("left".into(), Value::Int(left)),
            ("right".into(), Value::Int(right)),
            ("edge".into(), Value::Int(edge)),
        ])?);
    }
    let list = heap.construct("$list", recs)?;
    heap.construct("Prelude.Result.Ok", vec![list])
}

fn reach(g: &GraphData, source: usize, direction: &str) -> Vec<i32> {
    let dir = if g.directed { direction } else { "outgoing" };
    let (primary, secondary) = adjacencies(g, dir);
    let mut distances = vec![-1i32; g.nodes.len()];
    distances[source] = 0;
    let mut queue = VecDeque::from([source]);
    while let Some(cur) = queue.pop_front() {
        for rows in [Some(primary.as_slice()), secondary.as_deref()]
            .into_iter()
            .flatten()
        {
            for &next in &rows[cur] {
                if distances[next] == -1 {
                    distances[next] = distances[cur] + 1;
                    queue.push_back(next);
                }
            }
        }
    }
    distances
}

pub(super) fn unweighted_distances(
    heap: &mut Heap,
    src_val: &Value,
    opts: &Value,
    val: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let src = expect_int(src_val)?;
    let dir = direction_of(heap, heap.named_field(opts, "direction")?)?;
    let pairs = {
        let g = expect_graph(heap, val)?;
        let Some(sp) = node_at(g, src) else {
            return graph_err(heap, &missing_node_msg(src));
        };
        charge(fuel, g.nodes.len() + g.edges.len())?;
        reach(g, sp, &dir)
            .into_iter()
            .enumerate()
            .filter(|&(_, d)| d != -1)
            .map(|(p, d)| (g.nodes[p].index, d))
            .collect::<Vec<_>>()
    };
    let mut tuples = Vec::with_capacity(pairs.len());
    for (idx, d) in pairs {
        tuples.push(heap.construct("$tuple", vec![Value::Int(idx), Value::Int(d)])?);
    }
    let list = heap.construct("$list", tuples)?;
    heap.construct("Prelude.Result.Ok", vec![list])
}

pub(super) fn has_path(
    heap: &mut Heap,
    src_val: &Value,
    dst_val: &Value,
    opts: &Value,
    val: &Value,
    fuel: Fuel<'_>,
) -> Result<Value, Error> {
    let s = expect_int(src_val)?;
    let t = expect_int(dst_val)?;
    let dir = direction_of(heap, heap.named_field(opts, "direction")?)?;
    let found = {
        let g = expect_graph(heap, val)?;
        let Some(sp) = node_at(g, s) else {
            return graph_err(heap, &missing_node_msg(s));
        };
        let Some(tp) = node_at(g, t) else {
            return graph_err(heap, &missing_node_msg(t));
        };
        charge(fuel, g.nodes.len() + g.edges.len())?;
        sp == tp || reach(g, sp, &dir)[tp] != -1
    };
    heap.construct("Prelude.Result.Ok", vec![Value::Bool(found)])
}
