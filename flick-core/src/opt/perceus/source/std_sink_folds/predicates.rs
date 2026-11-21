//! Predicate and `takeWhile`/`forEachWhile` `Sink` IR helpers.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat};

fn take_state_fold(b: &ExprBuilder, step_body: Expr) -> Expr {
    let unwrap_st = b.match_(
        b.loc("st"),
        [arm(
            pair_pat(b, "xs", "_"),
            b.tuple([b.loc("xs"), b.loc("lo")]),
        )],
    );
    let map_done = b.lam(
        &["end"],
        b.match_(b.loc("end"), [arm(pair_pat(b, "st", "lo"), unwrap_st)]),
    );
    let init = b.lam(&["_u"], b.tuple([b.list(vec![]), b.bool_(true)]));
    let cont = b.lam(
        &["st"],
        b.match_(b.loc("st"), [arm(pair_pat(b, "_", "go"), b.loc("go"))]),
    );
    let step = b.lam(
        &["st", "x"],
        b.match_(b.loc("st"), [arm(pair_pat(b, "xs", "_"), step_body)]),
    );
    let folded = b.app3(rt(b, "Helper.sk_fold"), init, cont, step);
    b.app2(rt(b, "Helper.ch_map_done"), map_done, folded)
}

pub(super) fn sk_find_helper(b: &ExprBuilder) -> Expr {
    let init = b.lam(&["_u"], b.none());
    let cont = b.lam(
        &["opt"],
        b.match_(
            b.loc("opt"),
            [
                arm(b.some_pat("_"), b.bool_(false)),
                arm(Pat::Wild, b.bool_(true)),
            ],
        ),
    );
    let step = b.lam(
        &["_o", "x"],
        b.if_(
            b.app(b.loc("pred"), b.loc("x")),
            b.some(b.loc("x")),
            b.none(),
        ),
    );
    b.lam(&["pred"], b.app3(rt(b, "Helper.sk_fold"), init, cont, step))
}

pub(super) fn sk_every_helper(b: &ExprBuilder) -> Expr {
    let init = b.lam(&["_u"], b.bool_(true));
    let cont = b.lam(&["ok"], b.loc("ok"));
    let step = b.lam(&["_ok", "x"], b.app(b.loc("pred"), b.loc("x")));
    b.lam(&["pred"], b.app3(rt(b, "Helper.sk_fold"), init, cont, step))
}

pub(super) fn sk_some_helper(b: &ExprBuilder) -> Expr {
    let init = b.lam(&["_u"], b.bool_(false));
    let cont = b.lam(&["hit"], b.not(b.loc("hit")));
    let step = b.lam(&["_h", "x"], b.app(b.loc("pred"), b.loc("x")));
    b.lam(&["pred"], b.app3(rt(b, "Helper.sk_fold"), init, cont, step))
}

pub(super) fn sk_take_while_helper(b: &ExprBuilder) -> Expr {
    let pushed = b.app2(
        b.global("List.Core", "append"),
        b.loc("xs"),
        b.list([b.loc("x")]),
    );
    let step_body = b.if_(
        b.app(b.loc("pred"), b.loc("x")),
        b.tuple([pushed, b.bool_(true)]),
        b.tuple([b.loc("xs"), b.bool_(false)]),
    );
    b.lam(&["pred"], take_state_fold(b, step_body))
}

pub(super) fn sk_take_while_filter_helper(b: &ExprBuilder) -> Expr {
    let pushed = b.app2(
        b.global("List.Core", "append"),
        b.loc("xs"),
        b.list([b.loc("y")]),
    );
    let step_body = b.match_(
        b.app(b.loc("flt"), b.loc("x")),
        [
            arm(b.ok_pat("y"), b.tuple([pushed, b.bool_(true)])),
            arm(Pat::Wild, b.tuple([b.loc("xs"), b.bool_(false)])),
        ],
    );
    b.lam(&["flt"], take_state_fold(b, step_body))
}

pub(super) fn sk_take_until_helper(b: &ExprBuilder) -> Expr {
    let pushed = b.app2(
        b.global("List.Core", "append"),
        b.loc("xs"),
        b.list([b.loc("x")]),
    );
    let step_body = b.tuple([pushed, b.not(b.app(b.loc("pred"), b.loc("x")))]);
    b.lam(&["pred"], take_state_fold(b, step_body))
}

pub(super) fn sk_for_each_while_helper(b: &ExprBuilder) -> Expr {
    let map_done = b.lam(&["_end"], b.tuple([b.unit(), b.none()]));
    let folded = b.app3(
        rt(b, "Helper.sk_fold"),
        b.lam(&["_u"], b.bool_(true)),
        b.lam(&["go"], b.loc("go")),
        b.lam(&["_g", "x"], b.app(b.loc("f"), b.loc("x"))),
    );
    b.lam(
        &["f"],
        b.app2(rt(b, "Helper.ch_map_done"), map_done, folded),
    )
}

pub(super) fn sk_for_each_while_array_helper(b: &ExprBuilder) -> Expr {
    let map_done = b.lam(&["_end"], b.tuple([b.unit(), b.none()]));
    let folded = b.app3(
        rt(b, "Helper.sk_fold_array"),
        b.lam(&["_u"], b.bool_(true)),
        b.lam(&["go"], b.loc("go")),
        b.lam(&["_g", "xs"], b.app(b.loc("f"), b.loc("xs"))),
    );
    b.lam(
        &["f"],
        b.app2(rt(b, "Helper.ch_map_done"), map_done, folded),
    )
}
