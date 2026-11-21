//! `PubSub` publisher and lifecycle IR templates (`shutdown`, `end`, `publish`, `publish_all`).

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat};
use super::shared::{
    append_replay_expr, hub_pat, max_sub_len_expr, push_to_subs_stmt, state_pat, wake_pubs_false,
    wake_unit_list,
};

pub(super) fn shutdown_template(b: &ExprBuilder) -> Expr {
    let shut_state = b.tuple([
        b.loc("strat"),
        b.loc("rcap"),
        b.list(vec![]),
        b.loc("nid"),
        b.list(vec![]),
        b.loc("ended"),
        b.bool_(true),
        b.list(vec![]),
        b.list(vec![]),
        b.loc("sdef"),
    ]);
    let do_shut = b.block(
        [
            b.rset(b.loc("r"), shut_state),
            wake_pubs_false(b, b.loc("pubs")),
            wake_unit_list(b, b.loc("takes")),
            b.dsucceed(b.unit(), b.loc("sdef")),
        ],
        b.unit(),
    );
    let on_state = b.match_(
        b.rget(b.loc("r")),
        [arm(state_pat(b), b.if_(b.loc("shut"), b.unit(), do_shut))],
    );
    b.lam(
        &["hub"],
        b.no_preempt(b.match_(b.loc("hub"), [arm(hub_pat(b), on_state)])),
    )
}

pub(super) fn end_template(b: &ExprBuilder) -> Expr {
    let already_ended = b.match_(
        b.loc("ended"),
        [
            arm(b.some_pat("_e"), b.bool_(true)),
            arm(Pat::Wild, b.bool_(false)),
        ],
    );
    let is_closed = b.bin(BinOp::Or, b.loc("shut"), already_ended);
    let end_state = b.tuple([
        b.loc("strat"),
        b.loc("rcap"),
        b.loc("rbuf"),
        b.loc("nid"),
        b.loc("subs"),
        b.some(b.loc("x")),
        b.loc("shut"),
        b.list(vec![]),
        b.list(vec![]),
        b.loc("sdef"),
    ]);
    let do_end = b.block(
        [
            push_to_subs_stmt(b, b.loc("subs"), b.loc("x"), None),
            b.rset(b.loc("r"), end_state),
            wake_pubs_false(b, b.loc("pubs")),
            wake_unit_list(b, b.loc("takes")),
        ],
        b.bool_(true),
    );
    let on_state = b.match_(
        b.rget(b.loc("r")),
        [arm(state_pat(b), b.if_(is_closed, b.bool_(false), do_end))],
    );
    b.lam(
        &["x", "hub"],
        b.no_preempt(b.match_(b.loc("hub"), [arm(hub_pat(b), on_state)])),
    )
}

fn publish_do_push(b: &ExprBuilder, slide: bool) -> Expr {
    let slide_cap = if slide { Some(b.loc("cap")) } else { None };
    let next_state = b.tuple([
        b.loc("strat"),
        b.loc("rcap"),
        b.loc("rb2"),
        b.loc("nid"),
        b.loc("subs"),
        b.loc("ended"),
        b.loc("shut"),
        b.loc("pubs"),
        b.list(vec![]),
        b.loc("sdef"),
    ]);
    b.block(
        [
            b.let_(
                "rb2",
                append_replay_expr(b, b.loc("rcap"), b.loc("rbuf"), b.loc("x")),
            ),
            push_to_subs_stmt(b, b.loc("subs"), b.loc("x"), slide_cap),
            b.rset(b.loc("r"), next_state),
            wake_unit_list(b, b.loc("takes")),
        ],
        b.ok(b.bool_(true)),
    )
}

pub(super) fn publish_template(b: &ExprBuilder) -> Expr {
    let is_closed = b.bin(
        BinOp::Or,
        b.loc("shut"),
        b.match_(
            b.loc("ended"),
            [
                arm(b.some_pat("_e"), b.bool_(true)),
                arm(Pat::Wild, b.bool_(false)),
            ],
        ),
    );
    let has_room = b.bin(
        BinOp::Or,
        b.bin(BinOp::Lt, b.loc("cap"), b.int(0)),
        b.bin(BinOp::Lt, max_sub_len_expr(b, b.loc("subs")), b.loc("cap")),
    );
    let next_pubs = b.app2(
        b.global("List.Core", "append"),
        b.loc("pubs"),
        b.list([b.tuple([b.loc("x"), b.loc("pd")])]),
    );
    let suspend_pub = b.block(
        [
            b.let_("pd", b.dmake()),
            b.rset(
                b.loc("r"),
                b.tuple([
                    b.loc("strat"),
                    b.loc("rcap"),
                    b.loc("rbuf"),
                    b.loc("nid"),
                    b.loc("subs"),
                    b.loc("ended"),
                    b.loc("shut"),
                    next_pubs,
                    b.loc("takes"),
                    b.loc("sdef"),
                ]),
            ),
        ],
        b.err(b.loc("pd")),
    );
    let slide_branch = b.if_(
        b.bin(BinOp::Le, b.loc("cap"), b.int(0)),
        b.ok(b.bool_(false)),
        publish_do_push(b, true),
    );
    let full_branch = b.if_(
        b.bin(BinOp::Eq, b.loc("strat"), b.int(1)),
        b.ok(b.bool_(false)),
        b.if_(
            b.bin(BinOp::Eq, b.loc("strat"), b.int(2)),
            slide_branch,
            suspend_pub,
        ),
    );
    let open_branch = b.if_(has_room, publish_do_push(b, false), full_branch);
    let on_state = b.match_(
        b.rget(b.loc("r")),
        [arm(
            state_pat(b),
            b.if_(is_closed, b.ok(b.bool_(false)), open_branch),
        )],
    );
    let try_pub = b.no_preempt(b.match_(b.loc("hub"), [arm(hub_pat(b), on_state)]));
    let masked = b.lam(
        &["restore"],
        b.match_(
            try_pub,
            [
                arm(b.ok_pat("res"), b.loc("res")),
                arm(
                    b.err_pat("pd"),
                    b.await_(b.app(b.loc("restore"), b.task(b.dawait(b.loc("pd"))))),
                ),
            ],
        ),
    );
    b.lam(
        &["x", "hub"],
        b.app(
            b.global("Effect.Concurrency", "uninterruptible_mask"),
            masked,
        ),
    )
}

pub(super) fn publish_all_template(b: &ExprBuilder) -> Expr {
    let step = b.for_ctrl(
        "pspa",
        "x",
        b.loc("xs"),
        b.if_(
            b.app2(b.global("PubSub", "publish"), b.loc("x"), b.loc("hub")),
            b.unit(),
            b.block([b.assign("ok", b.bool_(false))], b.unit()),
        ),
    );
    b.lam(
        &["xs", "hub"],
        b.block(
            [b.var_decl("ok", b.bool_(true)), b.let_("_u", step)],
            b.var("ok"),
        ),
    )
}
