//! `Stream.Transform` foreign dispatch templates.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::rt;
use super::super::{Expr, Pat};

pub(super) fn stream_transform_template(
    b: &ExprBuilder,
    bind: &str,
    stages: usize,
) -> Option<Expr> {
    match (bind, stages) {
        ("map", 2) => {
            let chunk = b.lam(
                &["xs"],
                b.gen_strict(b.each("x", b.loc("xs"), b.yield_(b.app(b.loc("f"), b.loc("x"))))),
            );
            Some(b.lam(
                &["f", "self"],
                b.app2(rt(b, "Helper.ch_map"), chunk, b.loc("self")),
            ))
        }
        ("as", 2) => {
            let chunk = b.lam(
                &["xs"],
                b.gen_strict(b.each("_x", b.loc("xs"), b.yield_(b.loc("v")))),
            );
            Some(b.lam(
                &["v", "self"],
                b.app2(rt(b, "Helper.ch_map"), chunk, b.loc("self")),
            ))
        }
        ("mapError", 2) => {
            let pull = b.lam(
                &["_u"],
                b.app(b.global("Fail", "fail"), b.app(b.loc("f"), b.loc("e"))),
            );
            let handler = b.lam(&["e", "_up", "_own"], pull);
            Some(b.lam(
                &["f", "self"],
                b.app2(rt(b, "Helper.ch_catch"), handler, b.loc("self")),
            ))
        }
        ("mapBoth", 2) => Some(map_both_template(b)),
        ("mapArray" | "mapArrayEffect", 2) => Some(rt(b, "Helper.ch_map")),
        ("mapEffect", 2) => Some(rt(b, "Helper.st_map_effect")),
        ("flickMapEffectPar", 3) => Some(rt(b, "Helper.st_map_effect_par")),
        ("flattenEffect", 1) => Some(b.app(
            rt(b, "Helper.st_map_effect"),
            b.lam(&["t"], b.await_(b.loc("t"))),
        )),
        ("result", 1) => Some(result_template(b)),
        ("tap", 2) => {
            let tap_el = b.lam(
                &["a"],
                b.block([b.let_("_r", b.app(b.loc("f"), b.loc("a")))], b.loc("a")),
            );
            Some(b.lam(
                &["f", "self"],
                b.app2(rt(b, "Helper.st_map_effect"), tap_el, b.loc("self")),
            ))
        }
        ("tapBoth", 2) => {
            let tap_el = b.lam(
                &["a"],
                b.block(
                    [b.let_("_r", b.app(b.field(b.loc("opts"), "onElement"), b.loc("a")))],
                    b.loc("a"),
                ),
            );
            let tapped_err = b.app2(
                rt(b, "Helper.ch_tap_error"),
                b.field(b.loc("opts"), "onError"),
                b.loc("self"),
            );
            Some(b.lam(
                &["opts", "self"],
                b.app2(rt(b, "Helper.st_map_effect"), tap_el, tapped_err),
            ))
        }
        _ => stream_slice_template(b, bind, stages),
    }
}

fn map_both_template(b: &ExprBuilder) -> Expr {
    let err_pull = b.lam(
        &["_u"],
        b.app(
            b.global("Fail", "fail"),
            b.app(b.field(b.loc("opts"), "onError"), b.loc("e")),
        ),
    );
    let err_handler = b.lam(&["e", "_up", "_own"], err_pull);
    let map_el = b.yield_(b.app(b.field(b.loc("opts"), "onElement"), b.loc("x")));
    let mapped = b.app2(
        rt(b, "Helper.ch_map"),
        b.lam(&["xs"], b.gen_strict(b.each("x", b.loc("xs"), map_el))),
        b.loc("self"),
    );
    b.lam(
        &["opts", "self"],
        b.app2(rt(b, "Helper.ch_catch"), err_handler, mapped),
    )
}

fn result_template(b: &ExprBuilder) -> Expr {
    let err_chunk = b.list([b.list([b.err(b.loc("e"))])]);
    let catch_fn = b.lam(&["e"], b.app(rt(b, "Helper.ch_from_list"), err_chunk));
    let ok_chunk = b.lam(
        &["xs"],
        b.gen_strict(b.each("x", b.loc("xs"), b.yield_(b.ok(b.loc("x"))))),
    );
    let mapped = b.app2(rt(b, "Helper.ch_map"), ok_chunk, b.loc("self"));
    b.lam(
        &["self"],
        b.app2(rt(b, "Helper.ch_catch"), catch_fn, mapped),
    )
}

fn stream_slice_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("filter" | "filterEffect", 2) => Some(rt(b, "Helper.ch_filter_array")),
        ("filterMap" | "filterMapEffect", 2) => Some(rt(b, "Helper.ch_filter_map_array")),
        ("take", 2) => Some(rt(b, "Helper.st_take")),
        ("takeRight", 2) => Some(rt(b, "Helper.st_take_right")),
        ("takeUntil" | "takeUntilEffect", 2) => {
            Some(b.app(rt(b, "Helper.st_take_until"), b.bool_(false)))
        }
        ("takeWhile" | "takeWhileEffect", 2) => {
            let stop = b.lam(&["a"], b.not(b.app(b.loc("p"), b.loc("a"))));
            let body = b.app3(
                rt(b, "Helper.st_take_until"),
                b.bool_(true),
                stop,
                b.loc("self"),
            );
            Some(b.lam(&["p", "self"], body))
        }
        ("takeWhileFilter", 2) => Some(rt(b, "Helper.st_take_while_filter")),
        ("drop", 2) => Some(rt(b, "Helper.st_drop")),
        ("dropRight", 2) => Some(rt(b, "Helper.st_drop_right")),
        ("dropWhile" | "dropWhileEffect", 2) => Some(rt(b, "Helper.st_drop_while")),
        ("dropWhileFilter", 2) => {
            let pred = b.lam(
                &["a"],
                b.match_(
                    b.app(b.loc("f"), b.loc("a")),
                    [
                        arm(b.ok_pat("_v"), b.bool_(true)),
                        arm(Pat::Wild, b.bool_(false)),
                    ],
                ),
            );
            Some(b.lam(
                &["f", "self"],
                b.app2(rt(b, "Helper.st_drop_while"), pred, b.loc("self")),
            ))
        }
        ("dropUntil" | "dropUntilEffect", 2) => {
            let pred = b.lam(&["a"], b.not(b.app(b.loc("p"), b.loc("a"))));
            let dropped = b.app2(rt(b, "Helper.st_drop_while"), pred, b.loc("self"));
            Some(b.lam(
                &["p", "self"],
                b.app2(rt(b, "Helper.st_drop"), b.int(1), dropped),
            ))
        }
        ("when", 2) => {
            let empty = b.lam(&["_up", "_own"], rt(b, "Helper.ch_empty_pull"));
            let body = b.task(b.if_(b.await_(b.loc("test")), b.loc("self"), empty));
            Some(b.lam(&["test", "self"], b.app(rt(b, "Helper.ch_unwrap"), body)))
        }
        ("flickBuffer", 2) => Some(b.app(rt(b, "Helper.ch_buffer"), b.bool_(true))),
        ("flickBufferArray", 2) => Some(b.app(rt(b, "Helper.ch_buffer"), b.bool_(false))),
        _ => None,
    }
}
