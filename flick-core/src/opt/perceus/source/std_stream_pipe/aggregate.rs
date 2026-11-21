//! `Stream.Pipe` scheduled aggregation (`aggregateWithin`) helper.

use flick_ir::{Arith, BinOp, Num};

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat, Stmt};
use super::run_sink_on_pull;

fn agg_producer(b: &ExprBuilder) -> Expr {
    let on_cause = b.lam(
        &["c"],
        b.block(
            [
                b.rset(b.loc("q_done"), b.bool_(true)),
                b.rset(b.loc("q_cause"), b.some(b.loc("c"))),
                b.let_(
                    "_fc",
                    b.app2(b.global("Queue", "fail_cause"), b.loc("c"), b.loc("buf_q")),
                ),
            ],
            b.unit(),
        ),
    );
    let on_chunk = b.block(
        [
            b.let_("_cl", b.app(b.global("Latch", "close"), b.loc("pl"))),
            b.let_(
                "_o",
                b.app2(
                    b.global("Queue", "offer"),
                    b.ok(b.loc("xs")),
                    b.loc("buf_q"),
                ),
            ),
        ],
        b.unit(),
    );
    let on_end = b.block(
        [
            b.rset(b.loc("q_done"), b.bool_(true)),
            b.let_("_e", b.app(b.global("Queue", "end"), b.loc("buf_q"))),
        ],
        b.break_("saw", b.unit()),
    );
    let loop_body = b.block(
        [Stmt::Expr(b.app(b.global("Latch", "await_"), b.loc("pl")))],
        b.match_(
            b.app(b.loc("pull"), b.unit()),
            [arm(b.ok_pat("xs"), on_chunk), arm(b.err_pat("_d"), on_end)],
        ),
    );
    let caught = b.app2(
        b.global("Effect.Errors", "catch_cause"),
        on_cause,
        b.task(b.loop_ctrl("saw", loop_body)),
    );
    b.app3(
        b.global("Fiber", "spawn"),
        b.loc("forked"),
        b.lam(&["_c"], b.unit()),
        b.task(caught),
    )
}

fn agg_sink_up(b: &ExprBuilder) -> Expr {
    let open_pl = b.if_(
        b.not(b.rget(b.loc("q_done"))),
        b.block(
            [b.let_("_o", b.app(b.global("Latch", "open"), b.loc("pl")))],
            b.unit(),
        ),
        b.unit(),
    );
    let unwrap_item = b.match_(
        b.loc("item"),
        [
            arm(b.ok_pat("xs"), b.app(b.loc("on_in"), b.loc("xs"))),
            arm(b.err_pat("_step"), b.err(b.unit())),
        ],
    );
    let pull_queue = b.block(
        [Stmt::Expr(open_pl)],
        b.match_(
            b.app(b.global("Queue", "take"), b.loc("buf_q")),
            [
                arm(b.some_pat("item"), unwrap_item),
                arm(Pat::Wild, b.err(b.unit())),
            ],
        ),
    );
    let take_left = b.block(
        [b.rset(b.loc("left"), b.none())],
        b.app(b.loc("on_in"), b.loc("xs")),
    );
    b.lam(
        &["_u"],
        b.match_(
            b.rget(b.loc("left")),
            [arm(b.some_pat("xs"), take_left), arm(Pat::Wild, pull_queue)],
        ),
    )
}

fn agg_step_task(b: &ExprBuilder) -> Expr {
    let stop_sched = b.block(
        [
            b.rset(b.loc("q_done"), b.bool_(true)),
            b.let_("_e", b.app(b.global("Queue", "end"), b.loc("buf_q"))),
        ],
        b.unit(),
    );
    let sleep_if_pos = b.if_(
        b.app(rt(b, "Duration.IsPositive"), b.loc("del")),
        b.app(rt(b, "Async.EffectSleep"), b.loc("del")),
        b.unit(),
    );
    let emit_tick = b.block(
        [
            Stmt::Expr(sleep_if_pos),
            b.let_(
                "_o",
                b.app2(b.global("Queue", "offer"), b.err(b.unit()), b.loc("buf_q")),
            ),
        ],
        b.unit(),
    );
    let step_res = b.match_(
        b.app2(
            b.loc("step"),
            b.rget(b.loc("sch_t")),
            b.rget(b.loc("last_o")),
        ),
        [arm(
            b.tup_pat(&["_o", "del", "dn"]),
            b.if_(b.loc("dn"), stop_sched, emit_tick),
        )],
    );
    let stmts = [
        Stmt::Expr(b.app(b.global("Latch", "await_"), b.loc("has_in"))),
        Stmt::Expr(step_res),
    ];
    b.task(b.block(stmts, b.app(rt(b, "Async.EffectNever"), b.unit())))
}

fn agg_run_task(b: &ExprBuilder) -> Expr {
    let elapsed = b.bin(
        BinOp::Arith(Arith::Sub, Num::Float),
        b.app(rt(b, "Host.CurrentTimeMillis"), b.unit()),
        b.rget(b.loc("open_at")),
    );
    let next_t = b.bin(
        BinOp::Arith(Arith::Add, Num::Float),
        b.rget(b.loc("sch_t")),
        elapsed,
    );
    let with_input = b.block(
        [
            b.rset(b.loc("sch_t"), next_t),
            b.rset(b.loc("last_o"), b.some(b.loc("v"))),
            b.rset(b.loc("left"), b.loc("lo")),
        ],
        b.ok(b.list([b.loc("v")])),
    );
    let without_input = b.if_(
        b.rget(b.loc("q_done")),
        b.err(b.unit()),
        b.block(
            [
                b.rset(b.loc("last_o"), b.some(b.loc("v"))),
                b.rset(b.loc("left"), b.loc("lo")),
            ],
            b.ok(b.list([b.loc("v")])),
        ),
    );
    let on_result = b.if_(
        b.app(b.global("Latch", "is_open"), b.loc("has_in")),
        with_input,
        without_input,
    );
    let run_sink = run_sink_on_pull(b, b.loc("sink"), b.loc("sink_up"), b.loc("forked"));
    b.task(b.match_(run_sink, [arm(pair_pat(b, "v", "lo"), on_result)]))
}

fn agg_consumer(b: &ExprBuilder) -> Expr {
    let no_left = b.match_(
        b.rget(b.loc("left")),
        [
            arm(b.some_pat("_"), b.bool_(false)),
            arm(Pat::Wild, b.bool_(true)),
        ],
    );
    let finish = b.match_(
        b.rget(b.loc("q_cause")),
        [
            arm(
                b.some_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
            arm(Pat::Wild, b.err(b.unit())),
        ],
    );
    let race_or_run = b.if_(
        b.not(b.rget(b.loc("q_done"))),
        b.app2(rt(b, "Async.RaceFirst"), b.loc("run_t"), b.loc("step_t")),
        b.await_(b.loc("run_t")),
    );
    let pull_step = b.block(
        [b.let_("_cl", b.app(b.global("Latch", "close"), b.loc("has_in")))],
        race_or_run,
    );
    b.lam(
        &["_u"],
        b.if_(
            b.bin(BinOp::And, b.rget(b.loc("q_done")), no_left),
            finish,
            pull_step,
        ),
    )
}

pub(super) fn st_aggregate_within_helper(b: &ExprBuilder) -> Expr {
    let shutdown = b.block(
        [b.let_("_sh", b.app(b.global("Queue", "shutdown"), b.loc("buf_q")))],
        b.unit(),
    );
    let reg_shutdown = b.app2(
        rt(b, "Layer.OwnerRun"),
        b.loc("forked"),
        b.task(b.app(b.global("Scope", "add_finalizer"), b.task(shutdown))),
    );
    let open_mark = b.if_(
        b.app(b.global("Latch", "open"), b.loc("has_in")),
        b.block(
            [b.rset(
                b.loc("open_at"),
                b.app(rt(b, "Host.CurrentTimeMillis"), b.unit()),
            )],
            b.unit(),
        ),
        b.unit(),
    );
    let on_in = b.lam(&["xs"], b.block([Stmt::Expr(open_mark)], b.ok(b.loc("xs"))));
    let stmts = [
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("pl", b.app(b.global("Latch", "make"), b.bool_(false))),
        b.let_("buf_q", b.app(b.global("Queue", "unbounded"), b.unit())),
        b.let_("q_done", b.rmake(b.bool_(false))),
        b.let_("q_cause", b.rmake(b.none())),
        Stmt::Expr(reg_shutdown),
        b.let_("_pf", agg_producer(b)),
        b.let_("step", b.app(b.loc("sched"), b.unit())),
        b.let_("has_in", b.app(b.global("Latch", "make"), b.bool_(false))),
        b.let_(
            "sch_t",
            b.rmake(b.app(rt(b, "Host.CurrentTimeMillis"), b.unit())),
        ),
        b.let_("open_at", b.rmake(b.float(0.0))),
        b.let_("last_o", b.rmake(b.none())),
        b.let_("left", b.rmake(b.none())),
        b.let_("on_in", on_in),
        b.let_("sink_up", agg_sink_up(b)),
        b.let_("step_t", agg_step_task(b)),
        b.let_("run_t", agg_run_task(b)),
    ];
    let build = b.lam(&["up", "own", "forked"], b.block(stmts, agg_consumer(b)));
    b.lam(
        &["sink", "sched", "self"],
        b.app(rt(b, "Helper.ch_bracket"), build),
    )
}
