//! Audited `Stream.Transform` chunk/window/accum/pace pure IR adapters (Stage 4E).

mod accum;
mod changes;
mod chunk;
mod debounce;
mod group;
mod sliding;
mod throttle;

use flick_ir::BinOp;

use super::expr_builder::ExprBuilder;
use super::std_schema_rt::rt;
use super::{Expr, Span};

pub(super) fn stream_window_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("chunks", 1) => Some(b.app(
            rt(b, "Helper.ch_map"),
            b.lam(&["xs"], b.list([b.loc("xs")])),
        )),
        ("rechunk", 2) => Some(rt(b, "Helper.st_rechunk")),
        ("sliding", 2) => Some(b.lam(
            &["sz", "self"],
            b.app3(
                rt(b, "Helper.st_sliding_size"),
                b.loc("sz"),
                b.int(1),
                b.loc("self"),
            ),
        )),
        ("slidingSize", 3) => Some(rt(b, "Helper.st_sliding_size")),
        ("split", 2) => Some(rt(b, "Helper.st_split")),
        ("grouped", 2) => {
            let rechunked = b.app2(rt(b, "Helper.st_rechunk"), b.loc("n"), b.loc("self"));
            let body = b.app2(
                rt(b, "Helper.ch_map"),
                b.lam(&["xs"], b.list([b.loc("xs")])),
                rechunked,
            );
            Some(b.lam(&["n", "self"], body))
        }
        ("groupedWithin", 3) => {
            let count = b.if_(b.bin(BinOp::Le, b.loc("n"), b.int(0)), b.int(1), b.loc("n"));
            let sink = b.app(rt(b, "Helper.sk_take"), count);
            let sched = b.app(b.global("Schedule", "spaced"), b.loc("d"));
            let body = b.app3(
                rt(b, "Helper.st_aggregate_within"),
                sink,
                sched,
                b.loc("self"),
            );
            Some(b.lam(&["n", "d", "self"], body))
        }
        ("groupAdjacentBy", 2) => Some(rt(b, "Helper.st_group_adjacent_by")),
        ("flickMapAccum" | "flickMapAccumEffect", 3) => Some(rt(b, "Helper.st_map_accum")),
        ("flickMapAccumArray" | "flickMapAccumArrayEffect", 3) => {
            Some(rt(b, "Helper.st_map_accum_array"))
        }
        ("flickScan" | "flickScanEffect", 3) => Some(rt(b, "Helper.st_scan")),
        ("collect", 1) => {
            let collected = b.task(b.list([b.app(rt(b, "Helper.st_run_collect"), b.loc("self"))]));
            Some(b.lam(&["self"], b.app(rt(b, "Helper.ch_from_effect"), collected)))
        }
        ("accumulate", 1) => Some(rt(b, "Helper.st_accumulate")),
        ("changes", 1) => Some(b.app(
            rt(b, "Helper.st_changes_with"),
            b.lam(&["a", "b"], b.bin(BinOp::Eq, b.loc("a"), b.loc("b"))),
        )),
        ("changesWith" | "changesWithEffect", 2) => Some(rt(b, "Helper.st_changes_with")),
        ("intersperse", 2) => Some(rt(b, "Helper.st_intersperse")),
        ("intersperseAffixes", 2) => Some(intersperse_affixes_template(b)),
        ("debounce", 2) => Some(rt(b, "Helper.st_debounce")),
        ("flickThrottle" | "flickThrottleEffect", 2) => Some(rt(b, "Helper.st_throttle")),
        _ => None,
    }
}

fn intersperse_affixes_template(b: &ExprBuilder) -> Expr {
    let start = b.app(
        rt(b, "Helper.ch_from_list"),
        b.list([b.list([b.field(b.loc("opts"), "start")])]),
    );
    let mid = b.lam(
        &["_d"],
        b.app2(
            rt(b, "Helper.st_intersperse"),
            b.field(b.loc("opts"), "middle"),
            b.loc("self"),
        ),
    );
    let end = b.lam(
        &["_d"],
        b.app(
            rt(b, "Helper.ch_from_list"),
            b.list([b.list([b.field(b.loc("opts"), "end")])]),
        ),
    );
    let prefix = b.app2(rt(b, "Helper.ch_concat_with"), mid, start);
    b.lam(
        &["opts", "self"],
        b.app2(rt(b, "Helper.ch_concat_with"), end, prefix),
    )
}

pub(super) fn stream_window_helper(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$stw");
    match name {
        "st_rechunk" => Some(chunk::st_rechunk_helper(&b)),
        "st_sliding_size" => Some(sliding::st_sliding_size_helper(&b)),
        "st_split" => Some(chunk::st_split_helper(&b)),
        "st_group_adjacent_by" => Some(group::st_group_adjacent_by_helper(&b)),
        "st_map_accum" => Some(accum::st_map_accum_helper(&b)),
        "st_map_accum_array" => Some(accum::st_map_accum_array_helper(&b)),
        "st_scan" => Some(accum::st_scan_helper(&b)),
        "st_accumulate" => Some(accum::st_accumulate_helper(&b)),
        "st_changes_with" => Some(changes::st_changes_with_helper(&b)),
        "st_intersperse" => Some(changes::st_intersperse_helper(&b)),
        "st_debounce" => Some(debounce::st_debounce_helper(&b)),
        "st_throttle" => Some(throttle::st_throttle_helper(&b)),
        _ => None,
    }
}
