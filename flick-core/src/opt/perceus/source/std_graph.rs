//! Audited `Graph` foreign adapters (`packages/std/src/Graph/*.fl`).

use flick_ir::memory::GraphIntrinsic;
use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::std_graph_ops::{
    combine_template, flow_template, graph_json_template, graphviz_template, mermaid_template,
};
use super::std_schema_rt::{pair_pat, rt};
use super::{Code, Env, Expr, Intrinsic, Local, Lower, LowerError, Pat, Span, Stmt};

impl Lower<'_> {
    pub(super) fn std_graph_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || !has_standard_shape(value) || value.mode != ForeignMode::Pure {
            return None;
        }
        let from = value.binding.from()?;
        if from != "flick:Graph" {
            return None;
        }
        let bind = value.binding.export_name().unwrap_or("");
        let stages = value.stages.len();
        if let Some(op) = direct_graph_intrinsic(bind, stages) {
            return Some(self.intrinsic(Intrinsic::Graph(op), false, vec![], block, span));
        }
        let b = ExprBuilder::new(span, "$gr");
        let expr = graph_template(&b, bind, stages)?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

fn direct_graph_intrinsic(bind: &str, stages: usize) -> Option<GraphIntrinsic> {
    use GraphIntrinsic as G;
    let op = match bind {
        "flickMake" => G::Make,
        "directed" => G::Directed,
        "undirected" => G::Undirected,
        "flickFromSnapshot" => G::FromSnapshot,
        "flickToSnapshot" => G::ToSnapshot,
        "flickAddNode" => G::AddNode,
        "flickAddEdge" => G::AddEdge,
        "flickRemoveNode" => G::RemoveNode,
        "flickRemoveNodes" => G::RemoveNodes,
        "flickRemoveEdge" => G::RemoveEdge,
        "flickRemoveEdges" => G::RemoveEdges,
        "flickReverse" => G::Reverse,
        "sum" => G::Sum,
        "flickNeighborhood" => G::Neighborhood,
        "flickInducedSubgraph" => G::InducedSubgraph,
        "getNode" => G::GetNode,
        "hasNode" => G::HasNode,
        "nodeCount" => G::NodeCount,
        "getEdge" => G::GetEdge,
        "hasEdge" => G::HasEdge,
        "edgeCount" => G::EdgeCount,
        "flickIncidentEdges" => G::IncidentEdges,
        "flickOutgoingEdges" => G::OutgoingEdges,
        "flickIncomingEdges" => G::IncomingEdges,
        "flickEdgesBetween" => G::EdgesBetween,
        "flickDegree" => G::Degree,
        "flickOutDegree" => G::OutDegree,
        "flickInDegree" => G::InDegree,
        "neighbors" => G::Neighbors,
        "successors" => G::Successors,
        "predecessors" => G::Predecessors,
        "neighborsDirected" => G::NeighborsDirected,
        "findCycle" => G::FindCycle,
        "isAcyclic" => G::IsAcyclic,
        "isBipartite" => G::IsBipartite,
        "flickMaximumBipartiteMatching" => G::MaximumBipartiteMatching,
        "flickUnweightedDistances" => G::UnweightedDistances,
        "flickHasPath" => G::HasPath,
        "connectedComponents" => G::ConnectedComponents,
        "bridges" => G::Bridges,
        "articulationPoints" => G::ArticulationPoints,
        "biconnectedComponents" => G::BiconnectedComponents,
        "weaklyConnectedComponents" => G::WeaklyConnectedComponents,
        "stronglyConnectedComponents" => G::StronglyConnectedComponents,
        "isConnected" => G::IsConnected,
        "isWeaklyConnected" => G::IsWeaklyConnected,
        "isStronglyConnected" => G::IsStronglyConnected,
        "isTree" => G::IsTree,
        "flickTransitiveReduction" => G::TransitiveReduction,
        "flickSimplePaths" => G::SimplePaths,
        "flickDfs" => G::Dfs,
        "flickBfs" => G::Bfs,
        "flickDfsPostOrder" => G::DfsPostOrder,
        "flickTopo" => G::Topo,
        "nodes" => G::Nodes,
        "edges" => G::Edges,
        "flickExternals" => G::Externals,
        "flickIndices" => G::Indices,
        "flickValues" => G::Values,
        "flickEntries" => G::Entries,
        _ => return None,
    };
    (op.arity() == stages).then_some(op)
}

pub(super) fn map_strict(b: &ExprBuilder, item: &str, src: Expr, body: Expr) -> Expr {
    b.gen_strict(b.each(item, src, b.yield_(body)))
}

pub(super) fn snap_nodes(b: &ExprBuilder, g: Expr) -> Expr {
    b.field(b.app(rt(b, "Graph.ToSnapshot"), g), "nodes")
}

pub(super) fn snap_edges(b: &ExprBuilder, g: Expr) -> Expr {
    b.field(b.app(rt(b, "Graph.ToSnapshot"), g), "edges")
}

#[expect(
    clippy::too_many_lines,
    reason = "central Graph callback & codec templates"
)]
fn graph_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickUpdateNode", 3) => Some(b.lam(
            &["i", "f", "g"],
            b.match_(
                b.app2(rt(b, "Graph.GetNode"), b.loc("i"), b.loc("g")),
                vec![
                    arm(
                        b.some_pat("d"),
                        b.app3(
                            rt(b, "Graph.UpdateNodeData"),
                            b.loc("i"),
                            b.app(b.loc("f"), b.loc("d")),
                            b.loc("g"),
                        ),
                    ),
                    arm(Pat::Wild, b.loc("g")),
                ],
            ),
        )),
        ("flickUpdateEdge", 3) => Some(b.lam(
            &["i", "f", "g"],
            b.match_(
                b.app2(rt(b, "Graph.GetEdge"), b.loc("i"), b.loc("g")),
                vec![
                    arm(
                        b.some_pat("e"),
                        b.app3(
                            rt(b, "Graph.UpdateEdgeData"),
                            b.loc("i"),
                            b.app(b.loc("f"), b.field(b.loc("e"), "data")),
                            b.loc("g"),
                        ),
                    ),
                    arm(Pat::Wild, b.loc("g")),
                ],
            ),
        )),
        ("flickMapNodes", 2) => Some(b.lam(
            &["f", "g"],
            b.app2(
                rt(b, "Graph.ReplaceNodes"),
                map_strict(
                    b,
                    "n",
                    snap_nodes(b, b.loc("g")),
                    b.app(b.loc("f"), b.field(b.loc("n"), "data")),
                ),
                b.loc("g"),
            ),
        )),
        ("flickMapEdges", 2) => Some(b.lam(
            &["f", "g"],
            b.app2(
                rt(b, "Graph.ReplaceEdges"),
                map_strict(
                    b,
                    "e",
                    snap_edges(b, b.loc("g")),
                    b.app(b.loc("f"), b.field(b.loc("e"), "data")),
                ),
                b.loc("g"),
            ),
        )),
        ("flickFilterMapNodes", 2) => Some(b.lam(
            &["f", "g"],
            b.app2(
                rt(b, "Graph.FilterMapNodesApply"),
                map_strict(
                    b,
                    "n",
                    snap_nodes(b, b.loc("g")),
                    b.app(b.loc("f"), b.field(b.loc("n"), "data")),
                ),
                b.loc("g"),
            ),
        )),
        ("flickFilterMapEdges", 2) => Some(b.lam(
            &["f", "g"],
            b.app2(
                rt(b, "Graph.FilterMapEdgesApply"),
                map_strict(
                    b,
                    "e",
                    snap_edges(b, b.loc("g")),
                    b.app(b.loc("f"), b.field(b.loc("e"), "data")),
                ),
                b.loc("g"),
            ),
        )),
        ("flickFilterNodes", 2) => Some(b.lam(
            &["f", "g"],
            b.app2(
                rt(b, "Graph.FilterMapNodesApply"),
                map_strict(
                    b,
                    "n",
                    snap_nodes(b, b.loc("g")),
                    b.if_(
                        b.app(b.loc("f"), b.field(b.loc("n"), "data")),
                        b.some(b.field(b.loc("n"), "data")),
                        b.none(),
                    ),
                ),
                b.loc("g"),
            ),
        )),
        ("flickFilterEdges", 2) => Some(b.lam(
            &["f", "g"],
            b.app2(
                rt(b, "Graph.FilterMapEdgesApply"),
                map_strict(
                    b,
                    "e",
                    snap_edges(b, b.loc("g")),
                    b.if_(
                        b.app(b.loc("f"), b.field(b.loc("e"), "data")),
                        b.some(b.field(b.loc("e"), "data")),
                        b.none(),
                    ),
                ),
                b.loc("g"),
            ),
        )),
        ("findNode", 2) => Some(b.lam(
            &["f", "g"],
            b.block(
                vec![
                    b.var_decl("found", b.none()),
                    Stmt::Expr(b.for_ctrl(
                        "$fn",
                        "n",
                        snap_nodes(b, b.loc("g")),
                        b.if_(
                            b.app(b.loc("f"), b.field(b.loc("n"), "data")),
                            b.block(
                                vec![b.assign("found", b.some(b.field(b.loc("n"), "index")))],
                                b.break_("$fn", b.unit()),
                            ),
                            b.unit(),
                        ),
                    )),
                ],
                b.var("found"),
            ),
        )),
        ("findNodes", 2) => Some(b.lam(
            &["f", "g"],
            b.gen_strict(b.each(
                "n",
                snap_nodes(b, b.loc("g")),
                b.if_(
                    b.app(b.loc("f"), b.field(b.loc("n"), "data")),
                    b.yield_(b.field(b.loc("n"), "index")),
                    b.unit(),
                ),
            )),
        )),
        ("findEdge", 2) => Some(b.lam(
            &["f", "g"],
            b.block(
                vec![
                    b.var_decl("found", b.none()),
                    Stmt::Expr(b.for_ctrl(
                        "$fe",
                        "e",
                        snap_edges(b, b.loc("g")),
                        b.if_(
                            b.app3(
                                b.loc("f"),
                                b.field(b.loc("e"), "data"),
                                b.field(b.loc("e"), "source"),
                                b.field(b.loc("e"), "target"),
                            ),
                            b.block(
                                vec![b.assign("found", b.some(b.field(b.loc("e"), "index")))],
                                b.break_("$fe", b.unit()),
                            ),
                            b.unit(),
                        ),
                    )),
                ],
                b.var("found"),
            ),
        )),
        ("findEdges", 2) => Some(b.lam(
            &["f", "g"],
            b.gen_strict(b.each(
                "e",
                snap_edges(b, b.loc("g")),
                b.if_(
                    b.app3(
                        b.loc("f"),
                        b.field(b.loc("e"), "data"),
                        b.field(b.loc("e"), "source"),
                        b.field(b.loc("e"), "target"),
                    ),
                    b.yield_(b.field(b.loc("e"), "index")),
                    b.unit(),
                ),
            )),
        )),
        ("flickCompose", 3) => Some(combine_template(b, "compose")),
        ("flickIntersection", 3) => Some(combine_template(b, "intersection")),
        ("flickDifference", 3) => Some(combine_template(b, "difference")),
        ("flickSymmetricDifference", 3) => Some(combine_template(b, "symmetric_difference")),
        ("complement", 2) => Some(b.lam(
            &["f", "g"],
            b.app2(
                rt(b, "Graph.ComplementBuild"),
                map_strict(
                    b,
                    "p",
                    b.app(rt(b, "Graph.ComplementPairs"), b.loc("g")),
                    b.match_(
                        b.loc("p"),
                        vec![arm(
                            pair_pat(b, "a", "c"),
                            b.app2(b.loc("f"), b.loc("a"), b.loc("c")),
                        )],
                    ),
                ),
                b.loc("g"),
            ),
        )),
        ("flickToGraphViz", 2) => Some(graphviz_template(b)),
        ("flickToMermaid", 2) => Some(mermaid_template(b)),
        ("flickDijkstra", 2) => Some(b.lam(
            &["cfg", "g"],
            b.block(
                vec![b.let_(
                    "ws",
                    map_strict(
                        b,
                        "e",
                        snap_edges(b, b.loc("g")),
                        b.app(b.field(b.loc("cfg"), "cost"), b.field(b.loc("e"), "data")),
                    ),
                )],
                b.app(
                    b.app3(
                        rt(b, "Graph.Dijkstra"),
                        b.field(b.loc("cfg"), "source"),
                        b.field(b.loc("cfg"), "target"),
                        b.loc("ws"),
                    ),
                    b.loc("g"),
                ),
            ),
        )),
        ("flickAstar", 2) => Some(b.lam(
            &["cfg", "g"],
            b.block(
                vec![
                    b.let_("snap", b.app(rt(b, "Graph.ToSnapshot"), b.loc("g"))),
                    b.let_(
                        "ws",
                        map_strict(
                            b,
                            "e",
                            b.field(b.loc("snap"), "edges"),
                            b.app(b.field(b.loc("cfg"), "cost"), b.field(b.loc("e"), "data")),
                        ),
                    ),
                    b.let_(
                        "hs",
                        b.match_(
                            b.app2(
                                rt(b, "Graph.GetNode"),
                                b.field(b.loc("cfg"), "target"),
                                b.loc("g"),
                            ),
                            vec![
                                arm(
                                    b.some_pat("td"),
                                    map_strict(
                                        b,
                                        "n",
                                        b.field(b.loc("snap"), "nodes"),
                                        b.app2(
                                            b.field(b.loc("cfg"), "heuristic"),
                                            b.field(b.loc("n"), "data"),
                                            b.loc("td"),
                                        ),
                                    ),
                                ),
                                arm(Pat::Wild, b.list(vec![])),
                            ],
                        ),
                    ),
                ],
                b.app2(
                    b.app3(
                        rt(b, "Graph.Astar"),
                        b.field(b.loc("cfg"), "source"),
                        b.field(b.loc("cfg"), "target"),
                        b.loc("ws"),
                    ),
                    b.loc("hs"),
                    b.loc("g"),
                ),
            ),
        )),
        ("flickBellmanFord", 2) => Some(b.lam(
            &["cfg", "g"],
            b.block(
                vec![b.let_(
                    "ws",
                    map_strict(
                        b,
                        "e",
                        snap_edges(b, b.loc("g")),
                        b.app(b.field(b.loc("cfg"), "cost"), b.field(b.loc("e"), "data")),
                    ),
                )],
                b.app(
                    b.app3(
                        rt(b, "Graph.BellmanFord"),
                        b.field(b.loc("cfg"), "source"),
                        b.field(b.loc("cfg"), "target"),
                        b.loc("ws"),
                    ),
                    b.loc("g"),
                ),
            ),
        )),
        ("flickFloydWarshall", 2) => Some(b.lam(
            &["cost", "g"],
            b.app2(
                rt(b, "Graph.FloydWarshall"),
                map_strict(
                    b,
                    "e",
                    snap_edges(b, b.loc("g")),
                    b.app(b.loc("cost"), b.field(b.loc("e"), "data")),
                ),
                b.loc("g"),
            ),
        )),
        ("flickAllShortestPaths", 2) => Some(b.lam(
            &["cfg", "g"],
            b.block(
                vec![b.let_(
                    "ws",
                    map_strict(
                        b,
                        "e",
                        snap_edges(b, b.loc("g")),
                        b.app(b.field(b.loc("cfg"), "cost"), b.field(b.loc("e"), "data")),
                    ),
                )],
                b.app2(
                    b.app3(
                        rt(b, "Graph.AllShortestPaths"),
                        b.field(b.loc("cfg"), "source"),
                        b.field(b.loc("cfg"), "target"),
                        b.field(b.loc("cfg"), "limit"),
                    ),
                    b.loc("ws"),
                    b.loc("g"),
                ),
            ),
        )),
        ("flickMaximumFlow", 2) => Some(flow_template(b, "Graph.MaximumFlow")),
        ("flickMinimumCut", 2) => Some(flow_template(b, "Graph.MinimumCut")),
        ("flickMinimumSpanningForest", 2) => Some(b.lam(
            &["cost", "g"],
            b.app2(
                rt(b, "Graph.MinimumSpanningForest"),
                map_strict(
                    b,
                    "e",
                    snap_edges(b, b.loc("g")),
                    b.app(b.loc("cost"), b.field(b.loc("e"), "data")),
                ),
                b.loc("g"),
            ),
        )),
        ("flickJson", 3) => Some(graph_json_template(b)),
        _ => None,
    }
}
