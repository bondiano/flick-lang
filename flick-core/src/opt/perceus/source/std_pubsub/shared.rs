//! Shared `PubSub` state representation, replay, wakeup, and publisher-drain IR helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};

// Hub representation: `(cap : Int, state : Ref (strat, replay_cap, replay_buf, next_id, subs, ended, shut, pubs, takes, shut_def))`
// - `cap`: `-1` for unbounded, `>= 0` for bounded
// - `strat`: `0` Bounded/Suspend, `1` Dropping, `2` Sliding
// - `subs`: `List (Int, Ref (List a))`
// - `pubs`: `List (a, Deferred none Bool)`
// - `takes`: `List (Deferred none Unit)`
// Subscription representation: `(hub, sub_id, sub_buf_ref)`
pub(super) fn hub_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["cap", "r"])
}

pub(super) fn state_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&[
        "strat", "rcap", "rbuf", "nid", "subs", "ended", "shut", "pubs", "takes", "sdef",
    ])
}

pub(super) fn sub_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["hub", "sid", "sbuf"])
}

pub(super) fn max_sub_len_expr(b: &ExprBuilder, subs_expr: Expr) -> Expr {
    let update_max = b.if_(
        b.bin(BinOp::Gt, b.loc("n"), b.var("mx")),
        b.block([b.assign("mx", b.loc("n"))], b.unit()),
        b.unit(),
    );
    let check_sub = b.block(
        [b.let_(
            "n",
            b.app(b.global("List.Core", "length"), b.rget(b.loc("sb"))),
        )],
        update_max,
    );
    let step = b.for_ctrl(
        "psl",
        "sub",
        subs_expr,
        b.match_(b.loc("sub"), [arm(b.tup_pat(&["_id", "sb"]), check_sub)]),
    );
    b.block(
        [b.var_decl("mx", b.int(0)), b.let_("_u", step)],
        b.var("mx"),
    )
}

pub(super) fn wake_unit_list(b: &ExprBuilder, list_expr: Expr) -> Stmt {
    b.let_(
        "_wu",
        b.for_ctrl(
            "pswu",
            "d",
            list_expr,
            b.block([b.dsucceed(b.unit(), b.loc("d"))], b.unit()),
        ),
    )
}

pub(super) fn wake_pubs_false(b: &ExprBuilder, list_expr: Expr) -> Stmt {
    let wake_one = b.match_(
        b.loc("pw"),
        [arm(
            b.tup_pat(&["_x", "d"]),
            b.block([b.dsucceed(b.bool_(false), b.loc("d"))], b.unit()),
        )],
    );
    b.let_("_wp", b.for_ctrl("pswp", "pw", list_expr, wake_one))
}

pub(super) fn append_replay_expr(b: &ExprBuilder, rcap: Expr, rbuf: Expr, val: Expr) -> Expr {
    let trim = b.if_(
        b.bin(BinOp::Gt, b.loc("rl"), rcap.clone()),
        b.app2(
            b.global("List.Core", "drop"),
            b.sub_i(b.loc("rl"), rcap.clone()),
            b.loc("rb1"),
        ),
        b.loc("rb1"),
    );
    let push_rb = b.block(
        [
            b.let_(
                "rb1",
                b.app2(b.global("List.Core", "append"), rbuf.clone(), b.list([val])),
            ),
            b.let_("rl", b.app(b.global("List.Core", "length"), b.loc("rb1"))),
        ],
        trim,
    );
    b.if_(b.bin(BinOp::Le, rcap, b.int(0)), rbuf, push_rb)
}

pub(super) fn push_to_subs_stmt(
    b: &ExprBuilder,
    subs_expr: Expr,
    val_expr: Expr,
    slide_cap: Option<Expr>,
) -> Stmt {
    let push_one = match slide_cap {
        Some(cap) => {
            let full = b.bin(
                BinOp::And,
                b.bin(BinOp::Ge, cap.clone(), b.int(0)),
                b.bin(BinOp::Ge, b.loc("cl"), cap),
            );
            let base = b.if_(
                full,
                b.app2(b.global("List.Core", "drop"), b.int(1), b.loc("cur")),
                b.loc("cur"),
            );
            b.block(
                [
                    b.let_("cur", b.rget(b.loc("sb"))),
                    b.let_("cl", b.app(b.global("List.Core", "length"), b.loc("cur"))),
                    b.let_("base", base),
                    b.rset(
                        b.loc("sb"),
                        b.app2(
                            b.global("List.Core", "append"),
                            b.loc("base"),
                            b.list([val_expr]),
                        ),
                    ),
                ],
                b.unit(),
            )
        }
        None => b.block(
            [b.rset(
                b.loc("sb"),
                b.app2(
                    b.global("List.Core", "append"),
                    b.rget(b.loc("sb")),
                    b.list([val_expr]),
                ),
            )],
            b.unit(),
        ),
    };
    b.let_(
        "_pspush",
        b.for_ctrl(
            "pspush",
            "sub",
            subs_expr,
            b.match_(b.loc("sub"), [arm(b.tup_pat(&["_id", "sb"]), push_one)]),
        ),
    )
}

fn drain_pubs_stmt(b: &ExprBuilder) -> Stmt {
    // Assumes `cap`, `subs`, mutable vars `v_rbuf`, `v_pubs`, `woken_pubs`, `did_push`, and `rcap`.
    let at_cap = b.bin(
        BinOp::And,
        b.bin(BinOp::Ge, b.loc("cap"), b.int(0)),
        b.bin(BinOp::Ge, max_sub_len_expr(b, b.loc("subs")), b.loc("cap")),
    );
    let pop_pat = Pat::List(
        vec![b.tup_pat(&["px", "pd"])],
        Some(Box::new(Pat::Bind(b.name("prest")))),
    );
    let pop_pub = b.block(
        [
            b.assign(
                "v_rbuf",
                append_replay_expr(b, b.loc("rcap"), b.var("v_rbuf"), b.loc("px")),
            ),
            push_to_subs_stmt(b, b.loc("subs"), b.loc("px"), None),
            b.assign("v_pubs", b.loc("prest")),
            b.assign(
                "woken_pubs",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("woken_pubs"),
                    b.list([b.loc("pd")]),
                ),
            ),
            b.assign("did_push", b.bool_(true)),
        ],
        b.unit(),
    );
    let pop_match = b.match_(
        b.var("v_pubs"),
        [
            arm(Pat::List(vec![], None), b.break_("psdp", b.unit())),
            arm(pop_pat, pop_pub),
        ],
    );
    b.let_(
        "_psdp",
        b.loop_ctrl("psdp", b.if_(at_cap, b.break_("psdp", b.unit()), pop_match)),
    )
}

pub(super) fn make_hub(b: &ExprBuilder, strat: i32, bounded: bool) -> Expr {
    let cap = if bounded {
        b.field(b.loc("opts"), "capacity")
    } else {
        b.int(-1)
    };
    let rcap = b.match_(
        b.field(b.loc("opts"), "replay"),
        [arm(b.some_pat("n"), b.loc("n")), arm(Pat::Wild, b.int(0))],
    );
    let state = b.tuple([
        b.int(strat),
        rcap,
        b.list(vec![]),
        b.int(0),
        b.list(vec![]),
        b.none(),
        b.bool_(false),
        b.list(vec![]),
        b.list(vec![]),
        b.dmake(),
    ]);
    b.lam(&["opts"], b.tuple([cap, b.rmake(state)]))
}

pub(super) fn after_consume_expr(b: &ExprBuilder) -> Expr {
    let wake_wp = b.for_ctrl(
        "pswp2",
        "pd",
        b.var("woken_pubs"),
        b.block([b.dsucceed(b.bool_(true), b.loc("pd"))], b.unit()),
    );
    let next_state = b.tuple([
        b.loc("strat"),
        b.loc("rcap"),
        b.var("v_rbuf"),
        b.loc("nid"),
        b.loc("subs"),
        b.loc("ended"),
        b.loc("shut"),
        b.var("v_pubs"),
        b.if_(b.var("did_push"), b.list(vec![]), b.loc("takes")),
        b.loc("sdef"),
    ]);
    let wake_takes = b.if_(
        b.var("did_push"),
        b.block([wake_unit_list(b, b.loc("takes"))], b.unit()),
        b.unit(),
    );
    let body = b.block(
        [
            b.var_decl("v_rbuf", b.loc("rbuf")),
            b.var_decl("v_pubs", b.loc("pubs")),
            b.var_decl("woken_pubs", b.list(vec![])),
            b.var_decl("did_push", b.bool_(false)),
            drain_pubs_stmt(b),
            b.rset(b.loc("r"), next_state),
            b.let_("_wp2", wake_wp),
            Stmt::Expr(wake_takes),
        ],
        b.unit(),
    );
    let on_state = b.match_(b.rget(b.loc("r")), [arm(state_pat(b), body)]);
    b.match_(b.loc("hub"), [arm(hub_pat(b), on_state)])
}
