//! Core `Graph` primitives dispatch and CSR / ownership helpers.

use std::collections::BTreeMap;

use flick_core::ir::Lit;
use flick_core::ir::memory::GraphIntrinsic;

use super::{
    Data, Error, GraphData, GraphEdge, GraphNode, GraphWalkerData, Heap, ObjectId, Value, charge,
    graph_algos, graph_allpaths, graph_build, graph_combine, graph_conn, graph_flow, graph_json,
    graph_paths, graph_query, graph_transform, graph_walk,
};

#[expect(clippy::too_many_lines, reason = "central GraphIntrinsic dispatch")]
pub(super) fn run(
    heap: &mut Heap,
    op: GraphIntrinsic,
    args: &[Value],
    fuel: &mut u64,
) -> Result<Value, Error> {
    charge(fuel, 1)?;
    match op {
        GraphIntrinsic::Make => graph_build::make(heap, &args[0]),
        GraphIntrinsic::Directed => Ok(alloc_graph(heap, new_graph(true))),
        GraphIntrinsic::Undirected => Ok(alloc_graph(heap, new_graph(false))),
        GraphIntrinsic::FromSnapshot => graph_build::from_snapshot(heap, &args[0], fuel),
        GraphIntrinsic::ToSnapshot => graph_build::to_snapshot(heap, &args[0], fuel),
        GraphIntrinsic::AddNode => graph_build::add_node(heap, &args[0], &args[1], fuel),
        GraphIntrinsic::AddEdge => {
            graph_build::add_edge(heap, &args[0], &args[1], &args[2], &args[3], fuel)
        }
        GraphIntrinsic::RemoveNode => graph_build::remove_node(heap, &args[0], &args[1], fuel),
        GraphIntrinsic::RemoveNodes => graph_build::remove_nodes(heap, &args[0], &args[1], fuel),
        GraphIntrinsic::RemoveEdge => graph_build::remove_edge(heap, &args[0], &args[1], fuel),
        GraphIntrinsic::RemoveEdges => graph_build::remove_edges(heap, &args[0], &args[1], fuel),
        GraphIntrinsic::UpdateNodeData => {
            graph_build::update_node_data(heap, &args[0], &args[1], &args[2], fuel)
        }
        GraphIntrinsic::UpdateEdgeData => {
            graph_build::update_edge_data(heap, &args[0], &args[1], &args[2], fuel)
        }
        GraphIntrinsic::ReplaceNodes => graph_build::replace_nodes(heap, &args[0], &args[1], fuel),
        GraphIntrinsic::ReplaceEdges => graph_build::replace_edges(heap, &args[0], &args[1], fuel),
        GraphIntrinsic::FilterMapNodesApply => {
            graph_transform::filter_map_nodes_apply(heap, &args[0], &args[1], fuel)
        }
        GraphIntrinsic::FilterMapEdgesApply => {
            graph_transform::filter_map_edges_apply(heap, &args[0], &args[1], fuel)
        }
        GraphIntrinsic::Reverse => graph_transform::reverse(heap, &args[0], fuel),
        GraphIntrinsic::Sum => graph_transform::sum(heap, &args[0], &args[1], fuel),
        GraphIntrinsic::Combine => graph_combine::combine(heap, &args[0], &args[1], &args[2], fuel),
        GraphIntrinsic::ComplementPairs => graph_combine::complement_pairs(heap, &args[0], fuel),
        GraphIntrinsic::ComplementBuild => {
            graph_combine::complement_build(heap, &args[0], &args[1], fuel)
        }
        GraphIntrinsic::Neighborhood => {
            graph_transform::neighborhood(heap, &args[0], &args[1], &args[2], fuel)
        }
        GraphIntrinsic::InducedSubgraph => {
            graph_transform::induced_subgraph(heap, &args[0], &args[1], fuel)
        }
        GraphIntrinsic::GetNode => graph_query::get_node(heap, &args[0], &args[1]),
        GraphIntrinsic::HasNode => graph_query::has_node(heap, &args[0], &args[1]),
        GraphIntrinsic::NodeCount => {
            let g = expect_graph(heap, &args[0])?;
            Ok(Value::Int(i32::try_from(g.nodes.len()).unwrap_or(i32::MAX)))
        }
        GraphIntrinsic::GetEdge => graph_query::get_edge(heap, &args[0], &args[1]),
        GraphIntrinsic::HasEdge => graph_query::has_edge(heap, &args[0], &args[1], &args[2]),
        GraphIntrinsic::EdgeCount => {
            let g = expect_graph(heap, &args[0])?;
            Ok(Value::Int(i32::try_from(g.edges.len()).unwrap_or(i32::MAX)))
        }
        GraphIntrinsic::IncidentEdges => {
            graph_query::incident_edges(heap, &args[0], &args[1], fuel)
        }
        GraphIntrinsic::OutgoingEdges => {
            graph_query::outgoing_edges(heap, &args[0], &args[1], fuel)
        }
        GraphIntrinsic::IncomingEdges => {
            graph_query::incoming_edges(heap, &args[0], &args[1], fuel)
        }
        GraphIntrinsic::EdgesBetween => {
            graph_query::edges_between(heap, &args[0], &args[1], &args[2], fuel)
        }
        GraphIntrinsic::Degree => graph_query::degree(heap, &args[0], &args[1]),
        GraphIntrinsic::OutDegree => graph_query::out_degree(heap, &args[0], &args[1]),
        GraphIntrinsic::InDegree => graph_query::in_degree(heap, &args[0], &args[1]),
        GraphIntrinsic::Neighbors => graph_query::neighbors(heap, &args[0], &args[1], fuel),
        GraphIntrinsic::Successors => graph_query::successors(heap, &args[0], &args[1], fuel),
        GraphIntrinsic::Predecessors => graph_query::predecessors(heap, &args[0], &args[1], fuel),
        GraphIntrinsic::NeighborsDirected => {
            graph_query::neighbors_directed(heap, &args[0], &args[1], &args[2], fuel)
        }
        GraphIntrinsic::LabelText => graph_query::label_text_val(heap, &args[0], fuel),
        GraphIntrinsic::ToGraphVizRender => {
            graph_query::to_graph_viz_render(heap, &args[0], &args[1], &args[2], &args[3], fuel)
        }
        GraphIntrinsic::ToMermaidRender => {
            graph_query::to_mermaid_render(heap, &args[0], &args[1], &args[2], fuel)
        }
        GraphIntrinsic::FindCycle => graph_algos::find_cycle(heap, &args[0], fuel),
        GraphIntrinsic::IsAcyclic => {
            let g = expect_graph(heap, &args[0])?;
            Ok(Value::Bool(graph_algos::is_acyclic(g)))
        }
        GraphIntrinsic::IsBipartite => graph_algos::is_bipartite(heap, &args[0]),
        GraphIntrinsic::MaximumBipartiteMatching => {
            graph_algos::maximum_bipartite_matching(heap, &args[0], fuel)
        }
        GraphIntrinsic::UnweightedDistances => {
            graph_algos::unweighted_distances(heap, &args[0], &args[1], &args[2], fuel)
        }
        GraphIntrinsic::HasPath => {
            graph_algos::has_path(heap, &args[0], &args[1], &args[2], &args[3], fuel)
        }
        GraphIntrinsic::ConnectedComponents => {
            graph_conn::connected_components(heap, &args[0], fuel)
        }
        GraphIntrinsic::Bridges => graph_conn::bridges(heap, &args[0], fuel),
        GraphIntrinsic::ArticulationPoints => graph_conn::articulation_points(heap, &args[0], fuel),
        GraphIntrinsic::BiconnectedComponents => {
            graph_conn::biconnected_components(heap, &args[0], fuel)
        }
        GraphIntrinsic::WeaklyConnectedComponents => {
            graph_conn::weakly_connected_components(heap, &args[0], fuel)
        }
        GraphIntrinsic::StronglyConnectedComponents => {
            graph_conn::strongly_connected_components(heap, &args[0], fuel)
        }
        GraphIntrinsic::IsConnected => graph_conn::is_connected(heap, &args[0]),
        GraphIntrinsic::IsWeaklyConnected => graph_conn::is_weakly_connected(heap, &args[0]),
        GraphIntrinsic::IsStronglyConnected => graph_conn::is_strongly_connected(heap, &args[0]),
        GraphIntrinsic::IsTree => graph_conn::is_tree(heap, &args[0]),
        GraphIntrinsic::MaximumFlow => {
            graph_flow::maximum_flow(heap, &args[0], &args[1], &args[2], &args[3], fuel)
        }
        GraphIntrinsic::MinimumCut => {
            graph_flow::minimum_cut(heap, &args[0], &args[1], &args[2], &args[3], fuel)
        }
        GraphIntrinsic::MinimumSpanningForest => {
            graph_flow::minimum_spanning_forest(heap, &args[0], &args[1], fuel)
        }
        GraphIntrinsic::TransitiveReduction => {
            graph_flow::transitive_reduction(heap, &args[0], fuel)
        }
        GraphIntrinsic::Dijkstra => {
            graph_paths::dijkstra(heap, &args[0], &args[1], &args[2], &args[3], fuel)
        }
        GraphIntrinsic::Astar => {
            graph_paths::astar(heap, &args[0], &args[1], &args[2], &args[3], &args[4], fuel)
        }
        GraphIntrinsic::BellmanFord => {
            graph_paths::bellman_ford(heap, &args[0], &args[1], &args[2], &args[3], fuel)
        }
        GraphIntrinsic::FloydWarshall => {
            graph_allpaths::floyd_warshall(heap, &args[0], &args[1], fuel)
        }
        GraphIntrinsic::SimplePaths => graph_allpaths::simple_paths(heap, &args[0], &args[1], fuel),
        GraphIntrinsic::AllShortestPaths => graph_allpaths::all_shortest_paths(
            heap, &args[0], &args[1], &args[2], &args[3], &args[4], fuel,
        ),
        GraphIntrinsic::Dfs => graph_walk::dfs_op(heap, &args[0], &args[1], fuel),
        GraphIntrinsic::Bfs => graph_walk::bfs_op(heap, &args[0], &args[1], fuel),
        GraphIntrinsic::DfsPostOrder => {
            graph_walk::dfs_post_order_op(heap, &args[0], &args[1], fuel)
        }
        GraphIntrinsic::Topo => graph_walk::topo_op(heap, &args[0], &args[1], fuel),
        GraphIntrinsic::Nodes => graph_walk::nodes_walker(heap, &args[0], fuel),
        GraphIntrinsic::Edges => graph_walk::edges_walker(heap, &args[0], fuel),
        GraphIntrinsic::Externals => graph_walk::externals_walker(heap, &args[0], &args[1], fuel),
        GraphIntrinsic::Indices => graph_walk::walker_indices(heap, &args[0], fuel),
        GraphIntrinsic::Values => graph_walk::walker_values(heap, &args[0], fuel),
        GraphIntrinsic::Entries => graph_walk::walker_entries(heap, &args[0], fuel),
        GraphIntrinsic::JsonDecode => graph_json::json_decode(heap, &args[0], &args[1], fuel),
        GraphIntrinsic::JsonEncode => graph_json::json_encode(heap, &args[0], &args[1], fuel),
    }
}

pub(super) fn new_graph(directed: bool) -> GraphData {
    GraphData {
        directed,
        nodes: Vec::new(),
        edges: Vec::new(),
        adj: BTreeMap::new(),
        radj: BTreeMap::new(),
        next_node: 0,
        next_edge: 0,
    }
}

pub(super) fn hydrate(mut g: GraphData) -> GraphData {
    g.adj.clear();
    g.radj.clear();
    for n in &g.nodes {
        g.adj.insert(n.index, Vec::new());
        g.radj.insert(n.index, Vec::new());
    }
    for e in &g.edges {
        if let Some(v) = g.adj.get_mut(&e.source) {
            v.push(e.index);
        }
        if let Some(v) = g.radj.get_mut(&e.target) {
            v.push(e.index);
        }
        if !g.directed {
            if let Some(v) = g.adj.get_mut(&e.target) {
                v.push(e.index);
            }
            if let Some(v) = g.radj.get_mut(&e.source) {
                v.push(e.index);
            }
        }
    }
    g.next_node = g.nodes.last().map_or(0, |n| n.index + 1);
    g.next_edge = g.edges.last().map_or(0, |e| e.index + 1);
    g
}

pub(super) fn alloc_graph(heap: &mut Heap, g: GraphData) -> Value {
    heap.allocate(Data::Graph(Box::new(g)))
}

pub(super) fn expect_graph<'a>(heap: &'a Heap, val: &Value) -> Result<&'a GraphData, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::PatternKind("Graph"));
    };
    let Data::Graph(g) = &heap.object(tok.id)?.data else {
        return Err(Error::PatternKind("Graph"));
    };
    Ok(g)
}

pub(super) fn expect_graph_id(heap: &Heap, val: &Value) -> Result<ObjectId, Error> {
    let Value::Object(tok) = val else {
        return Err(Error::PatternKind("Graph"));
    };
    let Data::Graph(_) = &heap.object(tok.id)?.data else {
        return Err(Error::PatternKind("Graph"));
    };
    Ok(tok.id)
}

pub(super) fn expect_int(val: &Value) -> Result<i32, Error> {
    let Value::Int(i) = *val else {
        return Err(Error::ExpectedInt);
    };
    Ok(i)
}

pub(super) fn graph_err(heap: &mut Heap, msg: &str) -> Result<Value, Error> {
    let s = heap.literal(&Lit::String(msg.into()))?;
    let e = heap.construct("Graph.Types.GraphError.GraphError", vec![s])?;
    heap.construct("Prelude.Result.Err", vec![e])
}

pub(super) fn missing_node_msg(node: i32) -> String {
    format!("Node {node} does not exist")
}

pub(super) fn graph_defect(msg: &str) -> Error {
    Error::RangeError(format!("GraphError: {msg}"))
}

pub(super) fn node_at(g: &GraphData, idx: i32) -> Option<usize> {
    g.nodes.binary_search_by_key(&idx, |n| n.index).ok()
}

pub(super) fn edge_at(g: &GraphData, idx: i32) -> Option<usize> {
    g.edges.binary_search_by_key(&idx, |e| e.index).ok()
}

pub(super) fn neighbor_of(g: &GraphData, node: i32, e: &GraphEdge) -> i32 {
    if !g.directed && e.target == node {
        e.source
    } else {
        e.target
    }
}

pub(super) fn rows(g: &GraphData, incoming: bool) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
    let adj = if incoming { &g.radj } else { &g.adj };
    let mut neighbors = vec![Vec::new(); g.nodes.len()];
    let mut edges = vec![Vec::new(); g.nodes.len()];
    for (p, n) in g.nodes.iter().enumerate() {
        if let Some(eis) = adj.get(&n.index) {
            for &ei in eis {
                if let Some(q) = edge_at(g, ei) {
                    let e = &g.edges[q];
                    let next = if (!g.directed && e.source != n.index) || (g.directed && incoming) {
                        e.source
                    } else {
                        e.target
                    };
                    if let Some(np) = node_at(g, next) {
                        neighbors[p].push(np);
                        edges[p].push(q);
                    }
                }
            }
        }
    }
    (neighbors, edges)
}

pub(super) fn adjacencies(
    g: &GraphData,
    direction: &str,
) -> (Vec<Vec<usize>>, Option<Vec<Vec<usize>>>) {
    if g.directed && direction == "incoming" {
        (rows(g, true).0, None)
    } else if g.directed && direction == "undirected" {
        (rows(g, false).0, Some(rows(g, true).0))
    } else {
        (rows(g, false).0, None)
    }
}

pub(super) fn endpoints(g: &GraphData) -> (Vec<usize>, Vec<usize>) {
    let mut sources = Vec::with_capacity(g.edges.len());
    let mut targets = Vec::with_capacity(g.edges.len());
    for e in &g.edges {
        sources.push(node_at(g, e.source).unwrap_or(0));
        targets.push(node_at(g, e.target).unwrap_or(0));
    }
    (sources, targets)
}

pub(super) fn index_list(heap: &mut Heap, xs: &[i32]) -> Result<Value, Error> {
    let items: Vec<Value> = xs.iter().copied().map(Value::Int).collect();
    heap.construct("$list", items)
}

pub(super) fn opt_inner<'a>(heap: &'a Heap, val: &'a Value) -> Result<Option<&'a Value>, Error> {
    if heap.tag(val)? == "Prelude.Option.Some" {
        Ok(Some(heap.field(val, 0)?))
    } else {
        Ok(None)
    }
}

pub(super) fn direction_of(heap: &Heap, val: &Value) -> Result<String, Error> {
    match opt_inner(heap, val)? {
        Some(v) => heap.string_lossy(v),
        None => Ok("outgoing".into()),
    }
}

pub(super) fn drop_partial(
    heap: &mut Heap,
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
) -> Result<(), Error> {
    let mut vals: Vec<Value> = nodes.into_iter().map(|n| n.data).collect();
    vals.extend(edges.into_iter().map(|e| e.data));
    heap.drop_values(vals)
}

pub(super) fn clone_graph_filtered(
    heap: &mut Heap,
    gid: ObjectId,
    keep_node: impl Fn(usize, i32) -> bool,
    keep_edge: impl Fn(usize, i32, i32, i32) -> bool,
) -> Result<GraphData, Error> {
    let (directed, next_node, next_edge, node_reps, edge_reps) = {
        let Data::Graph(g) = &heap.object(gid)?.data else {
            return Err(Error::PatternKind("Graph"));
        };
        let nr: Vec<(i32, Value)> = g
            .nodes
            .iter()
            .enumerate()
            .filter(|(p, n)| keep_node(*p, n.index))
            .map(|(_, n)| (n.index, n.data.representation()))
            .collect();
        let er: Vec<(i32, i32, i32, Value)> = g
            .edges
            .iter()
            .enumerate()
            .filter(|(q, e)| keep_edge(*q, e.index, e.source, e.target))
            .map(|(_, e)| (e.index, e.source, e.target, e.data.representation()))
            .collect();
        (g.directed, g.next_node, g.next_edge, nr, er)
    };
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
    Ok(out)
}

pub(super) fn build_node_walker(
    heap: &mut Heap,
    gid: ObjectId,
    positions: &[usize],
) -> Result<Value, Error> {
    let pairs: Vec<(i32, Value)> = {
        let Data::Graph(g) = &heap.object(gid)?.data else {
            return Err(Error::PatternKind("Graph"));
        };
        positions
            .iter()
            .map(|&p| (g.nodes[p].index, g.nodes[p].data.representation()))
            .collect()
    };
    let mut indices = Vec::with_capacity(pairs.len());
    let mut values = Vec::with_capacity(pairs.len());
    for (idx, rep) in pairs {
        match heap.dup(&rep) {
            Ok(v) => {
                indices.push(Value::Int(idx));
                values.push(v);
            }
            Err(err) => {
                heap.drop_values(values)?;
                return Err(err);
            }
        }
    }
    Ok(heap.allocate(Data::GraphWalker(GraphWalkerData { indices, values })))
}
