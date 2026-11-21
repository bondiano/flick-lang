//! Bounded-concurrency and switching `Channel.mergeAll` IR helper.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::rt;
use super::super::{Expr, Pat, Stmt};

fn dec_active_expr(b: &ExprBuilder) -> Expr {
    let signal_idle = b.if_(
        b.bin(BinOp::Eq, b.loc("rem"), b.int(0)),
        b.block([b.dsucceed(b.unit(), b.rget(b.loc("idle_d")))], b.unit()),
        b.unit(),
    );
    b.block(
        [
            b.let_("rem", b.sub_i(b.rget(b.loc("active")), b.int(1))),
            b.rset(b.loc("active"), b.loc("rem")),
            b.let_("wd", b.rget(b.loc("room_d"))),
            b.rset(b.loc("room_d"), b.dmake()),
            b.dsucceed(b.unit(), b.loc("wd")),
            Stmt::Expr(signal_idle),
        ],
        b.unit(),
    )
}

fn child_on_exit(b: &ExprBuilder) -> Expr {
    let keep_other = b.lam(
        &["e"],
        b.match_(
            b.loc("e"),
            [arm(
                b.tup_pat(&["eid", "_", "_"]),
                b.not(b.bin(BinOp::Eq, b.loc("eid"), b.loc("cid"))),
            )],
        ),
    );
    let filtered = b.app2(
        b.global("List.Core", "filter"),
        keep_other,
        b.rget(b.loc("fibs")),
    );
    let fail_q = b.block(
        [b.let_(
            "_f",
            b.app2(b.global("Queue", "fail_cause"), b.loc("c"), b.loc("q")),
        )],
        b.unit(),
    );
    let propagate_fail = b.match_(
        b.loc("fc"),
        [arm(b.some_pat("c"), fail_q), arm(Pat::Wild, b.unit())],
    );
    let mark_done = b.block(
        [
            b.rset(b.loc("done_f"), b.bool_(true)),
            Stmt::Expr(propagate_fail),
        ],
        dec_active_expr(b),
    );
    let body = b.block(
        [
            b.let_("fc", b.app(b.global("Exit", "failure"), b.loc("ex"))),
            Stmt::Expr(b.app2(rt(b, "Layer.OwnerClose"), b.loc("cown"), b.loc("fc"))),
            b.rset(b.loc("fibs"), filtered),
        ],
        b.if_(b.rget(b.loc("done_f")), b.unit(), mark_done),
    );
    b.lam(&["ex"], b.some(b.task(body)))
}

fn child_pull_task(b: &ExprBuilder) -> Expr {
    let yield_and_offer = b.block(
        [
            Stmt::Expr(b.app(rt(b, "Async.EffectYieldNow"), b.unit())),
            b.let_(
                "_o",
                b.app2(b.global("Queue", "offer"), b.loc("v"), b.loc("q")),
            ),
        ],
        b.unit(),
    );
    let pull_loop = b.loop_ctrl(
        "mac",
        b.match_(
            b.app(b.loc("cp"), b.unit()),
            [
                arm(b.ok_pat("v"), yield_and_offer),
                arm(b.err_pat("_d"), b.break_("mac", b.unit())),
            ],
        ),
    );
    b.task(b.block(
        [b.let_("cp", b.app2(b.loc("ich"), b.loc("up"), b.loc("cown")))],
        pull_loop,
    ))
}

fn spawn_child_expr(b: &ExprBuilder) -> Expr {
    let reset_idle = b.if_(
        b.bin(BinOp::Eq, b.loc("active0"), b.int(0)),
        b.block([b.rset(b.loc("idle_d"), b.dmake())], b.unit()),
        b.unit(),
    );
    let child_body = b.task(b.app3(
        b.global("Effect.Errors", "on_exit_primitive"),
        b.bool_(false),
        child_on_exit(b),
        child_pull_task(b),
    ));
    let fib_expr = b.app3(
        b.global("Fiber", "spawn"),
        b.loc("forked"),
        b.lam(&["_c"], b.unit()),
        child_body,
    );
    let entry = b.tuple([b.loc("cid"), b.loc("fib"), b.loc("done_f")]);
    let record_fib = b.block(
        [b.rset(
            b.loc("fibs"),
            b.app2(
                b.global("List.Core", "append"),
                b.rget(b.loc("fibs")),
                b.list([entry]),
            ),
        )],
        b.unit(),
    );
    let track_fib = b.if_(b.not(b.rget(b.loc("done_f"))), record_fib, b.unit());
    b.block(
        [
            b.let_("cown", b.app(rt(b, "Layer.OwnerNew"), b.unit())),
            b.let_("cid", b.rget(b.loc("next_id"))),
            b.rset(b.loc("next_id"), b.add_i(b.loc("cid"), b.int(1))),
            b.let_("done_f", b.rmake(b.bool_(false))),
            b.let_("active0", b.rget(b.loc("active"))),
            b.rset(b.loc("active"), b.add_i(b.loc("active0"), b.int(1))),
            Stmt::Expr(reset_idle),
            b.let_("fib", fib_expr),
            Stmt::Expr(track_fib),
        ],
        b.unit(),
    )
}

fn evict_for_switch(b: &ExprBuilder) -> Expr {
    let dec_if_open = b.if_(
        b.not(b.rget(b.loc("odone"))),
        b.block([b.rset(b.loc("odone"), b.bool_(true))], dec_active_expr(b)),
        b.unit(),
    );
    let pop_pat = Pat::List(
        vec![b.tup_pat(&["_oid", "ofib", "odone"])],
        Some(Box::new(Pat::Bind(b.name("frest")))),
    );
    let evict_head = b.block(
        [
            b.rset(b.loc("fibs"), b.loc("frest")),
            Stmt::Expr(dec_if_open),
            Stmt::Expr(b.app(b.global("Fiber", "interrupt"), b.loc("ofib"))),
        ],
        b.unit(),
    );
    let step = b.match_(
        b.rget(b.loc("fibs")),
        [arm(pop_pat, evict_head), arm(Pat::Wild, b.unit())],
    );
    let at_cap = b.bin(BinOp::Ge, b.rget(b.loc("active")), b.loc("conc"));
    b.if_(b.loc("sw"), b.while_(at_cap, step), b.unit())
}

fn driver_fiber(b: &ExprBuilder) -> Expr {
    let wait_room = b.if_(
        b.not(b.loc("sw")),
        b.while_(
            b.bin(BinOp::Ge, b.rget(b.loc("active")), b.loc("conc")),
            b.dawait(b.rget(b.loc("room_d"))),
        ),
        b.unit(),
    );
    let on_child = b.block([Stmt::Expr(evict_for_switch(b))], spawn_child_expr(b));
    let on_outer_done = b.block(
        [
            Stmt::Expr(b.while_(
                b.bin(BinOp::Gt, b.rget(b.loc("active")), b.int(0)),
                b.dawait(b.rget(b.loc("idle_d"))),
            )),
            b.rset(b.loc("dref"), b.some(b.loc("d"))),
            b.let_("_e", b.app(b.global("Queue", "end"), b.loc("q"))),
        ],
        b.break_("mad", b.unit()),
    );
    let pull_outer = b.match_(
        b.app(b.loc("outer"), b.unit()),
        [
            arm(b.ok_pat("ich"), on_child),
            arm(b.err_pat("d"), on_outer_done),
        ],
    );
    let drv_loop = b.loop_ctrl("mad", b.block([Stmt::Expr(wait_room)], pull_outer));
    let on_cause = b.lam(
        &["c"],
        b.block(
            [b.let_(
                "_f",
                b.app2(b.global("Queue", "fail_cause"), b.loc("c"), b.loc("q")),
            )],
            b.unit(),
        ),
    );
    let body = b.task(b.app2(
        b.global("Effect.Errors", "catch_cause"),
        on_cause,
        b.task(drv_loop),
    ));
    b.app3(
        b.global("Fiber", "spawn"),
        b.loc("forked"),
        b.lam(&["_c"], b.unit()),
        body,
    )
}

fn opt_pos_int(b: &ExprBuilder, field: &str, default_val: i32) -> Expr {
    let clamp = b.if_(b.bin(BinOp::Lt, b.loc("n"), b.int(1)), b.int(1), b.loc("n"));
    b.match_(
        b.field(b.loc("opts"), field),
        [
            arm(b.some_pat("n"), clamp),
            arm(Pat::Wild, b.int(default_val)),
        ],
    )
}

pub(super) fn ch_merge_all_helper(b: &ExprBuilder) -> Expr {
    // (opts: { concurrency : Option Int; bufferSize : Option Int; switch : Bool }, channels: Channel) -> Channel
    let done_idle = b.block(
        [b.let_("d0", b.dmake()), b.dsucceed(b.unit(), b.loc("d0"))],
        b.loc("d0"),
    );
    let cleanup_fin = b.task(b.block(
        [
            b.rset(b.loc("fibs"), b.list(vec![])),
            b.let_("_sh", b.app(b.global("Queue", "shutdown"), b.loc("q"))),
        ],
        b.unit(),
    ));
    let reg_fin = b.app2(
        rt(b, "Layer.OwnerRun"),
        b.loc("forked"),
        b.task(b.app(b.global("Scope", "add_finalizer"), cleanup_fin)),
    );
    let on_end = b.match_(
        b.rget(b.loc("dref")),
        [
            arm(b.some_pat("d"), b.err(b.loc("d"))),
            arm(Pat::Wild, b.err(b.unit())),
        ],
    );
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.app(b.global("Queue", "take"), b.loc("q")),
            [
                arm(b.some_pat("v"), b.ok(b.loc("v"))),
                arm(Pat::Wild, on_end),
            ],
        ),
    );
    let build = b.lam(
        &["up", "own", "forked"],
        b.block(
            [
                b.let_("conc", opt_pos_int(b, "concurrency", i32::MAX)),
                b.let_("bsz", opt_pos_int(b, "bufferSize", 16)),
                b.let_("sw", b.field(b.loc("opts"), "switch")),
                b.let_("q", b.app(b.global("Queue", "bounded"), b.loc("bsz"))),
                b.let_("dref", b.rmake(b.none())),
                b.let_("active", b.rmake(b.int(0))),
                b.let_("next_id", b.rmake(b.int(0))),
                b.let_("fibs", b.rmake(b.list(vec![]))),
                b.let_("room_d", b.rmake(b.dmake())),
                b.let_("idle_d", b.rmake(done_idle)),
                Stmt::Expr(reg_fin),
                b.let_(
                    "outer",
                    b.app2(b.loc("channels"), b.loc("up"), b.loc("own")),
                ),
                b.let_("_drv", driver_fiber(b)),
            ],
            pull,
        ),
    );
    b.lam(
        &["opts", "channels"],
        b.app(rt(b, "Helper.ch_bracket"), build),
    )
}
