//! Audited `Stream.Combine` split/broadcast/share/partition/groupBy IR adapters (Stage 4E).

mod broadcast;
mod group;
mod partition;
mod share;

use super::expr_builder::ExprBuilder;
use super::std_schema_rt::rt;
use super::{Expr, Span};

pub(super) fn stream_split_pure_template(
    b: &ExprBuilder,
    bind: &str,
    stages: usize,
) -> Option<Expr> {
    match (bind, stages) {
        ("groupBy", 2) => Some(b.app(rt(b, "Helper.st_group_by"), b.bool_(false))),
        ("groupByKey", 2) => Some(b.app(rt(b, "Helper.st_group_by"), b.bool_(true))),
        _ => None,
    }
}

pub(super) fn stream_split_effect_template(
    b: &ExprBuilder,
    bind: &str,
    stages: usize,
) -> Option<Expr> {
    match (bind, stages) {
        ("flickBroadcast", 2) => Some(rt(b, "Helper.st_broadcast")),
        ("flickBroadcastN", 3) => Some(rt(b, "Helper.st_broadcast_n")),
        ("flickShare", 3) => Some(rt(b, "Helper.st_share")),
        ("partition", 2) => Some(b.app(rt(b, "Helper.st_partition"), b.bool_(false))),
        ("partitionEffect", 2) => Some(b.app(rt(b, "Helper.st_partition"), b.bool_(true))),
        ("partitionQueue", 2) => Some(rt(b, "Helper.st_partition_queue")),
        _ => None,
    }
}

pub(super) fn stream_split_helper(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$stsp");
    match name {
        "st_take_stream" => Some(broadcast::st_take_stream_helper(&b)),
        "st_to_pubsub_take" => Some(broadcast::st_to_pubsub_take_helper(&b)),
        "st_broadcast" => Some(broadcast::st_broadcast_helper(&b)),
        "st_broadcast_n" => Some(broadcast::st_broadcast_n_helper(&b)),
        "st_share" => Some(share::st_share_helper(&b)),
        "st_queue_shutdown_stream" => Some(partition::st_queue_shutdown_stream_helper(&b)),
        "st_partition_queue" => Some(partition::st_partition_queue_helper(&b)),
        "st_partition" => Some(partition::st_partition_helper(&b)),
        "st_group_by" => Some(group::st_group_by_helper(&b)),
        _ => None,
    }
}
