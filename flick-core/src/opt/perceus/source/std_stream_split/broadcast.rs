//! `Stream.Combine` broadcast and `PubSub` subscription helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::rt;
use super::super::{Expr, Pat, Stmt};

pub(super) fn st_take_stream_helper(b: &ExprBuilder) -> Expr {
    // sub : Subscription (Result (Option Cause) (NonEmpty a)) -> Stream
    let on_cause = b.lam(
        &["c"],
        b.if_(
            b.app(b.global("Cause", "has_interrupts_only"), b.loc("c")),
            b.err(b.unit()),
            b.app(b.global("Failure", "rethrow"), b.loc("c")),
        ),
    );
    let on_end = b.match_(
        b.loc("opt_c"),
        [
            arm(
                b.some_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
            arm(Pat::Wild, b.err(b.unit())),
        ],
    );
    let take_body = b.match_(
        b.app(b.global("PubSub", "take"), b.loc("sub")),
        [
            arm(b.ok_pat("chunk"), b.ok(b.loc("chunk"))),
            arm(b.err_pat("opt_c"), on_end),
        ],
    );
    let pull = b.lam(
        &["_u"],
        b.app2(
            b.global("Effect.Errors", "catch_cause"),
            on_cause,
            b.task(take_body),
        ),
    );
    b.lam(&["sub", "_up", "_own"], pull)
}

fn spawn_pubsub_publisher(b: &ExprBuilder) -> Stmt {
    let pub_chunk = b.lam(
        &["chunk"],
        b.block(
            [b.let_(
                "_p",
                b.app2(
                    b.global("PubSub", "publish"),
                    b.ok(b.loc("chunk")),
                    b.loc("hub"),
                ),
            )],
            b.unit(),
        ),
    );
    let run_each = b.app2(rt(b, "Helper.ch_run_for_each"), pub_chunk, b.loc("self"));
    let catch_run = b.app2(
        b.global("Effect.Errors", "catch_cause"),
        b.lam(&["c"], b.some(b.loc("c"))),
        b.task(b.block([b.let_("_d", run_each)], b.none())),
    );
    let task_body = b.block(
        [
            b.let_("oc", catch_run),
            b.let_(
                "_pe",
                b.app2(
                    b.global("PubSub", "publish"),
                    b.err(b.loc("oc")),
                    b.loc("hub"),
                ),
            ),
        ],
        b.unit(),
    );
    b.let_(
        "_fib",
        b.app3(
            b.global("Fiber", "spawn"),
            b.loc("ow"),
            b.lam(&["_c"], b.unit()),
            b.task(task_body),
        ),
    )
}

pub(super) fn st_to_pubsub_take_helper(b: &ExprBuilder) -> Expr {
    // (owner, opts, self) ->{} PubSub (Result (Option Cause) (NonEmpty a))
    let shutdown = b.app(
        b.global("Scope", "add_finalizer"),
        b.task(b.app(b.global("PubSub", "shutdown"), b.loc("hub"))),
    );
    let stmts = [
        b.let_("hub", b.app(rt(b, "Helper.ch_make_pubsub"), b.loc("opts"))),
        Stmt::Expr(b.app2(rt(b, "Layer.OwnerRun"), b.loc("ow"), b.task(shutdown))),
        spawn_pubsub_publisher(b),
    ];
    b.lam(&["ow", "opts", "self"], b.block(stmts, b.loc("hub")))
}

pub(super) fn st_broadcast_helper(b: &ExprBuilder) -> Expr {
    let sub_stream = b.app(
        rt(b, "Helper.st_take_stream"),
        b.app(b.global("PubSub", "subscribe"), b.loc("hub")),
    );
    let stmts = [
        b.let_("ow", b.app(b.global("Scope", "current"), b.unit())),
        b.let_(
            "hub",
            b.app3(
                rt(b, "Helper.st_to_pubsub_take"),
                b.loc("ow"),
                b.loc("opts"),
                b.loc("self"),
            ),
        ),
    ];
    b.lam(
        &["opts", "self"],
        b.block(stmts, b.app(rt(b, "Helper.ch_unwrap"), b.task(sub_stream))),
    )
}

pub(super) fn st_broadcast_n_helper(b: &ExprBuilder) -> Expr {
    let sub_body = b.block(
        [
            b.assign("i", b.add_i(b.var("i"), b.int(1))),
            b.let_("sub", b.app(b.global("PubSub", "subscribe"), b.loc("hub"))),
        ],
        b.yield_(b.app(rt(b, "Helper.st_take_stream"), b.loc("sub"))),
    );
    let streams = b.gen_strict(b.while_(b.bin(BinOp::Lt, b.var("i"), b.loc("n")), sub_body));
    let shutdown = b.app(
        b.global("Scope", "add_finalizer"),
        b.task(b.app(b.global("PubSub", "shutdown"), b.loc("hub"))),
    );
    let stmts = [
        b.let_("ow", b.app(b.global("Scope", "current"), b.unit())),
        b.let_("hub", b.app(rt(b, "Helper.ch_make_pubsub"), b.loc("opts"))),
        Stmt::Expr(shutdown),
        b.var_decl("i", b.int(0)),
        b.let_("streams", streams),
        spawn_pubsub_publisher(b),
    ];
    b.lam(&["n", "opts", "self"], b.block(stmts, b.loc("streams")))
}
