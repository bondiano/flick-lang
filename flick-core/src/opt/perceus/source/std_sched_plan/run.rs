//! `Effect.with_execution_plan` step execution loop and event emission.

use flick_ir::{Arith, BinOp, Num};

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, ExprKind, Pat, Stmt};
use super::{build_retry_schedule, plan_meta_ref, step_pat};

fn emit_event(b: &ExprBuilder, ev: Expr) -> Stmt {
    b.let_(
        "_em",
        b.app(
            b.global("Effect.Core", "exit"),
            b.task(b.app(b.loc("on_ev"), ev)),
        ),
    )
}

fn masked_attempt_body(b: &ExprBuilder) -> Expr {
    let ev_start = b.at(ExprKind::Con(ExprBuilder::con(
        "ExecutionPlan",
        "Event",
        "AttemptStart",
        1,
    )));
    let ev_ok = b.at(ExprKind::Con(ExprBuilder::con(
        "ExecutionPlan",
        "Event",
        "AttemptSuccess",
        2,
    )));
    let ev_fail = b.at(ExprKind::Con(ExprBuilder::con(
        "ExecutionPlan",
        "Event",
        "AttemptFailure",
        3,
    )));
    let reset_step = b.if_(
        b.not(b.bin(BinOp::Eq, b.loc("idx"), b.rget(b.loc("last_idx")))),
        b.block(
            [
                b.rset(b.loc("last_idx"), b.loc("idx")),
                b.rset(b.loc("step_att"), b.int(0)),
            ],
            b.unit(),
        ),
        b.unit(),
    );
    let info = b.at(ExprKind::Record(vec![
        ("attempt".into(), b.loc("att")),
        ("stepAttempt".into(), b.loc("sa")),
        ("stepIndex".into(), b.loc("idx")),
    ]));
    let run_layer = b.task(b.app2(
        b.global("Layer", "run"),
        b.loc("lay"),
        b.lam(&["_u"], b.await_(b.loc("body"))),
    ));
    let elapsed_ms = b.bin(
        BinOp::Arith(Arith::Sub, Num::Float),
        b.app(b.global("Clock", "current_time_millis"), b.unit()),
        b.loc("t0"),
    );
    let dur_from_ms = b.match_(
        b.app(b.global("Duration", "millis"), elapsed_ms),
        [
            arm(b.ok_pat("d"), b.loc("d")),
            arm(Pat::Wild, b.global("Duration", "zero")),
        ],
    );
    let on_cause = b.block(
        [emit_event(
            b,
            b.app3(ev_fail, b.loc("info"), b.loc("took"), b.loc("c")),
        )],
        b.app(b.global("Failure", "rethrow"), b.loc("c")),
    );
    let on_fail = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("ex")),
        [
            arm(b.some_pat("c"), on_cause),
            arm(
                Pat::Wild,
                b.app(b.global("Effect.Core", "die"), b.str_("plan exit")),
            ),
        ],
    );
    let on_ok = b.block(
        [emit_event(b, b.app2(ev_ok, b.loc("info"), b.loc("took")))],
        b.loc("v"),
    );
    let finish = b.match_(
        b.app(b.global("Exit", "value"), b.loc("ex")),
        [arm(b.some_pat("v"), on_ok), arm(Pat::Wild, on_fail)],
    );
    let stmts = [
        Stmt::Expr(reset_step),
        b.let_("sa", b.add_i(b.rget(b.loc("step_att")), b.int(1))),
        b.rset(b.loc("step_att"), b.loc("sa")),
        b.let_("info", info),
        emit_event(b, b.app(ev_start, b.loc("info"))),
        b.let_(
            "t0",
            b.app(b.global("Clock", "current_time_millis"), b.unit()),
        ),
        b.let_(
            "ex",
            b.app(
                b.global("Effect.Core", "exit"),
                b.app(b.loc("restore"), run_layer),
            ),
        ),
        b.let_("took", dur_from_ms),
    ];
    b.lam(&["restore"], b.block(stmts, finish))
}

fn first_step_run(b: &ExprBuilder) -> Expr {
    let no_sched = b.match_(
        b.loc("osch"),
        [
            arm(b.some_pat("_s"), b.bool_(false)),
            arm(Pat::Wild, b.bool_(true)),
        ],
    );
    let no_retry = b.bin(
        BinOp::Or,
        b.bin(BinOp::Eq, b.loc("atts"), b.int(1)),
        b.bin(
            BinOp::And,
            b.bin(BinOp::Eq, b.loc("atts"), b.int(0)),
            no_sched,
        ),
    );
    let t1 = b.if_(
        b.bin(BinOp::Gt, b.loc("atts"), b.int(0)),
        b.sub_i(b.loc("atts"), b.int(1)),
        b.int(-1),
    );
    let retry_body = b.block(
        [
            b.let_("t1", t1),
            b.let_(
                "sch",
                build_retry_schedule(b, b.loc("osch"), b.loc("owh"), b.loc("t1")),
            ),
        ],
        b.app2(
            b.global("Effect.Core", "retry"),
            b.loc("sch"),
            b.loc("next_t"),
        ),
    );
    b.block(
        [b.let_("no_retry", no_retry)],
        b.if_(b.loc("no_retry"), b.await_(b.loc("next_t")), retry_body),
    )
}

fn next_step_run(b: &ExprBuilder) -> Expr {
    let replay_t = b.task(b.if_(
        b.rget(b.loc("tried")),
        b.await_(b.loc("next_t")),
        b.block(
            [b.rset(b.loc("tried"), b.bool_(true))],
            b.app(b.global("Fail", "fail"), b.var("prev_err")),
        ),
    ));
    let default_sched = b.if_(
        b.bin(BinOp::Eq, b.loc("atts"), b.int(0)),
        b.some(b.app(b.global("Schedule", "recurs"), b.int(1))),
        b.none(),
    );
    let osch2 = b.match_(
        b.loc("osch"),
        [
            arm(b.some_pat("s"), b.some(b.loc("s"))),
            arm(Pat::Wild, default_sched),
        ],
    );
    let t2 = b.if_(
        b.bin(BinOp::Eq, b.loc("atts"), b.int(0)),
        b.int(-1),
        b.loc("atts"),
    );
    b.block(
        [
            b.let_("tried", b.rmake(b.bool_(false))),
            b.let_("replay_t", replay_t),
            b.let_("osch2", osch2),
            b.let_("t2", t2),
            b.let_(
                "sch",
                build_retry_schedule(b, b.loc("osch2"), b.loc("owh"), b.loc("t2")),
            ),
        ],
        b.app2(
            b.global("Effect.Core", "retry"),
            b.loc("sch"),
            b.loc("replay_t"),
        ),
    )
}

fn run_step_expr(b: &ExprBuilder) -> Expr {
    let meta = b.at(ExprKind::Record(vec![
        ("attempt".into(), b.loc("att")),
        ("stepIndex".into(), b.loc("idx")),
    ]));
    let attempt_task = b.task(b.block(
        [
            b.let_("att", b.add_i(b.rget(b.loc("att_ref")), b.int(1))),
            b.rset(b.loc("att_ref"), b.loc("att")),
            b.let_("meta", meta),
        ],
        b.app3(
            b.global("Context", "with_reference"),
            plan_meta_ref(b),
            b.loc("meta"),
            b.task(b.app(
                b.global("Effect.Concurrency", "uninterruptible_mask"),
                masked_attempt_body(b),
            )),
        ),
    ));
    let record_err = b.block(
        [
            b.assign("v_failed", b.bool_(true)),
            b.assign("prev_err", b.loc("err")),
        ],
        b.unit(),
    );
    let on_cause = b.match_(
        b.app(b.global("Cause", "find_error_option"), b.loc("c")),
        [
            arm(b.some_pat("err"), record_err),
            arm(Pat::Wild, b.app(b.global("Failure", "rethrow"), b.loc("c"))),
        ],
    );
    let on_fail = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("step_ex")),
        [arm(b.some_pat("c"), on_cause), arm(Pat::Wild, b.unit())],
    );
    let on_ok = b.block(
        [
            b.assign("v_failed", b.bool_(false)),
            b.assign("v_out", b.some(b.loc("val"))),
        ],
        b.break_("ep_lp", b.unit()),
    );
    let step_task = b.task(b.if_(
        b.bin(BinOp::Eq, b.loc("idx"), b.int(0)),
        first_step_run(b),
        next_step_run(b),
    ));
    b.block(
        [
            b.let_("idx", b.var("v_idx")),
            b.let_("next_t", attempt_task),
            b.let_("step_ex", b.app(b.global("Effect.Core", "exit"), step_task)),
            b.assign("v_idx", b.add_i(b.loc("idx"), b.int(1))),
        ],
        b.match_(
            b.app(b.global("Exit", "value"), b.loc("step_ex")),
            [arm(b.some_pat("val"), on_ok), arm(Pat::Wild, on_fail)],
        ),
    )
}

pub(super) fn with_execution_plan_template(b: &ExprBuilder) -> Expr {
    let loop_steps = b.for_ctrl(
        "ep_lp",
        "st",
        b.loc("plan"),
        b.match_(b.loc("st"), [arm(step_pat(b), run_step_expr(b))]),
    );
    let unwrap_out = b.match_(
        b.var("v_out"),
        [
            arm(b.some_pat("v"), b.loc("v")),
            arm(
                Pat::Wild,
                b.app(
                    b.global("Effect.Core", "die"),
                    b.str_("empty execution plan"),
                ),
            ),
        ],
    );
    let finish = b.if_(
        b.var("v_failed"),
        b.app(b.global("Fail", "fail"), b.var("prev_err")),
        unwrap_out,
    );
    b.lam(
        &["plan", "on_ev", "body"],
        b.block(
            [
                b.let_("att_ref", b.rmake(b.int(0))),
                b.let_("last_idx", b.rmake(b.int(-1))),
                b.let_("step_att", b.rmake(b.int(0))),
                b.var_decl("v_idx", b.int(0)),
                b.var_decl("v_failed", b.bool_(false)),
                b.var_decl("prev_err", b.unit()),
                b.var_decl("v_out", b.none()),
                b.let_("_lp", loop_steps),
            ],
            finish,
        ),
    )
}
