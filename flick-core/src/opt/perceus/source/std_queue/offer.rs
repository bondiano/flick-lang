//! `Queue.offer` and `Queue.offer_all` IR templates.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};
use super::shared::{q_state_pat, q_wake_takes};

fn offer_full_branch(b: &ExprBuilder) -> Expr {
    let new_offer = b.tuple([b.loc("oid"), b.loc("x"), b.loc("d")]);
    let next_offers = b.app2(
        b.global("List.Core", "append"),
        b.loc("offers"),
        b.list([new_offer]),
    );
    let suspend_reg = b.block(
        [
            b.let_("d", b.dmake()),
            b.let_(
                "oid",
                b.app(b.global("List.Core", "length"), b.loc("offers")),
            ),
            b.rset(
                b.loc("q"),
                b.tuple([
                    b.loc("cap"),
                    b.loc("strat"),
                    b.loc("items"),
                    b.loc("status"),
                    next_offers,
                    b.loc("takes"),
                    b.loc("done_d"),
                ]),
            ),
        ],
        b.err(b.loc("d")),
    );
    let slid = b.app2(
        b.global("List.Core", "append"),
        b.app2(b.global("List.Core", "drop"), b.int(1), b.loc("items")),
        b.list([b.loc("x")]),
    );
    let do_slide = b.block(
        [
            b.let_("items2", slid),
            b.rset(
                b.loc("q"),
                b.tuple([
                    b.loc("cap"),
                    b.loc("strat"),
                    b.loc("items2"),
                    b.loc("status"),
                    b.loc("offers"),
                    b.list(vec![]),
                    b.loc("done_d"),
                ]),
            ),
            q_wake_takes(b, b.loc("takes")),
        ],
        b.ok(b.bool_(true)),
    );
    let sliding_push = b.if_(
        b.bin(BinOp::Le, b.loc("cap"), b.int(0)),
        b.ok(b.bool_(false)),
        do_slide,
    );
    b.if_(
        b.bin(BinOp::Eq, b.loc("strat"), b.int(1)),
        b.ok(b.bool_(false)),
        b.if_(
            b.bin(BinOp::Eq, b.loc("strat"), b.int(2)),
            sliding_push,
            suspend_reg,
        ),
    )
}

pub(super) fn offer_template(b: &ExprBuilder) -> Expr {
    let has_room = b.bin(
        BinOp::Or,
        b.bin(BinOp::Lt, b.loc("cap"), b.int(0)),
        b.bin(
            BinOp::Lt,
            b.app(b.global("List.Core", "length"), b.loc("items")),
            b.loc("cap"),
        ),
    );
    let push_ok = b.block(
        [
            b.rset(
                b.loc("q"),
                b.tuple([
                    b.loc("cap"),
                    b.loc("strat"),
                    b.app2(
                        b.global("List.Core", "append"),
                        b.loc("items"),
                        b.list([b.loc("x")]),
                    ),
                    b.loc("status"),
                    b.loc("offers"),
                    b.list(vec![]),
                    b.loc("done_d"),
                ]),
            ),
            q_wake_takes(b, b.loc("takes")),
        ],
        b.ok(b.bool_(true)),
    );
    let open_branch = b.if_(has_room, push_ok, offer_full_branch(b));
    let on_status = b.match_(
        b.loc("status"),
        [
            arm(b.some_pat("_s"), b.ok(b.bool_(false))),
            arm(Pat::Wild, open_branch),
        ],
    );
    let try_offer = b.no_preempt(b.match_(b.rget(b.loc("q")), [arm(q_state_pat(b), on_status)]));
    let wait_offer = b.await_(b.app(b.loc("restore"), b.task(b.dawait(b.loc("wd")))));
    let masked = b.lam(
        &["restore"],
        b.match_(
            try_offer,
            [
                arm(b.ok_pat("res"), b.loc("res")),
                arm(b.err_pat("wd"), wait_offer),
            ],
        ),
    );
    b.lam(
        &["x", "q"],
        b.app(
            b.global("Effect.Concurrency", "uninterruptible_mask"),
            masked,
        ),
    )
}

fn offer_all_loop_step(b: &ExprBuilder) -> Expr {
    let push_item = b.block(
        [
            b.assign(
                "cur",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("cur"),
                    b.list([b.loc("x")]),
                ),
            ),
            b.assign("pushed", b.bool_(true)),
            b.assign("idx", b.add_i(b.loc("ci"), b.int(1))),
        ],
        b.unit(),
    );
    let drop_item = b.block(
        [
            b.assign(
                "drp",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("drp"),
                    b.list([b.loc("x")]),
                ),
            ),
            b.assign("idx", b.add_i(b.loc("ci"), b.int(1))),
        ],
        b.unit(),
    );
    let slide_item = b.block(
        [
            b.assign(
                "cur",
                b.app2(
                    b.global("List.Core", "append"),
                    b.app2(b.global("List.Core", "drop"), b.int(1), b.var("cur")),
                    b.list([b.loc("x")]),
                ),
            ),
            b.assign("pushed", b.bool_(true)),
            b.assign("idx", b.add_i(b.loc("ci"), b.int(1))),
        ],
        b.unit(),
    );
    let defer_rest = b.block(
        [
            b.assign(
                "rem",
                b.app2(b.global("List.Core", "drop"), b.loc("ci"), b.loc("xs")),
            ),
            b.assign("idx", b.loc("tot")),
        ],
        b.unit(),
    );
    let on_slide = b.if_(
        b.bin(BinOp::Le, b.loc("cap"), b.int(0)),
        drop_item.clone(),
        slide_item,
    );
    let on_full = b.if_(
        b.bin(BinOp::Eq, b.loc("strat"), b.int(1)),
        drop_item,
        b.if_(
            b.bin(BinOp::Eq, b.loc("strat"), b.int(2)),
            on_slide,
            defer_rest,
        ),
    );
    let has_room = b.bin(
        BinOp::Or,
        b.bin(BinOp::Lt, b.loc("cap"), b.int(0)),
        b.bin(BinOp::Lt, b.loc("cl"), b.loc("cap")),
    );
    b.block(
        [
            b.let_("ci", b.var("idx")),
            b.let_(
                "x",
                b.app2(b.global("$rt", "Helper.list_at"), b.loc("ci"), b.loc("xs")),
            ),
            b.let_("cl", b.app(b.global("List.Core", "length"), b.var("cur"))),
            b.let_("room", has_room),
        ],
        b.if_(b.loc("room"), push_item, on_full),
    )
}

pub(super) fn offer_all_template(b: &ExprBuilder) -> Expr {
    // Fast-path atomic batch push under `no_preempt` for items that fit without suspending;
    // falls back to `Queue.offer` for any remaining items on a full Suspend queue.
    let commit_push = b.block(
        [
            b.rset(
                b.loc("q"),
                b.tuple([
                    b.loc("cap"),
                    b.loc("strat"),
                    b.var("cur"),
                    b.loc("status"),
                    b.loc("offers"),
                    b.list(vec![]),
                    b.loc("done_d"),
                ]),
            ),
            q_wake_takes(b, b.loc("takes")),
        ],
        b.unit(),
    );
    let batch_open = b.block(
        [
            b.var_decl("cur", b.loc("items")),
            b.var_decl("drp", b.list(vec![])),
            b.var_decl("rem", b.list(vec![])),
            b.var_decl("pushed", b.bool_(false)),
            b.var_decl("idx", b.int(0)),
            b.let_("tot", b.app(b.global("List.Core", "length"), b.loc("xs"))),
            Stmt::Expr(b.while_(
                b.bin(BinOp::Lt, b.var("idx"), b.loc("tot")),
                offer_all_loop_step(b),
            )),
            Stmt::Expr(b.if_(b.var("pushed"), commit_push, b.unit())),
        ],
        b.tuple([b.var("drp"), b.var("rem")]),
    );
    let on_status = b.match_(
        b.loc("status"),
        [
            arm(b.some_pat("_s"), b.tuple([b.loc("xs"), b.list(vec![])])),
            arm(Pat::Wild, batch_open),
        ],
    );
    let batch = b.no_preempt(b.match_(b.rget(b.loc("q")), [arm(q_state_pat(b), on_status)]));
    let push_dropped = b.assign_e(
        "dropped",
        b.app2(
            b.global("List.Core", "append"),
            b.var("dropped"),
            b.list([b.loc("x")]),
        ),
    );
    let offer_rem = b.for_ctrl(
        "qoa",
        "x",
        b.loc("rem0"),
        b.if_(
            b.app2(b.global("Queue", "offer"), b.loc("x"), b.loc("q")),
            b.unit(),
            push_dropped,
        ),
    );
    let body = b.match_(
        batch,
        [arm(
            b.tup_pat(&["drp0", "rem0"]),
            b.block(
                [
                    b.var_decl("dropped", b.loc("drp0")),
                    b.let_("_u", offer_rem),
                ],
                b.var("dropped"),
            ),
        )],
    );
    b.lam(&["xs", "q"], body)
}
