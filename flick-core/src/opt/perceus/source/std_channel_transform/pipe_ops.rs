//! Concatenation, drain, forever, and piping `Channel.Transform` IR helpers.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat, Stmt};
use super::close_active_owner;

pub(super) fn ch_concat_with_helper(b: &ExprBuilder) -> Expr {
    let start_second = b.block(
        [
            Stmt::Expr(b.app(b.loc("close"), b.none())),
            b.let_("ch2", b.app(b.loc("f"), b.loc("d"))),
            b.let_("q", b.app2(b.loc("ch2"), b.loc("up"), b.loc("own"))),
            b.rset(b.loc("p2"), b.some(b.loc("q"))),
        ],
        b.app(b.loc("q"), b.unit()),
    );
    let pull_first = b.match_(
        b.app(b.loc("p1"), b.unit()),
        [
            arm(b.ok_pat("v"), b.ok(b.loc("v"))),
            arm(b.err_pat("d"), start_second),
        ],
    );
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.rget(b.loc("p2")),
            [
                arm(b.some_pat("q"), b.app(b.loc("q"), b.unit())),
                arm(Pat::Wild, pull_first),
            ],
        ),
    );
    let body = b.block(
        [
            b.let_("p1", b.app2(b.loc("self"), b.loc("up"), b.loc("forked"))),
            b.let_("p2", b.rmake(b.none())),
        ],
        pull,
    );
    b.lam(
        &["f", "self", "up", "own"],
        b.match_(
            b.app(rt(b, "Helper.ch_fork_scope"), b.loc("own")),
            [arm(pair_pat(b, "forked", "close"), body)],
        ),
    )
}

pub(super) fn ch_or_else_if_empty_helper(b: &ExprBuilder) -> Expr {
    let on_ok = b.block([b.rset(b.loc("emitted"), b.bool_(true))], b.ok(b.loc("v")));
    let start_fallback = b.block(
        [
            b.let_(
                "q",
                b.app2(b.app(b.loc("f"), b.loc("d")), b.loc("up"), b.loc("own")),
            ),
            b.rset(b.loc("p2"), b.some(b.loc("q"))),
        ],
        b.app(b.loc("q"), b.unit()),
    );
    let on_err = b.if_(b.rget(b.loc("emitted")), b.err(b.loc("d")), start_fallback);
    let pull_first = b.match_(
        b.app(b.loc("p1"), b.unit()),
        [arm(b.ok_pat("v"), on_ok), arm(b.err_pat("d"), on_err)],
    );
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.rget(b.loc("p2")),
            [
                arm(b.some_pat("q"), b.app(b.loc("q"), b.unit())),
                arm(Pat::Wild, pull_first),
            ],
        ),
    );
    b.lam(
        &["f", "self", "up", "own"],
        b.block(
            [
                b.let_("p1", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
                b.let_("emitted", b.rmake(b.bool_(false))),
                b.let_("p2", b.rmake(b.none())),
            ],
            pull,
        ),
    )
}

pub(super) fn ch_drain_helper(b: &ExprBuilder) -> Expr {
    let step = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("_v"), b.unit()),
            arm(b.err_pat("d"), b.break_("dr", b.err(b.loc("d")))),
        ],
    );
    b.lam(
        &["self", "up", "own"],
        b.block(
            [b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own")))],
            b.lam(&["_u"], b.loop_ctrl("dr", step)),
        ),
    )
}

pub(super) fn ch_forever_helper(b: &ExprBuilder) -> Expr {
    let on_iter_done = b.block(
        [
            b.rset(b.loc("cur"), b.none()),
            Stmt::Expr(close_active_owner(b, b.none())),
        ],
        b.unit(),
    );
    let pull_cur = b.match_(
        b.app(b.loc("p"), b.unit()),
        [
            arm(b.ok_pat("v"), b.break_("fv", b.ok(b.loc("v")))),
            arm(b.err_pat("_d"), on_iter_done),
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
        &["self", "up", "own"],
        b.block(
            [
                b.let_("cur", b.rmake(b.none())),
                b.let_("aown", b.rmake(b.none())),
                Stmt::Expr(reg_fin),
            ],
            b.lam(&["_u"], b.loop_ctrl("fv", step)),
        ),
    )
}

pub(super) fn ch_pipe_to_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["that", "self", "up", "own"],
        b.app2(
            b.loc("that"),
            b.app2(b.loc("self"), b.loc("up"), b.loc("own")),
            b.loc("own"),
        ),
    )
}

pub(super) fn ch_pipe_to_or_fail_helper(b: &ExprBuilder) -> Expr {
    let record_cause = b.lam(
        &["c"],
        b.block(
            [b.rset(b.loc("up_err"), b.some(b.loc("c")))],
            b.app(b.global("Failure", "rethrow"), b.loc("c")),
        ),
    );
    let up2 = b.lam(
        &["_u"],
        b.app2(
            b.global("Effect.Errors", "catch_cause"),
            record_cause,
            b.task(b.app(b.loc("up1"), b.unit())),
        ),
    );
    let check_up = b.match_(
        b.rget(b.loc("up_err")),
        [
            arm(
                b.some_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
            arm(Pat::Wild, b.loc("res")),
        ],
    );
    let pull = b.lam(
        &["_u"],
        b.block([b.let_("res", b.app(b.loc("down"), b.unit()))], check_up),
    );
    b.lam(
        &["that", "self", "up", "own"],
        b.block(
            [
                b.let_("up_err", b.rmake(b.none())),
                b.let_("up1", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
                b.let_("up2", up2),
                b.let_("down", b.app2(b.loc("that"), b.loc("up2"), b.loc("own"))),
            ],
            pull,
        ),
    )
}
