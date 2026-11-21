//! `Stream.Transform` effectful and parallel map helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::rt;
use super::super::{Expr, Pat, Stmt};

pub(super) fn st_map_effect_helper(b: &ExprBuilder) -> Expr {
    let wrap = b.lam(&["a"], b.list([b.app(b.loc("f"), b.loc("a"))]));
    let flat = b.app(rt(b, "Helper.ch_flatten_array"), b.loc("self"));
    b.lam(&["f", "self"], b.app2(rt(b, "Helper.ch_map"), wrap, flat))
}

fn sem_op(b: &ExprBuilder, op: &str) -> Expr {
    b.match_(
        b.loc("sem"),
        [
            arm(
                b.some_pat("s"),
                b.app2(b.global("Semaphore", op), b.int(1), b.loc("s")),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    )
}

fn queue_shutdown_finalizer(b: &ExprBuilder, q_loc: &str) -> Stmt {
    let shutdown = b.block(
        [b.let_("_sh", b.app(b.global("Queue", "shutdown"), b.loc(q_loc)))],
        b.unit(),
    );
    Stmt::Expr(b.app2(
        rt(b, "Layer.OwnerRun"),
        b.loc("forked"),
        b.task(b.app(b.global("Scope", "add_finalizer"), b.task(shutdown))),
    ))
}

fn fail_queue_cause(b: &ExprBuilder, q_loc: &str) -> Expr {
    b.lam(
        &["c"],
        b.block(
            [b.let_(
                "_fc",
                b.app2(b.global("Queue", "fail_cause"), b.loc("c"), b.loc(q_loc)),
            )],
            b.unit(),
        ),
    )
}

fn unordered_worker(b: &ExprBuilder) -> Expr {
    let open_done = b.if_(
        b.bin(
            BinOp::And,
            b.bin(BinOp::Eq, b.loc("na"), b.int(0)),
            b.rget(b.loc("udone")),
        ),
        b.block(
            [b.let_("_op", b.app(b.global("Latch", "open"), b.loc("dl")))],
            b.unit(),
        ),
        b.unit(),
    );
    let cleanup = b.task(b.block(
        [
            Stmt::Expr(sem_op(b, "release")),
            b.let_("na", b.sub_i(b.rget(b.loc("act")), b.int(1))),
            b.rset(b.loc("act"), b.loc("na")),
            Stmt::Expr(open_done),
        ],
        b.unit(),
    ));
    let work = b.task(b.block(
        [
            b.let_("v", b.app(b.loc("f"), b.loc("a"))),
            b.let_(
                "_o",
                b.app2(
                    b.global("Queue", "offer"),
                    b.list([b.loc("v")]),
                    b.loc("out_q"),
                ),
            ),
        ],
        b.unit(),
    ));
    let caught = b.task(b.app2(
        b.global("Effect.Errors", "catch_cause"),
        fail_queue_cause(b, "out_q"),
        work,
    ));
    let body = b.task(b.app2(b.global("Effect.Errors", "ensuring"), cleanup, caught));
    b.app3(
        b.global("Fiber", "spawn"),
        b.loc("forked"),
        b.lam(&["_c"], b.unit()),
        body,
    )
}

fn unordered_branch(b: &ExprBuilder) -> Expr {
    let on_elem = b.block(
        [
            Stmt::Expr(sem_op(b, "take")),
            b.rset(b.loc("act"), b.add_i(b.rget(b.loc("act")), b.int(1))),
            b.let_("_wf", unordered_worker(b)),
        ],
        b.unit(),
    );
    let wait_active = b.if_(
        b.bin(BinOp::Gt, b.rget(b.loc("act")), b.int(0)),
        b.app(b.global("Latch", "await_"), b.loc("dl")),
        b.unit(),
    );
    let on_end = b.block(
        [
            b.rset(b.loc("udone"), b.bool_(true)),
            Stmt::Expr(wait_active),
            b.let_("_e", b.app(b.global("Queue", "end"), b.loc("out_q"))),
        ],
        b.break_("mpu", b.unit()),
    );
    let pull_loop = b.loop_ctrl(
        "mpu",
        b.match_(
            b.app(b.loc("pull"), b.unit()),
            [arm(b.ok_pat("a"), on_elem), arm(b.err_pat("_d"), on_end)],
        ),
    );
    let producer = b.app3(
        b.global("Fiber", "spawn"),
        b.loc("forked"),
        b.lam(&["_c"], b.unit()),
        b.task(b.app2(
            b.global("Effect.Errors", "catch_cause"),
            fail_queue_cause(b, "out_q"),
            b.task(pull_loop),
        )),
    );
    let consumer = b.lam(
        &["_u"],
        b.match_(
            b.app(b.global("Queue", "take"), b.loc("out_q")),
            [
                arm(b.some_pat("v"), b.ok(b.loc("v"))),
                arm(Pat::Wild, b.err(b.unit())),
            ],
        ),
    );
    let stmts = [
        b.let_("out_q", b.app(b.global("Queue", "unbounded"), b.unit())),
        queue_shutdown_finalizer(b, "out_q"),
        b.let_("act", b.rmake(b.int(0))),
        b.let_("udone", b.rmake(b.bool_(false))),
        b.let_("dl", b.app(b.global("Latch", "make"), b.bool_(false))),
        b.let_("_pf", producer),
    ];
    b.block(stmts, consumer)
}

fn ordered_worker(b: &ExprBuilder) -> Expr {
    let on_cause = b.lam(
        &["c"],
        b.block(
            [b.let_(
                "_s",
                b.app2(
                    b.global("Deferred", "succeed"),
                    b.err(b.loc("c")),
                    b.loc("def"),
                ),
            )],
            b.unit(),
        ),
    );
    let work = b.task(b.block(
        [
            b.let_("v", b.app(b.loc("f"), b.loc("a"))),
            b.let_(
                "_s",
                b.app2(
                    b.global("Deferred", "succeed"),
                    b.ok(b.loc("v")),
                    b.loc("def"),
                ),
            ),
        ],
        b.unit(),
    ));
    let caught = b.task(b.app2(b.global("Effect.Errors", "catch_cause"), on_cause, work));
    let body = b.task(b.app2(
        b.global("Effect.Errors", "ensuring"),
        b.task(sem_op(b, "release")),
        caught,
    ));
    b.app3(
        b.global("Fiber", "spawn"),
        b.loc("forked"),
        b.lam(&["_c"], b.unit()),
        body,
    )
}

fn ordered_branch(b: &ExprBuilder) -> Expr {
    let on_elem = b.block(
        [
            Stmt::Expr(sem_op(b, "take")),
            b.let_("def", b.app(b.global("Deferred", "make"), b.unit())),
            b.let_(
                "_o",
                b.app2(b.global("Queue", "offer"), b.loc("def"), b.loc("res_q")),
            ),
            b.let_("_wf", ordered_worker(b)),
        ],
        b.unit(),
    );
    let on_end = b.block(
        [b.let_("_e", b.app(b.global("Queue", "end"), b.loc("res_q")))],
        b.break_("mpo", b.unit()),
    );
    let pull_loop = b.loop_ctrl(
        "mpo",
        b.match_(
            b.app(b.loc("pull"), b.unit()),
            [arm(b.ok_pat("a"), on_elem), arm(b.err_pat("_d"), on_end)],
        ),
    );
    let producer = b.app3(
        b.global("Fiber", "spawn"),
        b.loc("forked"),
        b.lam(&["_c"], b.unit()),
        b.task(b.app2(
            b.global("Effect.Errors", "catch_cause"),
            fail_queue_cause(b, "res_q"),
            b.task(pull_loop),
        )),
    );
    let await_def = b.match_(
        b.app(b.global("Deferred", "await_"), b.loc("def")),
        [
            arm(b.ok_pat("v"), b.ok(b.list([b.loc("v")]))),
            arm(
                b.err_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
        ],
    );
    let consumer = b.lam(
        &["_u"],
        b.match_(
            b.app(b.global("Queue", "take"), b.loc("res_q")),
            [
                arm(b.some_pat("def"), await_def),
                arm(Pat::Wild, b.err(b.unit())),
            ],
        ),
    );
    let stmts = [
        b.let_("res_q", b.app(b.global("Queue", "unbounded"), b.unit())),
        queue_shutdown_finalizer(b, "res_q"),
        b.let_("_pf", producer),
    ];
    b.block(stmts, consumer)
}

pub(super) fn st_map_effect_par_helper(b: &ExprBuilder) -> Expr {
    let sem_init = b.match_(
        b.field(b.loc("opts"), "concurrency"),
        [
            arm(
                b.some_pat("n"),
                b.some(b.app(b.global("Semaphore", "make"), b.loc("n"))),
            ),
            arm(Pat::Wild, b.none()),
        ],
    );
    let par_build = b.lam(
        &["up", "own", "forked"],
        b.block(
            [
                b.let_(
                    "pull",
                    b.app3(
                        rt(b, "Helper.ch_flatten_array"),
                        b.loc("self"),
                        b.loc("up"),
                        b.loc("own"),
                    ),
                ),
                b.let_("sem", sem_init),
            ],
            b.if_(
                b.field(b.loc("opts"), "unordered"),
                unordered_branch(b),
                ordered_branch(b),
            ),
        ),
    );
    let run_par = b.app(rt(b, "Helper.ch_bracket"), par_build);
    let seq_or_par = b.if_(
        b.bin(BinOp::Le, b.loc("n"), b.int(1)),
        b.app2(rt(b, "Helper.st_map_effect"), b.loc("f"), b.loc("self")),
        run_par.clone(),
    );
    b.lam(
        &["opts", "f", "self"],
        b.match_(
            b.field(b.loc("opts"), "concurrency"),
            [arm(b.some_pat("n"), seq_or_par), arm(Pat::Wild, run_par)],
        ),
    )
}
