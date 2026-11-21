//! `Stream.Combine` shared-stream (`flickShare`) helper.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat, Stmt};

fn share_release(b: &ExprBuilder) -> Expr {
    let close = b.match_(
        b.loc("pair"),
        [arm(
            pair_pat(b, "_hub", "close_fn"),
            b.block(
                [b.rset(b.loc("cur"), b.none())],
                b.app(b.loc("close_fn"), b.none()),
            ),
        )],
    );
    b.lam(
        &["_u"],
        b.match_(
            b.rget(b.loc("cur")),
            [arm(b.some_pat("pair"), close), arm(Pat::Wild, b.unit())],
        ),
    )
}

fn share_acquire_hub(b: &ExprBuilder) -> Expr {
    let existing = b.match_(b.loc("p"), [arm(pair_pat(b, "h", "_"), b.loc("h"))]);
    let start_body = b.block(
        [
            b.let_(
                "h",
                b.app3(
                    rt(b, "Helper.st_to_pubsub_take"),
                    b.loc("co"),
                    b.loc("opts"),
                    b.loc("self"),
                ),
            ),
            b.rset(b.loc("cur"), b.some(b.tuple([b.loc("h"), b.loc("cclose")]))),
        ],
        b.loc("h"),
    );
    let fresh = b.match_(
        b.app(rt(b, "Helper.ch_fork_scope"), b.loc("parent")),
        [arm(pair_pat(b, "co", "cclose"), start_body)],
    );
    b.match_(
        b.rget(b.loc("cur")),
        [arm(b.some_pat("p"), existing), arm(Pat::Wild, fresh)],
    )
}

fn share_drop_finalizer(b: &ExprBuilder) -> Expr {
    let still_idle = b.bin(
        BinOp::And,
        b.bin(BinOp::Eq, b.rget(b.loc("refs")), b.int(0)),
        b.bin(BinOp::Eq, b.rget(b.loc("ver")), b.loc("vv")),
    );
    let sleep_task = b.task(b.block(
        [Stmt::Expr(b.app(rt(b, "Async.EffectSleep"), b.loc("ttl")))],
        b.if_(still_idle, b.app(b.loc("rel"), b.unit()), b.unit()),
    ));
    let spawn_timer = b.block(
        [b.let_(
            "_tf",
            b.app3(
                b.global("Fiber", "spawn"),
                b.loc("parent"),
                b.lam(&["_c"], b.unit()),
                sleep_task,
            ),
        )],
        b.unit(),
    );
    let on_ttl = b.if_(
        b.app(rt(b, "Duration.IsPositive"), b.loc("ttl")),
        spawn_timer,
        b.app(b.loc("rel"), b.unit()),
    );
    let on_zero = b.match_(
        b.loc("idle"),
        [
            arm(b.some_pat("ttl"), on_ttl),
            arm(Pat::Wild, b.app(b.loc("rel"), b.unit())),
        ],
    );
    let body = b.block(
        [
            b.let_("nr", b.sub_i(b.rget(b.loc("refs")), b.int(1))),
            b.rset(b.loc("refs"), b.loc("nr")),
            b.let_("vv", b.rget(b.loc("ver"))),
        ],
        b.if_(b.bin(BinOp::Gt, b.loc("nr"), b.int(0)), b.unit(), on_zero),
    );
    b.app(b.global("Scope", "add_finalizer"), b.task(body))
}

pub(super) fn st_share_helper(b: &ExprBuilder) -> Expr {
    // (opts, idle_ttl : Option Duration, self) ->{Scope} Stream
    let subscribe = b.block(
        [
            b.let_("hub", share_acquire_hub(b)),
            b.rset(b.loc("refs"), b.add_i(b.rget(b.loc("refs")), b.int(1))),
            b.let_("v_now", b.add_i(b.rget(b.loc("ver")), b.int(1))),
            b.rset(b.loc("ver"), b.loc("v_now")),
            Stmt::Expr(share_drop_finalizer(b)),
            b.let_("sub", b.app(b.global("PubSub", "subscribe"), b.loc("hub"))),
        ],
        b.app(rt(b, "Helper.st_take_stream"), b.loc("sub")),
    );
    let stmts = [
        b.let_("parent", b.app(b.global("Scope", "current"), b.unit())),
        b.let_("cur", b.rmake(b.none())),
        b.let_("refs", b.rmake(b.int(0))),
        b.let_("ver", b.rmake(b.int(0))),
        b.let_("rel", share_release(b)),
        Stmt::Expr(b.app(
            b.global("Scope", "add_finalizer"),
            b.task(b.app(b.loc("rel"), b.unit())),
        )),
    ];
    b.lam(
        &["opts", "idle", "self"],
        b.block(stmts, b.app(rt(b, "Helper.ch_unwrap"), b.task(subscribe))),
    )
}
