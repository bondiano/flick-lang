/// Pure `Path` primitives (`packages/std/src/Path.fl`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathIntrinsic {
    Basename,
    BasenameWithout,
    Dirname,
    Extname,
    Format,
    FromFileUrl,
    IsAbsolute,
    Join,
    Normalize,
    Parse,
    Relative,
    Resolve,
    ResolveWithin,
    ToFileUrl,
    ToNamespacedPath,
}

impl PathIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::BasenameWithout | Self::Relative | Self::ResolveWithin => 2,
            _ => 1,
        }
    }
}

/// Pure `DateTime` primitives (`packages/std/src/DateTime.fl`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DateTimeIntrinsic {
    Make,
    FromParts,
    FromEpochSeconds,
    MakeZoned,
    MakeZonedFromString,
    UnsafeFromEpochMillis,
    IsUtc,
    IsZoned,
    ToUtc,
    SetZone,
    SetZoneOffset,
    SetZoneNamed,
    ZoneMakeOffset,
    ZoneMakeNamed,
    ZoneMakeLocal,
    ZoneFromString,
    ZoneToString,
    ToEpochMillis,
    ToEpochSeconds,
    WithEpochMillis,
    ToParts,
    ToPartsUtc,
    SetParts,
    SetPartsUtc,
    RemoveTime,
    ZonedOffset,
    ZonedOffsetIso,
    Add,
    Subtract,
    AddDuration,
    SubtractDuration,
    StartOf,
    EndOf,
    Nearest,
    Distance,
    Compare,
    Format,
    FormatLocal,
    FormatUtc,
    FormatIntl,
    FormatIso,
    FormatIsoDate,
    FormatIsoDateUtc,
    FormatIsoOffset,
    FormatIsoZoned,
}

impl DateTimeIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::MakeZoned
            | Self::SetZone
            | Self::SetZoneOffset
            | Self::SetZoneNamed
            | Self::Add
            | Self::Subtract
            | Self::StartOf
            | Self::EndOf
            | Self::Nearest => 3,
            Self::WithEpochMillis
            | Self::SetParts
            | Self::SetPartsUtc
            | Self::AddDuration
            | Self::SubtractDuration
            | Self::Distance
            | Self::Compare
            | Self::Format
            | Self::FormatLocal
            | Self::FormatUtc
            | Self::FormatIntl => 2,
            _ => 1,
        }
    }
}

/// Pure `Cron` primitives (`packages/std/src/Cron.fl`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CronIntrinsic {
    Make,
    Parse,
    Format,
    Match,
    Next,
    Prev,
    Sequence,
    Equals,
}

impl CronIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::Make => 1,
            Self::Sequence => 3,
            _ => 2,
        }
    }
}

/// Pure `Graph` primitives (`packages/std/src/Graph/*.fl`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphIntrinsic {
    Make,
    Directed,
    Undirected,
    FromSnapshot,
    ToSnapshot,
    AddNode,
    AddEdge,
    RemoveNode,
    RemoveNodes,
    RemoveEdge,
    RemoveEdges,
    UpdateNodeData,
    UpdateEdgeData,
    ReplaceNodes,
    ReplaceEdges,
    FilterMapNodesApply,
    FilterMapEdgesApply,
    Reverse,
    Sum,
    Combine,
    ComplementPairs,
    ComplementBuild,
    Neighborhood,
    InducedSubgraph,
    GetNode,
    HasNode,
    NodeCount,
    GetEdge,
    HasEdge,
    EdgeCount,
    IncidentEdges,
    OutgoingEdges,
    IncomingEdges,
    EdgesBetween,
    Degree,
    OutDegree,
    InDegree,
    Neighbors,
    Successors,
    Predecessors,
    NeighborsDirected,
    LabelText,
    ToGraphVizRender,
    ToMermaidRender,
    FindCycle,
    IsAcyclic,
    IsBipartite,
    MaximumBipartiteMatching,
    UnweightedDistances,
    HasPath,
    ConnectedComponents,
    Bridges,
    ArticulationPoints,
    BiconnectedComponents,
    WeaklyConnectedComponents,
    StronglyConnectedComponents,
    IsConnected,
    IsWeaklyConnected,
    IsStronglyConnected,
    IsTree,
    MaximumFlow,
    MinimumCut,
    MinimumSpanningForest,
    TransitiveReduction,
    Dijkstra,
    Astar,
    BellmanFord,
    FloydWarshall,
    SimplePaths,
    AllShortestPaths,
    Dfs,
    Bfs,
    DfsPostOrder,
    Topo,
    Nodes,
    Edges,
    Externals,
    Indices,
    Values,
    Entries,
    JsonDecode,
    JsonEncode,
}

impl GraphIntrinsic {
    pub fn arity(self) -> usize {
        match self {
            Self::AddEdge
            | Self::HasPath
            | Self::ToGraphVizRender
            | Self::MaximumFlow
            | Self::MinimumCut
            | Self::Dijkstra
            | Self::BellmanFord => 4,
            Self::Astar | Self::AllShortestPaths => 5,
            Self::UpdateNodeData
            | Self::UpdateEdgeData
            | Self::Combine
            | Self::Neighborhood
            | Self::HasEdge
            | Self::EdgesBetween
            | Self::NeighborsDirected
            | Self::ToMermaidRender
            | Self::UnweightedDistances => 3,
            Self::AddNode
            | Self::RemoveNode
            | Self::RemoveNodes
            | Self::RemoveEdge
            | Self::RemoveEdges
            | Self::ReplaceNodes
            | Self::ReplaceEdges
            | Self::FilterMapNodesApply
            | Self::FilterMapEdgesApply
            | Self::Sum
            | Self::ComplementBuild
            | Self::InducedSubgraph
            | Self::GetNode
            | Self::HasNode
            | Self::GetEdge
            | Self::IncidentEdges
            | Self::OutgoingEdges
            | Self::IncomingEdges
            | Self::Degree
            | Self::OutDegree
            | Self::InDegree
            | Self::Neighbors
            | Self::Successors
            | Self::Predecessors
            | Self::MinimumSpanningForest
            | Self::FloydWarshall
            | Self::SimplePaths
            | Self::Dfs
            | Self::Bfs
            | Self::DfsPostOrder
            | Self::Topo
            | Self::Externals
            | Self::JsonDecode
            | Self::JsonEncode => 2,
            _ => 1,
        }
    }
}
