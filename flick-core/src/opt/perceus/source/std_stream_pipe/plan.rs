//! `Stream.Pipe` execution-plan step orchestration helper.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_sched_plan::{build_retry_schedule, step_pat};
use super::super::std_schema_rt::rt;
use super::super::{Expr, Pat};
use super::plan_inst::plan_instrument;

fn plan_retry_from_prev(b: &ExprBuilder) -> Expr {
    let fail_prev = b.block(
        [b.rset(b.loc("tried"), b.bool_(true))],
        b.lam(&["_u"], b.app(b.global("Fail", "fail"), b.loc("pe"))),
    );
    let rep = b.lam(
        &["up", "own"],
        b.if_(
            b.rget(b.loc("tried")),
            b.app2(b.loc("base"), b.loc("up"), b.loc("own")),
            fail_prev,
        ),
    );
    let default_sch = b.if_(
        b.bin(BinOp::Eq, b.loc("atts"), b.int(0)),
        b.some(b.app(b.global("Schedule", "recurs"), b.int(1))),
        b.none(),
    );
    let osch2 = b.match_(
        b.loc("osch"),
        [
            arm(b.some_pat("s"), b.some(b.loc("s"))),
            arm(Pat::Wild, default_sch),
        ],
    );
    let t2 = b.if_(
        b.bin(BinOp::Eq, b.loc("atts"), b.int(0)),
        b.int(-1),
        b.loc("atts"),
    );
    let stmts = [
        b.let_("tried", b.rmake(b.bool_(false))),
        b.let_("rep", rep),
        b.let_("osch2", osch2),
        b.let_("t2", t2),
        b.let_(
            "sch",
            build_retry_schedule(b, b.loc("osch2"), b.loc("owh"), b.loc("t2")),
        ),
    ];
    b.block(
        stmts,
        b.app2(
            rt(b, "Helper.st_retry_no_reset"),
            b.loc("sch"),
            b.loc("rep"),
        ),
    )
}

fn plan_retry_fresh(b: &ExprBuilder) -> Expr {
    let no_sched = b.match_(
        b.loc("osch"),
        [
            arm(b.some_pat("_s"), b.bool_(false)),
            arm(Pat::Wild, b.bool_(true)),
        ],
    );
    let single_shot = b.bin(
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
    let with_retry = b.block(
        [
            b.let_("t1", t1),
            b.let_(
                "sch",
                build_retry_schedule(b, b.loc("osch"), b.loc("owh"), b.loc("t1")),
            ),
        ],
        b.app2(
            rt(b, "Helper.st_retry_no_reset"),
            b.loc("sch"),
            b.loc("base"),
        ),
    );
    b.if_(single_shot, b.loc("base"), with_retry)
}

fn plan_run_step(b: &ExprBuilder) -> Expr {
    let fail_stream = b.lam(
        &["_up", "_own"],
        b.lam(&["_u"], b.app(b.global("Fail", "fail"), b.loc("e"))),
    );
    let empty_plan_die = b.lam(
        &["_up", "_own"],
        b.lam(
            &["_u"],
            b.app(
                b.global("Effect.Core", "die"),
                b.str_("empty execution plan"),
            ),
        ),
    );
    let out_of_steps = b.match_(
        b.loc("prev_opt"),
        [
            arm(b.some_pat("e"), fail_stream),
            arm(Pat::Wild, empty_plan_die),
        ],
    );
    let with_ret = b.match_(
        b.loc("prev_opt"),
        [
            arm(b.some_pat("pe"), plan_retry_from_prev(b)),
            arm(Pat::Wild, plan_retry_fresh(b)),
        ],
    );
    let mark_recv = b.lam(
        &["_x"],
        b.block([b.rset(b.loc("recv"), b.bool_(true))], b.unit()),
    );
    let tracked = b.if_(
        b.loc("prev_fb"),
        b.app2(rt(b, "Helper.ch_on_first"), mark_recv, b.loc("with_ret")),
        b.loc("with_ret"),
    );
    let rethrow_err = b.lam(
        &["_up", "_own"],
        b.lam(&["_u"], b.app(b.global("Fail", "fail"), b.loc("err"))),
    );
    let next_step = b.app2(
        b.rget(b.loc("go_r")),
        b.add_i(b.loc("idx"), b.int(1)),
        b.some(b.loc("err")),
    );
    let on_err = b.lam(
        &["err"],
        b.if_(
            b.bin(BinOp::And, b.loc("prev_fb"), b.rget(b.loc("recv"))),
            rethrow_err,
            next_step,
        ),
    );
    let step_body = b.block(
        [
            b.let_("base", b.app2(b.loc("inst"), b.loc("idx"), b.loc("lay"))),
            b.let_("with_ret", with_ret),
            b.let_("recv", b.rmake(b.bool_(false))),
            b.let_("tracked", tracked),
        ],
        b.app2(rt(b, "Helper.ch_catch"), on_err, b.loc("tracked")),
    );
    let run_curr = b.match_(
        b.app2(rt(b, "Helper.list_at"), b.loc("idx"), b.loc("plan")),
        [arm(step_pat(b), step_body)],
    );
    let at_end = b.bin(
        BinOp::Ge,
        b.loc("idx"),
        b.app(b.global("List.Core", "length"), b.loc("plan")),
    );
    b.lam(&["idx", "prev_opt"], b.if_(at_end, out_of_steps, run_curr))
}

pub(super) fn st_with_execution_plan_helper(b: &ExprBuilder) -> Expr {
    let stub = b.lam(
        &["_i", "_p"],
        b.lam(&["_u", "_o"], rt(b, "Helper.ch_empty_pull")),
    );
    let stmts = [
        b.let_("att_r", b.rmake(b.int(0))),
        b.let_("lidx_r", b.rmake(b.int(-1))),
        b.let_("satt_r", b.rmake(b.int(0))),
        b.let_("info_r", b.rmake(b.none())),
        b.let_("t0_r", b.rmake(b.float(0.0))),
        b.let_("inst", plan_instrument(b)),
        b.let_("go_r", b.rmake(stub)),
        b.rset(b.loc("go_r"), plan_run_step(b)),
    ];
    let start = b.app2(
        b.app2(b.rget(b.loc("go_r")), b.int(0), b.none()),
        b.loc("up"),
        b.loc("own"),
    );
    b.lam(
        &["plan", "prev_fb", "on_ev", "self", "up", "own"],
        b.block(stmts, start),
    )
}
