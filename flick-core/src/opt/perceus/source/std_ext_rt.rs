//! Synthetic `$rt` global resolution for `Path`, `DateTime`, `Graph`, `Layer`, and `Prelude`.

use flick_ir::memory::{DateTimeIntrinsic, GraphIntrinsic, Intrinsic, PathIntrinsic};

pub(super) fn ext_rt_intrinsic(name: &str) -> Option<Intrinsic> {
    if name == "Layer.Succeed" {
        return Some(Intrinsic::LayerSucceed);
    }
    if name == "Prelude.FromResult" {
        return Some(Intrinsic::FromResult);
    }
    if let Some(op) = name.strip_prefix("Path.") {
        return path_intrinsic_by_name(op).map(Intrinsic::Path);
    }
    if let Some(op) = name.strip_prefix("DateTime.") {
        return datetime_intrinsic_by_name(op).map(Intrinsic::DateTime);
    }
    if let Some(op) = name.strip_prefix("Graph.") {
        return graph_intrinsic_by_name(op).map(Intrinsic::Graph);
    }
    None
}

fn path_intrinsic_by_name(name: &str) -> Option<PathIntrinsic> {
    use PathIntrinsic as P;
    Some(match name {
        "Basename" => P::Basename,
        "BasenameWithout" => P::BasenameWithout,
        "Dirname" => P::Dirname,
        "Extname" => P::Extname,
        "Format" => P::Format,
        "FromFileUrl" => P::FromFileUrl,
        "IsAbsolute" => P::IsAbsolute,
        "Join" => P::Join,
        "Normalize" => P::Normalize,
        "Parse" => P::Parse,
        "Relative" => P::Relative,
        "Resolve" => P::Resolve,
        "ResolveWithin" => P::ResolveWithin,
        "ToFileUrl" => P::ToFileUrl,
        "ToNamespacedPath" => P::ToNamespacedPath,
        _ => return None,
    })
}

fn datetime_intrinsic_by_name(name: &str) -> Option<DateTimeIntrinsic> {
    use DateTimeIntrinsic as D;
    Some(match name {
        "IsUtc" => D::IsUtc,
        "ToEpochMillis" => D::ToEpochMillis,
        "WithEpochMillis" => D::WithEpochMillis,
        "Compare" => D::Compare,
        _ => return None,
    })
}

fn graph_intrinsic_by_name(name: &str) -> Option<GraphIntrinsic> {
    use GraphIntrinsic as G;
    Some(match name {
        "ToSnapshot" => G::ToSnapshot,
        "GetNode" => G::GetNode,
        "GetEdge" => G::GetEdge,
        "UpdateNodeData" => G::UpdateNodeData,
        "UpdateEdgeData" => G::UpdateEdgeData,
        "ReplaceNodes" => G::ReplaceNodes,
        "ReplaceEdges" => G::ReplaceEdges,
        "FilterMapNodesApply" => G::FilterMapNodesApply,
        "FilterMapEdgesApply" => G::FilterMapEdgesApply,
        "Combine" => G::Combine,
        "ComplementPairs" => G::ComplementPairs,
        "ComplementBuild" => G::ComplementBuild,
        "LabelText" => G::LabelText,
        "ToGraphVizRender" => G::ToGraphVizRender,
        "ToMermaidRender" => G::ToMermaidRender,
        "MaximumFlow" => G::MaximumFlow,
        "MinimumCut" => G::MinimumCut,
        "MinimumSpanningForest" => G::MinimumSpanningForest,
        "Dijkstra" => G::Dijkstra,
        "Astar" => G::Astar,
        "BellmanFord" => G::BellmanFord,
        "FloydWarshall" => G::FloydWarshall,
        "AllShortestPaths" => G::AllShortestPaths,
        "JsonDecode" => G::JsonDecode,
        "JsonEncode" => G::JsonEncode,
        _ => return None,
    })
}
