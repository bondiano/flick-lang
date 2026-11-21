//! `PubSub` subscription and consumer IR templates (`subscribe`, `take`, `take_all`, `take_up_to`, `take_between`).

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};
use super::shared::{after_consume_expr, hub_pat, state_pat, sub_pat};

fn unsubscribe_expr(b: &ExprBuilder) -> Expr {
    let keep_other = b.lam(
        &["e"],
        b.match_(
            b.loc("e"),
            [arm(
                b.tup_pat(&["id", "_sb"]),
                b.not(b.bin(BinOp::Eq, b.loc("id"), b.loc("sid"))),
            )],
        ),
    );
    let next_state = b.tuple([
        b.loc("strat"),
        b.loc("rcap"),
        b.loc("rbuf"),
        b.loc("nid"),
        b.loc("subs2"),
        b.loc("ended"),
        b.loc("shut"),
        b.loc("pubs"),
        b.loc("takes"),
        b.loc("sdef"),
    ]);
    let body = b.block(
        [
            b.let_(
                "subs2",
                b.app2(b.global("List.Core", "filter"), keep_other, b.loc("subs")),
            ),
            b.rset(b.loc("r"), next_state),
        ],
        after_consume_expr(b),
    );
    b.no_preempt(b.match_(b.rget(b.loc("r")), [arm(state_pat(b), body)]))
}

pub(super) fn subscribe_template(b: &ExprBuilder) -> Expr {
    let init_buf = b.match_(
        b.loc("ended"),
        [
            arm(
                b.some_pat("ev"),
                b.app2(
                    b.global("List.Core", "append"),
                    b.loc("rbuf"),
                    b.list([b.loc("ev")]),
                ),
            ),
            arm(Pat::Wild, b.loc("rbuf")),
        ],
    );
    let entry = b.tuple([b.loc("sid"), b.loc("sbuf")]);
    let next_subs = b.if_(
        b.loc("shut"),
        b.loc("subs"),
        b.app2(
            b.global("List.Core", "append"),
            b.loc("subs"),
            b.list([entry]),
        ),
    );
    let next_state = b.tuple([
        b.loc("strat"),
        b.loc("rcap"),
        b.loc("rbuf"),
        b.add_i(b.loc("nid"), b.int(1)),
        next_subs,
        b.loc("ended"),
        b.loc("shut"),
        b.loc("pubs"),
        b.loc("takes"),
        b.loc("sdef"),
    ]);
    let register = b.no_preempt(b.match_(
        b.rget(b.loc("r")),
        [arm(
            state_pat(b),
            b.block(
                [
                    b.let_("sid", b.loc("nid")),
                    b.let_("init", init_buf),
                    b.let_("sbuf", b.rmake(b.loc("init"))),
                    b.rset(b.loc("r"), next_state),
                ],
                b.tuple([b.loc("sid"), b.loc("sbuf")]),
            ),
        )],
    ));
    let attach_fin = b.block(
        [Stmt::Expr(b.app(
            b.global("Scope", "add_finalizer"),
            b.task(unsubscribe_expr(b)),
        ))],
        b.tuple([b.loc("hub"), b.loc("sid"), b.loc("sbuf")]),
    );
    let body = b.match_(register, [arm(b.tup_pat(&["sid", "sbuf"]), attach_fin)]);
    b.lam(&["hub"], b.match_(b.loc("hub"), [arm(hub_pat(b), body)]))
}

fn park_sub_reg(b: &ExprBuilder) -> Expr {
    let next_takes = b.app2(
        b.global("List.Core", "append"),
        b.loc("takes"),
        b.list([b.loc("td")]),
    );
    let next_state = b.tuple([
        b.loc("strat"),
        b.loc("rcap"),
        b.loc("rbuf"),
        b.loc("nid"),
        b.loc("subs"),
        b.loc("ended"),
        b.loc("shut"),
        b.loc("pubs"),
        next_takes,
        b.loc("sdef"),
    ]);
    let wait_td = b.block(
        [b.let_("td", b.dmake()), b.rset(b.loc("r"), next_state)],
        b.err(b.some(b.loc("td"))),
    );
    let on_state = b.match_(
        b.rget(b.loc("r")),
        [arm(
            state_pat(b),
            b.if_(b.loc("shut"), b.err(b.none()), wait_td),
        )],
    );
    b.match_(b.loc("hub"), [arm(hub_pat(b), on_state)])
}

fn await_opt_td(b: &ExprBuilder) -> Expr {
    b.match_(
        b.loc("opt_td"),
        [
            arm(b.some_pat("td"), b.dawait(b.loc("td"))),
            arm(
                Pat::Wild,
                b.app(b.global("Effect.Concurrency", "interrupt"), b.unit()),
            ),
        ],
    )
}

fn park_sub_take(b: &ExprBuilder) -> Expr {
    let check_buf = b.no_preempt(b.match_(
        b.rget(b.loc("sbuf")),
        [
            arm(Pat::List(vec![], None), park_sub_reg(b)),
            arm(Pat::Wild, b.ok(b.unit())),
        ],
    ));
    b.match_(
        check_buf,
        [
            arm(b.ok_pat("_u"), b.unit()),
            arm(b.err_pat("opt_td"), await_opt_td(b)),
        ],
    )
}

pub(super) fn sub_take_template(b: &ExprBuilder) -> Expr {
    let cons_pat = Pat::List(
        vec![Pat::Bind(b.name("h"))],
        Some(Box::new(Pat::Bind(b.name("tl")))),
    );
    let pop_one = b.block(
        [
            b.rset(b.loc("sbuf"), b.loc("tl")),
            Stmt::Expr(after_consume_expr(b)),
        ],
        b.ok(b.loc("h")),
    );
    let try_step = b.no_preempt(b.match_(
        b.rget(b.loc("sbuf")),
        [arm(cons_pat, pop_one), arm(Pat::Wild, park_sub_reg(b))],
    ));
    let step = b.match_(
        try_step,
        [
            arm(b.ok_pat("v"), b.break_("pst", b.loc("v"))),
            arm(b.err_pat("opt_td"), await_opt_td(b)),
        ],
    );
    b.lam(
        &["sub"],
        b.match_(b.loc("sub"), [arm(sub_pat(b), b.loop_ctrl("pst", step))]),
    )
}

pub(super) fn sub_take_all_template(b: &ExprBuilder) -> Expr {
    let cons_pat = Pat::List(
        vec![Pat::Bind(b.name("h"))],
        Some(Box::new(Pat::Bind(b.name("tl")))),
    );
    let pop_all = b.block(
        [
            b.rset(b.loc("sbuf"), b.list(vec![])),
            Stmt::Expr(after_consume_expr(b)),
        ],
        b.ok(b.app2(b.global("NonEmpty", "cons"), b.loc("h"), b.loc("tl"))),
    );
    let try_step = b.no_preempt(b.match_(
        b.rget(b.loc("sbuf")),
        [arm(cons_pat, pop_all), arm(Pat::Wild, park_sub_reg(b))],
    ));
    let step = b.match_(
        try_step,
        [
            arm(b.ok_pat("v"), b.break_("psta", b.loc("v"))),
            arm(b.err_pat("opt_td"), await_opt_td(b)),
        ],
    );
    b.lam(
        &["sub"],
        b.match_(b.loc("sub"), [arm(sub_pat(b), b.loop_ctrl("psta", step))]),
    )
}

pub(super) fn sub_take_up_to_template(b: &ExprBuilder) -> Expr {
    let take_slice = b.block(
        [
            b.let_("cur", b.rget(b.loc("sbuf"))),
            b.let_(
                "got",
                b.app2(b.global("List.Core", "take"), b.loc("mx"), b.loc("cur")),
            ),
            b.rset(
                b.loc("sbuf"),
                b.app2(b.global("List.Core", "drop"), b.loc("mx"), b.loc("cur")),
            ),
            Stmt::Expr(after_consume_expr(b)),
        ],
        b.loc("got"),
    );
    let body = b.if_(
        b.bin(BinOp::Le, b.loc("mx"), b.int(0)),
        b.list(vec![]),
        b.no_preempt(b.match_(b.loc("sub"), [arm(sub_pat(b), take_slice)])),
    );
    b.lam(&["mx", "sub"], body)
}

pub(super) fn sub_take_between_template(b: &ExprBuilder) -> Expr {
    let on_empty_chunk = b.if_(
        b.bin(BinOp::Ge, b.loc("got"), b.loc("mn")),
        b.break_("pstb", b.var("acc")),
        park_sub_take(b),
    );
    let enough = b.bin(
        BinOp::Ge,
        b.app(b.global("List.Core", "length"), b.var("acc")),
        b.loc("mn"),
    );
    let on_chunk = b.block(
        [b.assign(
            "acc",
            b.app2(
                b.global("List.Core", "append"),
                b.var("acc"),
                b.loc("chunk"),
            ),
        )],
        b.if_(enough, b.break_("pstb", b.var("acc")), b.unit()),
    );
    let pull_chunk = b.block(
        [b.let_(
            "chunk",
            b.app2(
                b.global("PubSub", "take_up_to"),
                b.loc("need"),
                b.loc("sub"),
            ),
        )],
        b.match_(
            b.loc("chunk"),
            [
                arm(Pat::List(vec![], None), on_empty_chunk),
                arm(Pat::Wild, on_chunk),
            ],
        ),
    );
    let step = b.block(
        [
            b.let_("got", b.app(b.global("List.Core", "length"), b.var("acc"))),
            b.let_("need", b.sub_i(b.loc("mx"), b.loc("got"))),
        ],
        b.if_(
            b.bin(BinOp::Le, b.loc("need"), b.int(0)),
            b.break_("pstb", b.var("acc")),
            pull_chunk,
        ),
    );
    let body = b.block(
        [b.var_decl("acc", b.list(vec![]))],
        b.if_(
            b.bin(BinOp::Lt, b.loc("mx"), b.loc("mn")),
            b.list(vec![]),
            b.loop_ctrl("pstb", step),
        ),
    );
    b.lam(
        &["mn", "mx", "sub"],
        b.match_(b.loc("sub"), [arm(sub_pat(b), body)]),
    )
}
