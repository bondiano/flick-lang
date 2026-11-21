//! Core `Sink` fold, take, head, last, and collect IR helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat, Stmt};

fn opt_non_empty(b: &ExprBuilder, xs: Expr) -> Expr {
    let len = b.app(b.global("List.Core", "length"), b.loc("rem"));
    let check = b.if_(
        b.bin(BinOp::Gt, len, b.int(0)),
        b.some(b.loc("rem")),
        b.none(),
    );
    b.block([b.let_("rem", xs)], check)
}

pub(super) fn sk_fold_helper(b: &ExprBuilder) -> Expr {
    // (init, cont, f) -> Sink
    let stop_check = b.if_(
        b.not(b.app(b.loc("cont"), b.var("acc"))),
        b.assign_e("stopped", b.bool_(true)),
        b.unit(),
    );
    let inner_body = b.block(
        [
            b.let_(
                "x",
                b.app2(rt(b, "Helper.list_at"), b.var("i"), b.loc("xs")),
            ),
            b.assign("acc", b.app2(b.loc("f"), b.var("acc"), b.loc("x"))),
            b.assign("i", b.add_i(b.var("i"), b.int(1))),
            Stmt::Expr(stop_check),
        ],
        b.unit(),
    );
    let cond = b.bin(
        BinOp::And,
        b.not(b.var("stopped")),
        b.bin(BinOp::Lt, b.var("i"), b.loc("n")),
    );
    let leftover = opt_non_empty(
        b,
        b.app2(b.global("List.Core", "drop"), b.var("i"), b.loc("xs")),
    );
    let chunk_tail = b.if_(
        b.var("stopped"),
        b.break_("skfl", b.err(b.tuple([b.var("acc"), leftover]))),
        b.unit(),
    );
    let chunk_loop = b.block(
        [
            b.let_("n", b.app(b.global("List.Core", "length"), b.loc("xs"))),
            b.var_decl("i", b.int(0)),
            b.var_decl("stopped", b.bool_(false)),
            Stmt::Expr(b.while_(cond, inner_body)),
        ],
        chunk_tail,
    );
    let done_err = b.err(b.tuple([b.var("acc"), b.none()]));
    let pull_loop = b.loop_ctrl(
        "skfl",
        b.match_(
            b.app(b.loc("up"), b.unit()),
            [
                arm(b.ok_pat("xs"), chunk_loop),
                arm(b.err_pat("_d"), b.break_("skfl", done_err.clone())),
            ],
        ),
    );
    let body = b.block(
        [b.var_decl("acc", b.app(b.loc("init"), b.unit()))],
        b.if_(
            b.not(b.app(b.loc("cont"), b.var("acc"))),
            done_err,
            pull_loop,
        ),
    );
    b.lam(&["init", "cont", "f", "up", "_own"], b.lam(&["_u"], body))
}

pub(super) fn sk_fold_array_helper(b: &ExprBuilder) -> Expr {
    let done_break = b.break_("skfa", b.err(b.tuple([b.var("acc"), b.none()])));
    let on_pull = b.match_(
        b.app(b.loc("up"), b.unit()),
        [
            arm(
                b.ok_pat("xs"),
                b.assign_e("acc", b.app2(b.loc("f"), b.var("acc"), b.loc("xs"))),
            ),
            arm(b.err_pat("_d"), done_break.clone()),
        ],
    );
    let step = b.if_(
        b.not(b.app(b.loc("cont"), b.var("acc"))),
        done_break,
        on_pull,
    );
    let body = b.block(
        [b.var_decl("acc", b.app(b.loc("init"), b.unit()))],
        b.loop_ctrl("skfa", step),
    );
    b.lam(&["init", "cont", "f", "up", "_own"], b.lam(&["_u"], body))
}

pub(super) fn sk_fold_until_helper(b: &ExprBuilder) -> Expr {
    let unwrap_st = b.match_(
        b.loc("st"),
        [arm(
            pair_pat(b, "acc", "_cnt"),
            b.tuple([b.loc("acc"), b.loc("lo")]),
        )],
    );
    let map_done = b.lam(
        &["end"],
        b.match_(b.loc("end"), [arm(pair_pat(b, "st", "lo"), unwrap_st)]),
    );
    let init = b.lam(&["_u"], b.tuple([b.app(b.loc("init"), b.unit()), b.int(0)]));
    let cont = b.lam(
        &["st"],
        b.match_(
            b.loc("st"),
            [arm(
                pair_pat(b, "_", "cnt"),
                b.bin(BinOp::Lt, b.loc("cnt"), b.loc("max_n")),
            )],
        ),
    );
    let step_arm = arm(
        pair_pat(b, "acc", "cnt"),
        b.tuple([
            b.app2(b.loc("f"), b.loc("acc"), b.loc("x")),
            b.add_i(b.loc("cnt"), b.int(1)),
        ]),
    );
    let step = b.lam(&["st", "x"], b.match_(b.loc("st"), [step_arm]));
    let folded = b.app3(rt(b, "Helper.sk_fold"), init, cont, step);
    b.lam(
        &["init", "max_n", "f"],
        b.app2(rt(b, "Helper.ch_map_done"), map_done, folded),
    )
}

pub(super) fn sk_take_helper(b: &ExprBuilder) -> Expr {
    let init = b.lam(&["_u"], b.list(vec![]));
    let append = b.app2(
        b.global("List.Core", "append"),
        b.loc("xs"),
        b.list([b.loc("x")]),
    );
    let step = b.lam(&["xs", "x"], append);
    b.lam(
        &["n"],
        b.app3(rt(b, "Helper.sk_fold_until"), init, b.loc("n"), step),
    )
}

pub(super) fn sk_head_helper(b: &ExprBuilder) -> Expr {
    let empty = b.err(b.tuple([b.none(), b.none()]));
    let cons_pat = Pat::List(
        vec![Pat::Bind(b.name("h"))],
        Some(Box::new(Pat::Bind(b.name("tl")))),
    );
    let on_xs = b.match_(
        b.loc("xs"),
        [
            arm(
                cons_pat,
                b.err(b.tuple([b.some(b.loc("h")), opt_non_empty(b, b.loc("tl"))])),
            ),
            arm(Pat::Wild, empty.clone()),
        ],
    );
    let body = b.match_(
        b.app(b.loc("up"), b.unit()),
        [arm(b.ok_pat("xs"), on_xs), arm(b.err_pat("_d"), empty)],
    );
    b.lam(&["up", "_own"], b.lam(&["_u"], body))
}

pub(super) fn sk_last_helper(b: &ExprBuilder) -> Expr {
    let last_elem = b.app2(
        rt(b, "Helper.list_at"),
        b.sub_i(b.loc("n"), b.int(1)),
        b.loc("xs"),
    );
    let on_xs = b.block(
        [b.let_("n", b.app(b.global("List.Core", "length"), b.loc("xs")))],
        b.if_(
            b.bin(BinOp::Gt, b.loc("n"), b.int(0)),
            b.assign_e("last", b.some(last_elem)),
            b.unit(),
        ),
    );
    let done = b.break_("skls", b.err(b.tuple([b.var("last"), b.none()])));
    let pull_loop = b.loop_ctrl(
        "skls",
        b.match_(
            b.app(b.loc("up"), b.unit()),
            [arm(b.ok_pat("xs"), on_xs), arm(b.err_pat("_d"), done)],
        ),
    );
    let body = b.block([b.var_decl("last", b.none())], pull_loop);
    b.lam(&["up", "_own"], b.lam(&["_u"], body))
}

pub(super) fn sk_collect_helper(b: &ExprBuilder) -> Expr {
    let gen_body = b.loop_ctrl(
        "skcl",
        b.match_(
            b.app(b.loc("up"), b.unit()),
            [
                arm(
                    b.ok_pat("xs"),
                    b.each("x", b.loc("xs"), b.yield_(b.loc("x"))),
                ),
                arm(b.err_pat("_d"), b.break_("skcl", b.unit())),
            ],
        ),
    );
    let body = b.err(b.tuple([b.gen_strict(gen_body), b.none()]));
    b.lam(&["up", "_own"], b.lam(&["_u"], body))
}
