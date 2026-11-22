//! Synthetic `$rt` global resolution for `Path`, `DateTime`, `Graph`, `Config`, `Arbitrary`, `Layer`, and `Prelude`.

use flick_ir::memory::{
    ArbitraryIntrinsic, AsyncIntrinsic, BigDecimalIntrinsic, BigIntIntrinsic, BytesIntrinsic,
    ConfigIntrinsic, DateTimeIntrinsic, DurationIntrinsic, GraphIntrinsic, HostPrimitive,
    Intrinsic, LayerIntrinsic, PathIntrinsic, StringIntrinsic,
};

pub(super) fn ext_rt_intrinsic(name: &str) -> Option<Intrinsic> {
    if name == "Show" {
        return Some(Intrinsic::Show);
    }
    if name == "String.Length" {
        return Some(Intrinsic::StringLength);
    }
    if name == "String.CharCodeAt" {
        return Some(Intrinsic::String(StringIntrinsic::CharCodeAt));
    }
    if name == "String.Slice" {
        return Some(Intrinsic::String(StringIntrinsic::Slice));
    }
    if name == "String.ReplaceAll" {
        return Some(Intrinsic::String(StringIntrinsic::ReplaceAll));
    }
    if name == "Bytes.EncodeUtf8" {
        return Some(Intrinsic::Bytes(BytesIntrinsic::EncodeUtf8));
    }
    if name == "Bytes.ToList" {
        return Some(Intrinsic::Bytes(BytesIntrinsic::ToList));
    }
    if name == "Layer.Succeed" {
        return Some(Intrinsic::LayerSucceed);
    }
    if name == "Prelude.FromResult" {
        return Some(Intrinsic::FromResult);
    }
    if name == "Host.RandomNext" {
        return Some(Intrinsic::Host(HostPrimitive::RandomNext));
    }
    if name == "Host.CurrentTimeMillis" {
        return Some(Intrinsic::Host(HostPrimitive::CurrentTimeMillis));
    }
    if name == "Host.CurrentTimeNanos" {
        return Some(Intrinsic::Host(HostPrimitive::CurrentTimeNanos));
    }
    if name == "Host.MonotonicTimeNanos" {
        return Some(Intrinsic::Host(HostPrimitive::MonotonicTimeNanos));
    }
    if name == "BigInt.FromString" {
        return Some(Intrinsic::BigInt(BigIntIntrinsic::FromString));
    }
    if name == "BigDecimal.FromInteger" {
        return Some(Intrinsic::BigDecimal(BigDecimalIntrinsic::FromInteger));
    }
    if name == "BigDecimal.Scale" {
        return Some(Intrinsic::BigDecimal(BigDecimalIntrinsic::Scale));
    }
    if name == "Bytes.Utf8Encode" {
        return Some(Intrinsic::Bytes(BytesIntrinsic::LegacyUtf8Encode));
    }
    if name == "Bytes.Utf8Decode" {
        return Some(Intrinsic::Bytes(BytesIntrinsic::LegacyUtf8Decode));
    }
    if let Some(op) = name.strip_prefix("Layer.") {
        return layer_intrinsic_by_name(op).map(Intrinsic::Layer);
    }
    if let Some(op) = name.strip_prefix("Duration.") {
        return duration_intrinsic_by_name(op).map(Intrinsic::Duration);
    }
    if let Some(op) = name.strip_prefix("Async.") {
        return async_intrinsic_by_name(op).map(Intrinsic::Async);
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
    if let Some(op) = name.strip_prefix("Config.") {
        return config_intrinsic_by_name(op).map(Intrinsic::Config);
    }
    if let Some(op) = name.strip_prefix("Arbitrary.") {
        return arbitrary_intrinsic_by_name(op).map(Intrinsic::Arbitrary);
    }
    None
}

fn layer_intrinsic_by_name(name: &str) -> Option<LayerIntrinsic> {
    use LayerIntrinsic as L;
    Some(match name {
        "OwnerNew" => L::OwnerNew,
        "OwnerRun" => L::OwnerRun,
        "OwnerClose" => L::OwnerClose,
        "EnvCapture" => L::EnvCapture,
        "EnvRun" => L::EnvRun,
        "ProvideContext" => L::LayerProvideContext,
        "LookupService" => L::LayerLookupService,
        "Build" => L::LayerBuild,
        "ContextMake" => L::ContextMake,
        "ContextReference" => L::ContextReference,
        "ContextRead" => L::ContextRead,
        "ContextWithReference" => L::ContextWithReference,
        _ => return None,
    })
}

fn duration_intrinsic_by_name(name: &str) -> Option<DurationIntrinsic> {
    use DurationIntrinsic as D;
    Some(match name {
        "Millis" => D::Millis,
        "ToMillis" => D::ToMillis,
        "IsPositive" => D::IsPositive,
        _ => return None,
    })
}

fn async_intrinsic_by_name(name: &str) -> Option<AsyncIntrinsic> {
    use AsyncIntrinsic as A;
    Some(match name {
        "RaceFirst" => A::RaceFirst,
        "TimeoutOrElse" => A::TimeoutOrElse,
        "EffectYieldNow" => A::EffectYieldNow,
        "EffectSleep" => A::EffectSleep,
        "EffectNever" => A::EffectNever,
        _ => return None,
    })
}

fn config_intrinsic_by_name(name: &str) -> Option<ConfigIntrinsic> {
    use ConfigIntrinsic as C;
    Some(match name {
        "ConfigCase" => C::ConfigCase,
        "ParseDotEnv" => C::ParseDotEnv,
        "ParseUrl" => C::ParseUrl,
        "ParseDate" => C::ParseDate,
        "DecodeByteSize" => C::DecodeByteSize,
        "DecodeNumber" => C::DecodeNumber,
        "SegmentInfo" => C::SegmentInfo,
        "UnknownNode" => C::UnknownNode,
        "EnvNode" => C::EnvNode,
        _ => return None,
    })
}

fn arbitrary_intrinsic_by_name(name: &str) -> Option<ArbitraryIntrinsic> {
    use ArbitraryIntrinsic as A;
    Some(match name {
        "HashSeed" => A::HashSeed,
        "AttemptRandom" => A::AttemptRandom,
        "XoshiroNext" => A::XoshiroNext,
        "XoshiroDouble" => A::XoshiroDouble,
        "RandomInt" => A::RandomInt,
        "RandomNumber" => A::RandomNumber,
        "RandomBigIntBiased" => A::RandomBigIntBiased,
        "RandomDecimal" => A::RandomDecimal,
        "RandomString" => A::RandomString,
        "RandomCodePointString" => A::RandomCodePointString,
        "ShrinkNumber" => A::ShrinkNumber,
        "ShrinkBigInt" => A::ShrinkBigInt,
        "ShrinkDecimalCands" => A::ShrinkDecimalCands,
        "DecimalMidpoint" => A::DecimalMidpoint,
        "ShrinkString" => A::ShrinkString,
        "NumberBounds" => A::NumberBounds,
        "ExtractConstraint" => A::ExtractConstraint,
        "MergeConstraint" => A::MergeConstraint,
        "FormatSpaced" => A::FormatSpaced,
        "MakeDuration" => A::MakeDuration,
        "IsNonNegativeDuration" => A::IsNonNegativeDuration,
        "MakeZonedDateTime" => A::MakeZonedDateTime,
        "MakeZone" => A::MakeZone,
        "JsString" => A::JsString,
        "QuoteJson" => A::QuoteJson,
        _ => return None,
    })
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
        "UnsafeFromEpochMillis" => D::UnsafeFromEpochMillis,
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
