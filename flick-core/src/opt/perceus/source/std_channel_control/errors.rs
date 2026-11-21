//! Error recovery and error-tapping `Channel.Control` IR helpers.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::rt;
use super::super::{Expr, Pat, Stmt};
use super::close_active_owner;

pub(super) fn ch_catch_cause_helper(b: &ExprBuilder) -> Expr {
    // Uses `Effect.Errors.catch_cause` so G7g interruption skipping works automatically!
    let recover = b.lam(
        &["c"],
        b.block(
            [
                Stmt::Expr(close_active_owner(b, b.some(b.loc("c")))),
                b.let_("co1", b.app(rt(b, "Layer.OwnerNew"), b.unit())),
                b.rset(b.loc("aown"), b.some(b.loc("co1"))),
                b.let_(
                    "q",
                    b.app2(b.app(b.loc("f"), b.loc("c")), b.loc("up"), b.loc("co1")),
                ),
                b.rset(b.loc("p2"), b.some(b.loc("q"))),
            ],
            b.app(b.loc("q"), b.unit()),
        ),
    );
    let pull_first = b.app2(
        b.global("Effect.Errors", "catch_cause"),
        recover,
        b.task(b.app(b.loc("p1"), b.unit())),
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
    let reg_fin = b.app2(
        rt(b, "Layer.OwnerRun"),
        b.loc("own"),
        b.task(b.app(
            b.global("Scope", "add_finalizer_exit"),
            b.lam(&["oc"], close_active_owner(b, b.loc("oc"))),
        )),
    );
    b.lam(
        &["f", "self", "up", "own"],
        b.block(
            [
                b.let_("co0", b.app(rt(b, "Layer.OwnerNew"), b.unit())),
                b.let_("aown", b.rmake(b.some(b.loc("co0")))),
                Stmt::Expr(reg_fin),
                b.let_("p1", b.app2(b.loc("self"), b.loc("up"), b.loc("co0"))),
                b.let_("p2", b.rmake(b.none())),
            ],
            pull,
        ),
    )
}

pub(super) fn ch_catch_helper(b: &ExprBuilder) -> Expr {
    let rethrow_ch = b.lam(
        &["_up", "_own"],
        b.lam(&["_u"], b.app(b.global("Failure", "rethrow"), b.loc("c"))),
    );
    let handler = b.lam(
        &["c"],
        b.match_(
            b.app(b.global("Cause", "find_error_option"), b.loc("c")),
            [
                arm(b.some_pat("e"), b.app(b.loc("f"), b.loc("e"))),
                arm(Pat::Wild, rethrow_ch),
            ],
        ),
    );
    b.lam(
        &["f", "self"],
        b.app2(rt(b, "Helper.ch_catch_cause"), handler, b.loc("self")),
    )
}

pub(super) fn ch_tap_error_helper(b: &ExprBuilder) -> Expr {
    let run_tap = b.match_(
        b.app(b.global("Cause", "find_error_option"), b.loc("c")),
        [
            arm(
                b.some_pat("e"),
                b.block([b.let_("_x", b.app(b.loc("f"), b.loc("e")))], b.unit()),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let on_cause = b.lam(
        &["c"],
        b.block(
            [Stmt::Expr(run_tap)],
            b.app(b.global("Failure", "rethrow"), b.loc("c")),
        ),
    );
    let pull = b.lam(
        &["_u"],
        b.app2(
            b.global("Effect.Errors", "catch_cause"),
            on_cause,
            b.task(b.app(b.loc("pull"), b.unit())),
        ),
    );
    b.lam(
        &["f", "self", "up", "own"],
        b.block(
            [b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own")))],
            pull,
        ),
    )
}

pub(super) fn ch_tap_cause_helper(b: &ExprBuilder) -> Expr {
    let rethrow_body = b.block(
        [b.let_("_x", b.app(b.loc("f"), b.loc("c")))],
        b.app(b.global("Failure", "rethrow"), b.loc("c")),
    );
    let handler = b.lam(&["c", "_up", "_own"], b.lam(&["_u"], rethrow_body));
    b.lam(
        &["f", "self"],
        b.app2(rt(b, "Helper.ch_catch_cause"), handler, b.loc("self")),
    )
}
