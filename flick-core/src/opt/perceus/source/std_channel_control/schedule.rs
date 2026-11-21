//! Schedule-driven `repeat`, `retry`, and `schedule` `Channel.Control` IR helpers.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat, Stmt};
use super::close_active_owner;

pub(super) fn ch_repeat_helper(b: &ExprBuilder) -> Expr {
    let check_sched = b.match_(
        b.app2(rt(b, "Helper.ch_sched_step"), b.loc("step"), b.loc("d")),
        [arm(
            pair_pat(b, "_out", "done"),
            b.if_(b.loc("done"), b.break_("rp", b.err(b.loc("d"))), b.unit()),
        )],
    );
    let on_iter_done = b.block(
        [
            b.rset(b.loc("cur"), b.none()),
            Stmt::Expr(close_active_owner(b, b.none())),
        ],
        check_sched,
    );
    let pull_cur = b.match_(
        b.app(b.loc("p"), b.unit()),
        [
            arm(b.ok_pat("v"), b.break_("rp", b.ok(b.loc("v")))),
            arm(b.err_pat("d"), on_iter_done),
        ],
    );
    let start_iter = b.block(
        [
            b.let_("co", b.app(rt(b, "Layer.OwnerNew"), b.unit())),
            b.rset(b.loc("aown"), b.some(b.loc("co"))),
            b.rset(
                b.loc("cur"),
                b.some(b.app2(b.loc("self"), b.loc("up"), b.loc("co"))),
            ),
        ],
        b.unit(),
    );
    let step = b.match_(
        b.rget(b.loc("cur")),
        [arm(b.some_pat("p"), pull_cur), arm(Pat::Wild, start_iter)],
    );
    let reg_fin = b.app2(
        rt(b, "Layer.OwnerRun"),
        b.loc("own"),
        b.task(b.app(
            b.global("Scope", "add_finalizer_exit"),
            b.lam(&["oc"], close_active_owner(b, b.loc("oc"))),
        )),
    );
    b.lam(
        &["sched", "self", "up", "own"],
        b.block(
            [
                b.let_("step", b.app(b.loc("sched"), b.unit())),
                b.let_("cur", b.rmake(b.none())),
                b.let_("aown", b.rmake(b.none())),
                Stmt::Expr(reg_fin),
            ],
            b.lam(&["_u"], b.loop_ctrl("rp", step)),
        ),
    )
}

fn retry_on_err(b: &ExprBuilder) -> Expr {
    let init_step = b.block(
        [
            b.let_("s1", b.app(b.loc("sched"), b.unit())),
            b.rset(b.loc("step_r"), b.some(b.loc("s1"))),
        ],
        b.loc("s1"),
    );
    let get_step = b.match_(
        b.rget(b.loc("step_r")),
        [
            arm(b.some_pat("s0"), b.loc("s0")),
            arm(Pat::Wild, init_step),
        ],
    );
    let eval_step = b.match_(
        b.app2(rt(b, "Helper.ch_sched_step"), b.loc("sf"), b.loc("e")),
        [arm(
            pair_pat(b, "_out", "done"),
            b.if_(
                b.loc("done"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
                b.none(),
            ),
        )],
    );
    let on_typed_err = b.block([b.let_("sf", get_step)], eval_step);
    let dispatch_err = b.match_(
        b.app(b.global("Cause", "find_error_option"), b.loc("c")),
        [
            arm(b.some_pat("e"), on_typed_err),
            arm(Pat::Wild, b.app(b.global("Failure", "rethrow"), b.loc("c"))),
        ],
    );
    b.lam(
        &["c"],
        b.block(
            [
                b.rset(b.loc("cur"), b.none()),
                Stmt::Expr(close_active_owner(b, b.some(b.loc("c")))),
            ],
            dispatch_err,
        ),
    )
}

pub(super) fn ch_retry_helper(b: &ExprBuilder) -> Expr {
    let start_pull = b.block(
        [
            b.let_("co", b.app(rt(b, "Layer.OwnerNew"), b.unit())),
            b.rset(b.loc("aown"), b.some(b.loc("co"))),
            b.let_("p1", b.app2(b.loc("self"), b.loc("up"), b.loc("co"))),
            b.rset(b.loc("cur"), b.some(b.loc("p1"))),
        ],
        b.loc("p1"),
    );
    let get_pull = b.match_(
        b.rget(b.loc("cur")),
        [
            arm(b.some_pat("p0"), b.loc("p0")),
            arm(Pat::Wild, start_pull),
        ],
    );
    let on_res = b.match_(
        b.loc("res"),
        [
            arm(
                b.ok_pat("v"),
                b.block(
                    [b.rset(b.loc("step_r"), b.none())],
                    b.break_("rt", b.ok(b.loc("v"))),
                ),
            ),
            arm(b.err_pat("d"), b.break_("rt", b.err(b.loc("d")))),
        ],
    );
    let attempt = b.app2(
        b.global("Effect.Errors", "catch_cause"),
        retry_on_err(b),
        b.task(b.some(b.app(b.loc("p"), b.unit()))),
    );
    let step = b.block(
        [b.let_("p", get_pull)],
        b.match_(
            attempt,
            [arm(b.some_pat("res"), on_res), arm(Pat::Wild, b.unit())],
        ),
    );
    let reg_fin = b.app2(
        rt(b, "Layer.OwnerRun"),
        b.loc("own"),
        b.task(b.app(
            b.global("Scope", "add_finalizer_exit"),
            b.lam(&["oc"], close_active_owner(b, b.loc("oc"))),
        )),
    );
    b.lam(
        &["sched", "self", "up", "own"],
        b.block(
            [
                b.let_("cur", b.rmake(b.none())),
                b.let_("aown", b.rmake(b.none())),
                b.let_("step_r", b.rmake(b.none())),
                Stmt::Expr(reg_fin),
            ],
            b.lam(&["_u"], b.loop_ctrl("rt", step)),
        ),
    )
}

pub(super) fn ch_schedule_helper(b: &ExprBuilder) -> Expr {
    let on_ok = b.match_(
        b.app2(rt(b, "Helper.ch_sched_step"), b.loc("step"), b.loc("o")),
        [arm(
            pair_pat(b, "out", "done"),
            b.if_(b.loc("done"), b.err(b.loc("out")), b.ok(b.loc("o"))),
        )],
    );
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.app(b.loc("pull"), b.unit()),
            [
                arm(b.ok_pat("o"), on_ok),
                arm(b.err_pat("d"), b.err(b.loc("d"))),
            ],
        ),
    );
    b.lam(
        &["sched", "self", "up", "own"],
        b.block(
            [
                b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
                b.let_("step", b.app(b.loc("sched"), b.unit())),
            ],
            pull,
        ),
    )
}
