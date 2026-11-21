//! Scope, bracketing, callback, and acquire/release `Channel.Core` IR helpers.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, ExprKind, Pat, Stmt};

pub(super) fn ch_run_scoped_helper(b: &ExprBuilder) -> Expr {
    // (ch, consume : Pull -> a) ->{} a
    let close_on_exit = b.lam(
        &["ex"],
        b.some(b.task(b.app2(
            rt(b, "Layer.OwnerClose"),
            b.loc("own"),
            b.app(b.global("Exit", "failure"), b.loc("ex")),
        ))),
    );
    let run_consume = b.block(
        [b.let_(
            "pull",
            b.app2(b.loc("ch"), rt(b, "Helper.ch_empty_pull"), b.loc("own")),
        )],
        b.app(b.loc("consume"), b.loc("pull")),
    );
    let owned_run = b.app(
        b.loc("restore"),
        b.task(b.app2(rt(b, "Layer.OwnerRun"), b.loc("own"), b.task(run_consume))),
    );
    let masked = b.lam(
        &["restore"],
        b.block(
            [b.let_("own", b.app(rt(b, "Layer.OwnerNew"), b.unit()))],
            b.app3(
                b.global("Effect.Errors", "on_exit_primitive"),
                b.bool_(false),
                close_on_exit,
                owned_run,
            ),
        ),
    );
    b.lam(
        &["ch", "consume"],
        b.app(
            b.global("Effect.Concurrency", "uninterruptible_mask"),
            masked,
        ),
    )
}

pub(super) fn ch_fork_scope_helper(b: &ExprBuilder) -> Expr {
    // parent_owner ->{} (child_owner, close_once : Option Cause ->{} Unit)
    let do_close = b.block(
        [b.rset(b.loc("box"), b.none())],
        b.app2(rt(b, "Layer.OwnerClose"), b.loc("o"), b.loc("opt_c")),
    );
    let close_body = b.app(
        b.global("Effect.Concurrency", "uninterruptible_mask"),
        b.lam(
            &["_r"],
            b.match_(
                b.rget(b.loc("box")),
                [arm(b.some_pat("o"), do_close), arm(Pat::Wild, b.unit())],
            ),
        ),
    );
    let add_fin = b.app2(
        rt(b, "Layer.OwnerRun"),
        b.loc("parent"),
        b.task(b.app(
            b.global("Scope", "add_finalizer_exit"),
            b.lam(&["oc"], b.app(b.loc("close"), b.loc("oc"))),
        )),
    );
    let masked = b.lam(
        &["_restore"],
        b.block(
            [
                b.let_("child", b.app(rt(b, "Layer.OwnerNew"), b.unit())),
                b.let_("box", b.rmake(b.some(b.loc("child")))),
                b.let_("close", b.lam(&["opt_c"], close_body)),
                Stmt::Expr(add_fin),
            ],
            b.tuple([b.loc("child"), b.loc("close")]),
        ),
    );
    b.lam(
        &["parent"],
        b.app(
            b.global("Effect.Concurrency", "uninterruptible_mask"),
            masked,
        ),
    )
}

pub(super) fn ch_bracket_helper(b: &ExprBuilder) -> Expr {
    // (build : (up, parent_own, forked_own) -> Pull) -> Channel
    // Closes `forked_own` as soon as the returned pull ends (`Err d`) or fails/interrupts!
    let on_exit = b.lam(
        &["ex"],
        b.match_(
            b.app(b.global("Exit", "failure"), b.loc("ex")),
            [
                arm(
                    b.some_pat("c"),
                    b.some(b.task(b.app(b.loc("close"), b.some(b.loc("c"))))),
                ),
                arm(Pat::Wild, b.none()),
            ],
        ),
    );
    let on_done = b.block(
        [Stmt::Expr(b.app(b.loc("close"), b.none()))],
        b.err(b.loc("d")),
    );
    let pull_task = b.task(b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("v"), b.ok(b.loc("v"))),
            arm(b.err_pat("d"), on_done),
        ],
    ));
    let wrapped_pull = b.lam(
        &["_u"],
        b.app3(
            b.global("Effect.Errors", "on_exit_primitive"),
            b.bool_(false),
            on_exit,
            pull_task,
        ),
    );
    let built = b.block(
        [b.let_(
            "pull",
            b.app3(b.loc("build"), b.loc("up"), b.loc("own"), b.loc("forked")),
        )],
        wrapped_pull,
    );
    b.lam(
        &["build", "up", "own"],
        b.match_(
            b.app(rt(b, "Helper.ch_fork_scope"), b.loc("own")),
            [arm(pair_pat(b, "forked", "close"), built)],
        ),
    )
}

pub(super) fn ch_callback_helper(b: &ExprBuilder) -> Expr {
    let q_opts = b.at(ExprKind::Record(vec![
        ("capacity".into(), b.field(b.loc("opts"), "bufferSize")),
        ("strategy".into(), b.field(b.loc("opts"), "strategy")),
    ]));
    let shutdown_fin = b.task(b.block(
        [b.let_("_sh", b.app(b.global("Queue", "shutdown"), b.loc("q")))],
        b.unit(),
    ));
    let reg_fin = b.app2(
        rt(b, "Layer.OwnerRun"),
        b.loc("own"),
        b.task(b.app(b.global("Scope", "add_finalizer"), shutdown_fin)),
    );
    let on_cause = b.lam(
        &["c"],
        b.block(
            [b.let_(
                "_fc",
                b.app2(b.global("Queue", "fail_cause"), b.loc("c"), b.loc("q")),
            )],
            b.unit(),
        ),
    );
    let run_cb = b.task(b.block([b.let_("_r", b.app(b.loc("f"), b.loc("q")))], b.unit()));
    let spawn = b.app3(
        b.global("Fiber", "spawn"),
        b.loc("own"),
        b.lam(&["_c"], b.unit()),
        b.task(b.app2(b.global("Effect.Errors", "catch_cause"), on_cause, run_cb)),
    );
    let from_q = b.app4(
        rt(b, "Helper.ch_from_queue"),
        b.loc("arr"),
        b.loc("q"),
        rt(b, "Helper.ch_empty_pull"),
        b.loc("own"),
    );
    b.lam(
        &["arr", "opts", "f", "_up", "own"],
        b.block(
            [
                b.let_("q", b.app(b.global("Queue", "make"), q_opts)),
                Stmt::Expr(reg_fin),
                b.let_("_fib", spawn),
                Stmt::Expr(b.app(rt(b, "Async.EffectYieldNow"), b.unit())),
            ],
            from_q,
        ),
    )
}

pub(super) fn ch_acquire_release_helper(b: &ExprBuilder) -> Expr {
    // (release : a ->{} x, acquire : Eff r e a) -> Channel
    let rel_fin = b.task(b.block([b.let_("_r", b.app(b.loc("rel"), b.loc("v")))], b.unit()));
    let masked = b.lam(
        &["restore"],
        b.block(
            [
                b.let_("v", b.await_(b.app(b.loc("restore"), b.loc("acq")))),
                Stmt::Expr(b.app(b.global("Scope", "add_finalizer"), rel_fin)),
            ],
            b.app(rt(b, "Helper.ch_from_list"), b.list([b.loc("v")])),
        ),
    );
    let body = b.app(
        rt(b, "Helper.ch_unwrap"),
        b.task(b.app(
            b.global("Effect.Concurrency", "uninterruptible_mask"),
            masked,
        )),
    );
    b.lam(&["rel", "acq"], body)
}

pub(super) fn ch_acquire_use_release_helper(b: &ExprBuilder) -> Expr {
    // (acq, use_fn, rel : a -> Exit e d ->{} x) -> Channel
    let exit_val = b.match_(
        b.loc("oc"),
        [
            arm(
                b.some_pat("c"),
                b.app(b.global("Exit", "fail_cause"), b.loc("c")),
            ),
            arm(
                Pat::Wild,
                b.app(b.global("Exit", "succeed"), b.rget(b.loc("ended"))),
            ),
        ],
    );
    let run_rel = b.block(
        [
            b.let_("ex", exit_val),
            b.let_("_r", b.app2(b.loc("rel"), b.loc("a"), b.loc("ex"))),
        ],
        b.unit(),
    );
    let fin = b.lam(
        &["oc"],
        b.match_(
            b.rget(b.loc("val")),
            [arm(b.some_pat("a"), run_rel), arm(Pat::Wild, b.unit())],
        ),
    );
    let reg_fin = b.app2(
        rt(b, "Layer.OwnerRun"),
        b.loc("forked"),
        b.task(b.app(b.global("Scope", "add_finalizer_exit"), fin)),
    );
    let acq_masked = b.app(
        b.global("Effect.Concurrency", "uninterruptible_mask"),
        b.lam(
            &["_r"],
            b.block(
                [
                    b.let_("v", b.await_(b.loc("acq"))),
                    b.rset(b.loc("val"), b.some(b.loc("v"))),
                ],
                b.loc("v"),
            ),
        ),
    );
    let on_done = b.block([b.rset(b.loc("ended"), b.loc("d"))], b.err(b.loc("d")));
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.app(b.loc("pull"), b.unit()),
            [
                arm(b.ok_pat("o"), b.ok(b.loc("o"))),
                arm(b.err_pat("d"), on_done),
            ],
        ),
    );
    let build = b.lam(
        &["up", "own", "forked"],
        b.block(
            [
                b.let_("ended", b.rmake(b.unit())),
                b.let_("val", b.rmake(b.none())),
                Stmt::Expr(reg_fin),
                b.let_("a", acq_masked),
                b.let_(
                    "pull",
                    b.app2(
                        b.app(b.loc("use_fn"), b.loc("a")),
                        b.loc("up"),
                        b.loc("own"),
                    ),
                ),
            ],
            pull,
        ),
    );
    b.lam(
        &["acq", "use_fn", "rel"],
        b.app(rt(b, "Helper.ch_bracket"), build),
    )
}
