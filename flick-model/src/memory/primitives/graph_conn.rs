//! Counted `Graph` connectivity: components, Kosaraju SCC, trees, and Tarjan low-link analysis.

use std::collections::{BTreeSet, VecDeque};

use super::graph_ops::{adjacencies, endpoints, expect_graph, graph_defect, index_list, rows};
use super::{Error, GraphData, Heap, Value, charge};

fn components_positions(
    n: usize,
    primary: &[Vec<usize>],
    secondary: Option<&[Vec<usize>]>,
) -> Vec<Vec<usize>> {
    let mut visited = vec![false; n];
    let mut out = Vec::new();
    for start in 0..n {
        if visited[start] {
            continue;
        }
        let mut comp = Vec::new();
        let mut stack = vec![start];
        while let Some(cur) = stack.pop() {
            if visited[cur] {
                continue;
            }
            visited[cur] = true;
            comp.push(cur);
            for rows in [Some(primary), secondary].into_iter().flatten() {
                for &next in &rows[cur] {
                    if !visited[next] {
                        stack.push(next);
                    }
                }
            }
        }
        out.push(comp);
    }
    out
}

pub(super) fn connected_components(
    heap: &mut Heap,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let comps = {
        let g = expect_graph(heap, val)?;
        if g.directed {
            return Err(graph_defect(
                "Cannot find connected components of directed graph",
            ));
        }
        charge(fuel, g.nodes.len() + g.edges.len())?;
        let outgoing = rows(g, false).0;
        let unique: Vec<Vec<usize>> = outgoing
            .into_iter()
            .map(|row| {
                let mut seen = BTreeSet::new();
                row.into_iter().filter(|&x| seen.insert(x)).collect()
            })
            .collect();
        components_positions(g.nodes.len(), &unique, None)
    };
    let g_ids: Vec<Vec<i32>> = {
        let g = expect_graph(heap, val)?;
        comps
            .iter()
            .map(|c| c.iter().map(|&p| g.nodes[p].index).collect())
            .collect()
    };
    let mut items = Vec::with_capacity(g_ids.len());
    for c in &g_ids {
        items.push(index_list(heap, c)?);
    }
    heap.construct("$list", items)
}

pub(super) fn weakly_connected_components(
    heap: &mut Heap,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let g_ids: Vec<Vec<i32>> = {
        let g = expect_graph(heap, val)?;
        if !g.directed {
            return Err(graph_defect(
                "Cannot find weakly connected components of undirected graph",
            ));
        }
        charge(fuel, g.nodes.len() + g.edges.len())?;
        let (p, s) = adjacencies(g, "undirected");
        components_positions(g.nodes.len(), &p, s.as_deref())
            .into_iter()
            .map(|c| c.into_iter().map(|i| g.nodes[i].index).collect())
            .collect()
    };
    let mut items = Vec::with_capacity(g_ids.len());
    for c in &g_ids {
        items.push(index_list(heap, c)?);
    }
    heap.construct("$list", items)
}

pub(super) fn strongly_connected_components(
    heap: &mut Heap,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let g_ids: Vec<Vec<i32>> = {
        let g = expect_graph(heap, val)?;
        if !g.directed {
            return Err(graph_defect(
                "Cannot find strongly connected components of undirected graph",
            ));
        }
        charge(fuel, g.nodes.len() + g.edges.len())?;
        let n = g.nodes.len();
        let outgoing = rows(g, false).0;
        let incoming = rows(g, true).0;
        let mut visited = vec![false; n];
        let mut finish = Vec::with_capacity(n);
        for start in 0..n {
            if visited[start] {
                continue;
            }
            visited[start] = true;
            let mut stack = vec![(start, 0usize)];
            while let Some(top) = stack.last_mut() {
                let node = top.0;
                if top.1 < outgoing[node].len() {
                    let next = outgoing[node][top.1];
                    top.1 += 1;
                    if !visited[next] {
                        visited[next] = true;
                        stack.push((next, 0));
                    }
                } else {
                    finish.push(node);
                    stack.pop();
                }
            }
        }
        visited.fill(false);
        let mut out = Vec::new();
        for &start in finish.iter().rev() {
            if visited[start] {
                continue;
            }
            let mut comp = Vec::new();
            let mut stack = vec![start];
            while let Some(node) = stack.pop() {
                if visited[node] {
                    continue;
                }
                visited[node] = true;
                comp.push(g.nodes[node].index);
                for &prev in &incoming[node] {
                    if !visited[prev] {
                        stack.push(prev);
                    }
                }
            }
            out.push(comp);
        }
        out
    };
    let mut items = Vec::with_capacity(g_ids.len());
    for c in &g_ids {
        items.push(index_list(heap, c)?);
    }
    heap.construct("$list", items)
}

fn reaches_all(n: usize, primary: &[Vec<usize>], secondary: Option<&[Vec<usize>]>) -> bool {
    if n == 0 {
        return true;
    }
    let mut visited = vec![false; n];
    visited[0] = true;
    let mut count = 1usize;
    let mut queue = VecDeque::from([0usize]);
    while let Some(cur) = queue.pop_front() {
        for rows in [Some(primary), secondary].into_iter().flatten() {
            for &next in &rows[cur] {
                if !visited[next] {
                    visited[next] = true;
                    count += 1;
                    queue.push_back(next);
                }
            }
        }
    }
    count == n
}

pub(super) fn is_connected(heap: &Heap, val: &Value) -> Result<Value, Error> {
    let g = expect_graph(heap, val)?;
    if g.directed {
        return Err(graph_defect(
            "Cannot find connected components of directed graph",
        ));
    }
    Ok(Value::Bool(reaches_all(
        g.nodes.len(),
        &rows(g, false).0,
        None,
    )))
}

pub(super) fn is_weakly_connected(heap: &Heap, val: &Value) -> Result<Value, Error> {
    let g = expect_graph(heap, val)?;
    if !g.directed {
        return Err(graph_defect(
            "Cannot find weakly connected components of undirected graph",
        ));
    }
    let (p, s) = adjacencies(g, "undirected");
    Ok(Value::Bool(reaches_all(g.nodes.len(), &p, s.as_deref())))
}

pub(super) fn is_strongly_connected(heap: &Heap, val: &Value) -> Result<Value, Error> {
    let g = expect_graph(heap, val)?;
    if !g.directed {
        return Err(graph_defect(
            "Cannot find strongly connected components of undirected graph",
        ));
    }
    Ok(Value::Bool(
        reaches_all(g.nodes.len(), &rows(g, false).0, None)
            && reaches_all(g.nodes.len(), &rows(g, true).0, None),
    ))
}

pub(super) fn is_tree(heap: &Heap, val: &Value) -> Result<Value, Error> {
    let g = expect_graph(heap, val)?;
    if g.directed {
        return Err(graph_defect(
            "Cannot determine tree status of directed graph",
        ));
    }
    Ok(Value::Bool(
        !g.nodes.is_empty()
            && g.edges.len() == g.nodes.len() - 1
            && reaches_all(g.nodes.len(), &rows(g, false).0, None),
    ))
}

struct LowLinkResult {
    bridges: Vec<i32>,
    points: Vec<i32>,
    components: Vec<Vec<i32>>,
}

#[expect(clippy::too_many_lines, reason = "iterative Tarjan low-link DFS")]
fn low_links(g: &GraphData) -> Result<LowLinkResult, Error> {
    if g.directed {
        return Err(graph_defect(
            "Cannot analyze undirected connectivity of directed graph",
        ));
    }
    let n = g.nodes.len();
    let (outgoing, edge_rows) = rows(g, false);
    let (sources, targets) = endpoints(g);
    let mut discovered = vec![None::<usize>; n];
    let mut low = vec![0usize; n];
    let mut parent_node = vec![None::<usize>; n];
    let mut parent_edge = vec![None::<usize>; n];
    let mut children = vec![0usize; n];
    let mut bridge_marks = vec![false; g.edges.len()];
    let mut point_marks = vec![false; n];
    let mut edge_stack: Vec<usize> = Vec::new();
    let mut loops = Vec::new();
    let mut loop_seen = BTreeSet::new();
    let mut components: Vec<Vec<usize>> = Vec::new();
    let mut time = 0usize;

    for start in 0..n {
        if discovered[start].is_some() {
            continue;
        }
        discovered[start] = Some(time);
        low[start] = time;
        time += 1;
        let mut stack = vec![(start, 0usize)];
        while let Some(top) = stack.last_mut() {
            let node = top.0;
            if top.1 < outgoing[node].len() {
                let k = top.1;
                top.1 += 1;
                let (e, next) = (edge_rows[node][k], outgoing[node][k]);
                if next == node {
                    if loop_seen.insert(node) {
                        loops.push(node);
                    }
                } else if parent_edge[node] == Some(e) {
                    // skip tree edge back to parent
                } else if discovered[next].is_none() {
                    children[node] += 1;
                    parent_node[next] = Some(node);
                    parent_edge[next] = Some(e);
                    discovered[next] = Some(time);
                    low[next] = time;
                    time += 1;
                    edge_stack.push(e);
                    stack.push((next, 0));
                } else if let (Some(dn), Some(dcur)) = (discovered[next], discovered[node])
                    && dn < dcur
                {
                    low[node] = low[node].min(dn);
                    edge_stack.push(e);
                }
                continue;
            }
            stack.pop();
            let Some(parent) = parent_node[node] else {
                if children[node] > 1 {
                    point_marks[node] = true;
                }
                continue;
            };
            low[parent] = low[parent].min(low[node]);
            let d_parent = discovered[parent].unwrap_or(0);
            let pe = parent_edge[node].unwrap_or(0);
            if low[node] > d_parent {
                bridge_marks[pe] = true;
            }
            if low[node] >= d_parent {
                if parent_node[parent].is_some() {
                    point_marks[parent] = true;
                }
                let mut seen = BTreeSet::new();
                let mut nodes = Vec::new();
                while let Some(e) = edge_stack.pop() {
                    for p in [sources[e], targets[e]] {
                        if seen.insert(p) {
                            nodes.push(p);
                        }
                    }
                    if e == pe {
                        break;
                    }
                }
                if !nodes.is_empty() {
                    nodes.sort_unstable();
                    components.push(nodes);
                }
            }
        }
    }
    for p in loops {
        components.push(vec![p]);
    }
    components.sort();
    let bridges: Vec<i32> = bridge_marks
        .into_iter()
        .enumerate()
        .filter(|&(_, m)| m)
        .map(|(q, _)| g.edges[q].index)
        .collect();
    let points: Vec<i32> = point_marks
        .into_iter()
        .enumerate()
        .filter(|&(_, m)| m)
        .map(|(p, _)| g.nodes[p].index)
        .collect();
    let comp_ids: Vec<Vec<i32>> = components
        .into_iter()
        .map(|c| c.into_iter().map(|p| g.nodes[p].index).collect())
        .collect();
    Ok(LowLinkResult {
        bridges,
        points,
        components: comp_ids,
    })
}

pub(super) fn bridges(heap: &mut Heap, val: &Value, fuel: &mut u64) -> Result<Value, Error> {
    let b = {
        let g = expect_graph(heap, val)?;
        charge(fuel, g.nodes.len() + g.edges.len())?;
        low_links(g)?.bridges
    };
    index_list(heap, &b)
}

pub(super) fn articulation_points(
    heap: &mut Heap,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let pts = {
        let g = expect_graph(heap, val)?;
        charge(fuel, g.nodes.len() + g.edges.len())?;
        low_links(g)?.points
    };
    index_list(heap, &pts)
}

pub(super) fn biconnected_components(
    heap: &mut Heap,
    val: &Value,
    fuel: &mut u64,
) -> Result<Value, Error> {
    let comps = {
        let g = expect_graph(heap, val)?;
        charge(fuel, g.nodes.len() + g.edges.len())?;
        low_links(g)?.components
    };
    let mut items = Vec::with_capacity(comps.len());
    for c in &comps {
        items.push(index_list(heap, c)?);
    }
    heap.construct("$list", items)
}
