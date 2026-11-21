//! `Stream.Pipe` `tapSink` and schedule-preserving retry helpers.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat, Stmt};
use super::run_sink_on_pull;

fn tap_sink_up(b: &ExprBuilder) -> Expr {
    let close_ul = b.if_(
        b.not(b.rget(b.loc("stdone"))),
        b.block(
            [b.let_("_c", b.app(b.global("Latch", "close"), b.loc("ul")))],
            b.unit(),
        ),
        b.unit(),
    );
    let on_chunk = b.block(
        [
            Stmt::Expr(close_ul),
            b.let_("_o", b.app(b.global("Latch", "open"), b.loc("sl"))),
        ],
        b.ok(b.loc("xs")),
    );
    let stmts = [
        Stmt::Expr(b.app(b.global("Latch", "await_"), b.loc("ul"))),
        b.let_("cur", b.rget(b.loc("ch_ref"))),
        b.rset(b.loc("ch_ref"), b.none()),
    ];
    let body = b.match_(
        b.loc("cur"),
        [
            arm(b.some_pat("xs"), on_chunk),
            arm(Pat::Wild, b.err(b.unit())),
        ],
    );
    b.lam(&["_u"], b.block(stmts, body))
}

fn tap_sink_fiber(b: &ExprBuilder) -> Expr {
    let run_sink = b.block(
        [b.let_(
            "_e",
            run_sink_on_pull(b, b.loc("sink"), b.loc("sink_up"), b.loc("forked")),
        )],
        b.none(),
    );
    let caught = b.app2(
        b.global("Effect.Errors", "catch_cause"),
        b.lam(&["c"], b.some(b.loc("c"))),
        b.task(run_sink),
    );
    let body = b.block(
        [
            b.let_("oc", caught),
            b.rset(b.loc("sdone"), b.bool_(true)),
            b.rset(b.loc("scause"), b.loc("oc")),
            b.let_("_o", b.app(b.global("Latch", "open"), b.loc("sl"))),
        ],
        b.unit(),
    );
    b.app3(
        b.global("Fiber", "spawn"),
        b.loc("forked"),
        b.lam(&["_c"], b.unit()),
        b.task(body),
    )
}

fn tap_pull_chunk(b: &ExprBuilder) -> Expr {
    let handoff = b.block(
        [
            b.rset(b.loc("ch_ref"), b.some(b.loc("xs"))),
            b.let_("_c", b.app(b.global("Latch", "close"), b.loc("sl"))),
            b.let_("_o", b.app(b.global("Latch", "open"), b.loc("ul"))),
            Stmt::Expr(b.app(b.global("Latch", "await_"), b.loc("sl"))),
        ],
        b.ok(b.loc("xs")),
    );
    let if_active = b.if_(b.rget(b.loc("sdone")), b.ok(b.loc("xs")), handoff);
    b.match_(
        b.rget(b.loc("scause")),
        [
            arm(
                b.some_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
            arm(Pat::Wild, if_active),
        ],
    )
}

fn tap_pull_end(b: &ExprBuilder) -> Expr {
    let wake_sink = b.block(
        [
            b.let_("_c", b.app(b.global("Latch", "close"), b.loc("sl"))),
            b.let_("_o", b.app(b.global("Latch", "open"), b.loc("ul"))),
            Stmt::Expr(b.app(b.global("Latch", "await_"), b.loc("sl"))),
        ],
        b.unit(),
    );
    let finish = b.match_(
        b.rget(b.loc("scause")),
        [
            arm(
                b.some_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
            arm(Pat::Wild, b.err(b.loc("d"))),
        ],
    );
    let stmts = [
        b.rset(b.loc("stdone"), b.bool_(true)),
        Stmt::Expr(b.if_(b.not(b.rget(b.loc("sdone"))), wake_sink, b.unit())),
    ];
    b.block(stmts, finish)
}

pub(super) fn st_tap_sink_helper(b: &ExprBuilder) -> Expr {
    let pull_active = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("xs"), tap_pull_chunk(b)),
            arm(b.err_pat("d"), tap_pull_end(b)),
        ],
    );
    let pull_checked = b.if_(
        b.rget(b.loc("sdone")),
        b.app(b.loc("pull"), b.unit()),
        pull_active,
    );
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.rget(b.loc("scause")),
            [
                arm(
                    b.some_pat("c"),
                    b.app(b.global("Failure", "rethrow"), b.loc("c")),
                ),
                arm(Pat::Wild, pull_checked),
            ],
        ),
    );
    let stmts = [
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("ul", b.app(b.global("Latch", "make"), b.bool_(false))),
        b.let_("sl", b.app(b.global("Latch", "make"), b.bool_(false))),
        b.let_("ch_ref", b.rmake(b.none())),
        b.let_("sdone", b.rmake(b.bool_(false))),
        b.let_("scause", b.rmake(b.none())),
        b.let_("stdone", b.rmake(b.bool_(false))),
        b.let_("sink_up", tap_sink_up(b)),
        b.let_("_sf", tap_sink_fiber(b)),
    ];
    let build = b.lam(&["up", "own", "forked"], b.block(stmts, pull));
    b.lam(&["sink", "self"], b.app(rt(b, "Helper.ch_bracket"), build))
}

pub(super) fn st_retry_no_reset_helper(b: &ExprBuilder) -> Expr {
    // (sched, self) -> Stream: retries without resetting schedule stepper
    let retry_next = b.block(
        [
            Stmt::Expr(b.app(rt(b, "Async.EffectYieldNow"), b.unit())),
            b.rset(
                b.loc("cur"),
                b.app2(b.loc("self"), b.loc("up"), b.loc("own")),
            ),
        ],
        b.unit(),
    );
    let check_sched = b.match_(
        b.app2(rt(b, "Helper.ch_sched_step"), b.loc("step"), b.loc("err")),
        [arm(
            pair_pat(b, "_o", "dn"),
            b.if_(
                b.loc("dn"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
                retry_next,
            ),
        )],
    );
    let on_cause = b.lam(
        &["c"],
        b.match_(
            b.app(b.global("Cause", "find_error_option"), b.loc("c")),
            [
                arm(b.some_pat("err"), check_sched),
                arm(Pat::Wild, b.app(b.global("Failure", "rethrow"), b.loc("c"))),
            ],
        ),
    );
    let attempt = b.task(b.break_("srnr", b.app(b.rget(b.loc("cur")), b.unit())));
    let pull = b.lam(
        &["_u"],
        b.loop_ctrl(
            "srnr",
            b.app2(b.global("Effect.Errors", "catch_cause"), on_cause, attempt),
        ),
    );
    let init = [
        b.let_("step", b.app(b.loc("sched"), b.unit())),
        b.let_(
            "cur",
            b.rmake(b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        ),
    ];
    b.lam(&["sched", "self", "up", "own"], b.block(init, pull))
}
