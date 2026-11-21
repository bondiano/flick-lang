//! `Stream.Combine` concat, flatMap, drain, repeat, timeout, and merge templates.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_channel_core::DEFAULT_CHUNK_SIZE;
use super::super::std_schema_rt::{rt, str_lit};
use super::super::{Expr, Pat};

fn halt_pat(name: &str) -> Pat {
    Pat::Con(
        ExprBuilder::con("Stream.Core", "HaltStrategy", name, 0),
        vec![],
    )
}

fn drained_effect(b: &ExprBuilder, task_e: Expr) -> Expr {
    let body = b.block([b.let_("_x", b.await_(task_e))], b.err(b.unit()));
    b.lam(&["_up", "_own"], b.lam(&["_u"], body))
}

fn merge_left_drain(b: &ExprBuilder, params: &[&str], bg_task: Expr, main_loc: &str) -> Expr {
    let bg = drained_effect(b, bg_task);
    let body = b.app3(
        rt(b, "Helper.ch_merge"),
        str_lit(b, "left"),
        bg,
        b.loc(main_loc),
    );
    b.lam(params, body)
}

pub(super) fn stream_merge_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("concat", 2) => Some(b.lam(
            &["that", "self"],
            b.app2(
                rt(b, "Helper.ch_concat_with"),
                b.lam(&["_d"], b.loc("that")),
                b.loc("self"),
            ),
        )),
        ("prepend", 2) => {
            let prefix = b.app2(
                rt(b, "Helper.ch_from_list_chunked"),
                b.int(DEFAULT_CHUNK_SIZE),
                b.loc("xs"),
            );
            let body = b.app2(
                rt(b, "Helper.ch_concat_with"),
                b.lam(&["_d"], b.loc("self")),
                prefix,
            );
            Some(b.lam(&["xs", "self"], body))
        }
        ("flatMap", 2) => {
            let flat = b.app(rt(b, "Helper.ch_flatten_array"), b.loc("self"));
            Some(b.lam(
                &["f", "self"],
                b.app2(rt(b, "Helper.ch_flat_map"), b.loc("f"), flat),
            ))
        }
        ("flickFlatMapPar", 3) => Some(rt(b, "Helper.st_flat_map_par")),
        ("switchMap", 2) => {
            let flat = b.app(rt(b, "Helper.ch_flatten_array"), b.loc("self"));
            Some(b.lam(
                &["f", "self"],
                b.app2(rt(b, "Helper.ch_switch_map"), b.loc("f"), flat),
            ))
        }
        ("flatten", 1) => {
            let flat = b.app(rt(b, "Helper.ch_flatten_array"), b.loc("self"));
            let body = b.app2(
                rt(b, "Helper.ch_flat_map"),
                b.lam(&["inner"], b.loc("inner")),
                flat,
            );
            Some(b.lam(&["self"], body))
        }
        ("flattenArray", 1) => Some(rt(b, "Helper.ch_flatten_array")),
        ("flattenIterable", 1) => {
            let chunk = b.lam(
                &["xs"],
                b.app2(
                    rt(b, "Helper.ch_from_list_chunked"),
                    b.int(DEFAULT_CHUNK_SIZE),
                    b.loc("xs"),
                ),
            );
            let flat = b.app(rt(b, "Helper.ch_flatten_array"), b.loc("self"));
            Some(b.lam(&["self"], b.app2(rt(b, "Helper.ch_flat_map"), chunk, flat)))
        }
        ("drain", 1) => Some(rt(b, "Helper.ch_drain")),
        ("drainFork" | "mergeLeft", 2) => {
            let bg = b.task(b.app(rt(b, "Helper.ch_run_drain"), b.loc("other")));
            Some(merge_left_drain(b, &["other", "self"], bg, "self"))
        }
        ("mergeRight", 2) => {
            let bg = b.task(b.app(rt(b, "Helper.ch_run_drain"), b.loc("self")));
            Some(merge_left_drain(b, &["right", "self"], bg, "right"))
        }
        ("mergeEffect", 2) => Some(merge_left_drain(b, &["t", "self"], b.loc("t"), "self")),
        _ => stream_repeat_merge_template(b, bind, stages),
    }
}

fn stream_repeat_merge_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("repeat", 2) => Some(rt(b, "Helper.ch_repeat")),
        ("repeatElements", 2) => Some(rt(b, "Helper.st_repeat_elements")),
        ("forever", 1) => Some(rt(b, "Helper.ch_forever")),
        ("schedule", 2) => {
            let flat = b.app(rt(b, "Helper.ch_flatten_array"), b.loc("self"));
            let sched = b.app2(rt(b, "Helper.ch_schedule"), b.loc("sched"), flat);
            let wrapped = b.app2(
                rt(b, "Helper.ch_map"),
                b.lam(&["a"], b.list([b.loc("a")])),
                sched,
            );
            let body = b.app2(
                rt(b, "Helper.ch_map_done"),
                b.lam(&["_d"], b.unit()),
                wrapped,
            );
            Some(b.lam(&["sched", "self"], body))
        }
        ("timeout", 2) => {
            let empty = b.lam(&["_up", "_own"], rt(b, "Helper.ch_empty_pull"));
            let body = b.app3(
                rt(b, "Helper.st_timeout_or_else"),
                b.loc("d"),
                empty,
                b.loc("self"),
            );
            Some(b.lam(&["d", "self"], body))
        }
        ("flickTimeoutOrElse", 3) => Some(rt(b, "Helper.st_timeout_or_else")),
        ("merge", 2) => Some(b.app(rt(b, "Helper.ch_merge"), str_lit(b, "both"))),
        ("flickMergeWith", 3) => {
            let mode = b.match_(
                b.loc("strat"),
                [
                    arm(halt_pat("Left"), str_lit(b, "left")),
                    arm(halt_pat("Right"), str_lit(b, "right")),
                    arm(halt_pat("Either"), str_lit(b, "either")),
                    arm(Pat::Wild, str_lit(b, "both")),
                ],
            );
            let body = b.app3(rt(b, "Helper.ch_merge"), mode, b.loc("that"), b.loc("self"));
            Some(b.lam(&["strat", "that", "self"], body))
        }
        ("mergeResult", 2) => {
            let err_s = b.app2(
                rt(b, "Helper.ch_map"),
                b.lam(
                    &["xs"],
                    b.gen_strict(b.each("x", b.loc("xs"), b.yield_(b.err(b.loc("x"))))),
                ),
                b.loc("that"),
            );
            let ok_s = b.app2(
                rt(b, "Helper.ch_map"),
                b.lam(
                    &["xs"],
                    b.gen_strict(b.each("x", b.loc("xs"), b.yield_(b.ok(b.loc("x"))))),
                ),
                b.loc("self"),
            );
            let body = b.app3(rt(b, "Helper.ch_merge"), str_lit(b, "both"), err_s, ok_s);
            Some(b.lam(&["that", "self"], body))
        }
        ("flickMergeAll", 2) => {
            let src = b.app2(
                rt(b, "Helper.ch_from_list_chunked"),
                b.int(DEFAULT_CHUNK_SIZE),
                b.loc("streams"),
            );
            let body = b.app3(
                rt(b, "Helper.st_flat_map_par"),
                b.loc("opts"),
                b.lam(&["inner"], b.loc("inner")),
                src,
            );
            Some(b.lam(&["opts", "streams"], body))
        }
        _ => None,
    }
}
