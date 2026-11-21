//! `Stream.Pipe` execution-plan attempt instrumentation helper.

use flick_ir::{Arith, BinOp, Num};

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_sched_plan::PLAN_META_KEY;
use super::super::std_schema_rt::rt;
use super::super::{Expr, ExprKind, Pat, Stmt};

fn plan_emit(b: &ExprBuilder, ev: Expr) -> Stmt {
    let call = b.task(b.block([b.let_("_r", b.app(b.loc("on_ev"), ev))], b.unit()));
    b.let_(
        "_em",
        b.app2(
            b.global("Effect.Errors", "catch_cause"),
            b.lam(&["_c"], b.unit()),
            call,
        ),
    )
}

fn plan_meta_ref(b: &ExprBuilder) -> Expr {
    let default_meta = b.at(ExprKind::Record(vec![
        ("attempt".into(), b.int(0)),
        ("stepIndex".into(), b.int(0)),
    ]));
    b.app2(
        rt(b, "Layer.ContextReference"),
        b.str_(PLAN_META_KEY),
        b.lam(&["_u"], default_meta),
    )
}

fn plan_dur_from_ms(b: &ExprBuilder) -> Expr {
    let diff = b.bin(
        BinOp::Arith(Arith::Sub, Num::Float),
        b.app(rt(b, "Host.CurrentTimeMillis"), b.unit()),
        b.rget(b.loc("t0_r")),
    );
    b.match_(
        b.app(rt(b, "Duration.Millis"), diff),
        [
            arm(b.ok_pat("d"), b.loc("d")),
            arm(Pat::Wild, b.global("Duration", "zero")),
        ],
    )
}

fn plan_with_start(b: &ExprBuilder) -> Expr {
    let ev_start = b.at(ExprKind::Con(ExprBuilder::con(
        "ExecutionPlan",
        "Event",
        "AttemptStart",
        1,
    )));
    let reset_step_att = b.if_(
        b.not(b.bin(BinOp::Eq, b.loc("idx"), b.rget(b.loc("lidx_r")))),
        b.block(
            [
                b.rset(b.loc("lidx_r"), b.loc("idx")),
                b.rset(b.loc("satt_r"), b.int(0)),
            ],
            b.unit(),
        ),
        b.unit(),
    );
    let info_rec = b.at(ExprKind::Record(vec![
        ("attempt".into(), b.loc("att")),
        ("stepAttempt".into(), b.loc("sa")),
        ("stepIndex".into(), b.loc("idx")),
    ]));
    let meta_rec = b.at(ExprKind::Record(vec![
        ("attempt".into(), b.loc("att")),
        ("stepIndex".into(), b.loc("idx")),
    ]));
    let open_p = b.app3(
        rt(b, "Layer.ContextWithReference"),
        plan_meta_ref(b),
        b.loc("meta"),
        b.task(b.app2(b.loc("with_lay"), b.loc("up"), b.loc("own"))),
    );
    let pull_p = b.lam(
        &["_u"],
        b.app3(
            rt(b, "Layer.ContextWithReference"),
            plan_meta_ref(b),
            b.loc("meta"),
            b.task(b.app(b.loc("p"), b.unit())),
        ),
    );
    let wrapped = b.lam(&["up", "own"], b.block([b.let_("p", open_p)], pull_p));
    let stmts = [
        b.let_("att", b.add_i(b.rget(b.loc("att_r")), b.int(1))),
        b.rset(b.loc("att_r"), b.loc("att")),
        Stmt::Expr(reset_step_att),
        b.let_("sa", b.add_i(b.rget(b.loc("satt_r")), b.int(1))),
        b.rset(b.loc("satt_r"), b.loc("sa")),
        b.let_("info", info_rec),
        b.rset(b.loc("info_r"), b.some(b.loc("info"))),
        b.rset(
            b.loc("t0_r"),
            b.app(rt(b, "Host.CurrentTimeMillis"), b.unit()),
        ),
        plan_emit(b, b.app(ev_start, b.loc("info"))),
        b.let_("meta", meta_rec),
    ];
    b.app(rt(b, "Helper.ch_unwrap"), b.task(b.block(stmts, wrapped)))
}

pub(super) fn plan_instrument(b: &ExprBuilder) -> Expr {
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
    let on_fail = b.block(
        [plan_emit(
            b,
            b.app3(ev_fail, b.loc("info"), b.loc("took"), b.loc("c")),
        )],
        b.unit(),
    );
    let on_ok = b.block(
        [plan_emit(b, b.app2(ev_ok, b.loc("info"), b.loc("took")))],
        b.unit(),
    );
    let emit_exit = b.block(
        [
            b.rset(b.loc("info_r"), b.none()),
            b.let_("took", plan_dur_from_ms(b)),
        ],
        b.match_(
            b.app(b.global("Exit", "failure"), b.loc("ex")),
            [arm(b.some_pat("c"), on_fail), arm(Pat::Wild, on_ok)],
        ),
    );
    let on_exit = b.lam(
        &["ex"],
        b.match_(
            b.rget(b.loc("info_r")),
            [arm(b.some_pat("info"), emit_exit), arm(Pat::Wild, b.unit())],
        ),
    );
    let stmts = [
        b.let_(
            "with_lay",
            b.app2(rt(b, "Helper.ch_provide"), b.loc("lay"), b.loc("self")),
        ),
        b.let_("with_start", plan_with_start(b)),
    ];
    b.lam(
        &["idx", "lay"],
        b.block(
            stmts,
            b.app2(rt(b, "Helper.ch_on_exit"), on_exit, b.loc("with_start")),
        ),
    )
}
