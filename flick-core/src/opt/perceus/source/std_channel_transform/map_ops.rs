//! Map, tap, `flatMap`, `switchMap`, and flatten `Channel.Transform` IR helpers.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::rt;
use super::super::{Expr, ExprKind, Pat, Stmt};
use super::close_active_owner;

pub(super) fn ch_map_helper(b: &ExprBuilder) -> Expr {
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.app(b.loc("pull"), b.unit()),
            [
                arm(b.ok_pat("v"), b.ok(b.app(b.loc("f"), b.loc("v")))),
                arm(b.err_pat("d"), b.err(b.loc("d"))),
            ],
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

pub(super) fn ch_tap_helper(b: &ExprBuilder) -> Expr {
    let on_ok = b.block(
        [b.let_("_x", b.app(b.loc("f"), b.loc("v")))],
        b.ok(b.loc("v")),
    );
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.app(b.loc("pull"), b.unit()),
            [
                arm(b.ok_pat("v"), on_ok),
                arm(b.err_pat("d"), b.err(b.loc("d"))),
            ],
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

pub(super) fn ch_map_done_helper(b: &ExprBuilder) -> Expr {
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.app(b.loc("pull"), b.unit()),
            [
                arm(b.ok_pat("v"), b.ok(b.loc("v"))),
                arm(b.err_pat("d"), b.err(b.app(b.loc("f"), b.loc("d")))),
            ],
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

pub(super) fn ch_map_input_helper(b: &ExprBuilder) -> Expr {
    let mapped_up = b.lam(
        &["_u"],
        b.match_(
            b.app(b.loc("up"), b.unit()),
            [
                arm(b.ok_pat("v"), b.ok(b.app(b.loc("f"), b.loc("v")))),
                arm(b.err_pat("d"), b.err(b.loc("d"))),
            ],
        ),
    );
    b.lam(
        &["f", "self", "up", "own"],
        b.app2(b.loc("self"), mapped_up, b.loc("own")),
    )
}

pub(super) fn ch_map_input_error_helper(b: &ExprBuilder) -> Expr {
    let on_cause = b.lam(
        &["c"],
        b.match_(
            b.app(b.global("Cause", "find_error_option"), b.loc("c")),
            [
                arm(
                    b.some_pat("e"),
                    b.app(b.global("Fail", "fail"), b.app(b.loc("f"), b.loc("e"))),
                ),
                arm(Pat::Wild, b.app(b.global("Failure", "rethrow"), b.loc("c"))),
            ],
        ),
    );
    let mapped_up = b.lam(
        &["_u"],
        b.app2(
            b.global("Effect.Errors", "catch_cause"),
            on_cause,
            b.task(b.app(b.loc("up"), b.unit())),
        ),
    );
    b.lam(
        &["f", "self", "up", "own"],
        b.app2(b.loc("self"), mapped_up, b.loc("own")),
    )
}

pub(super) fn ch_flat_map_helper(b: &ExprBuilder) -> Expr {
    // O(1) parent finalizer: single `active_own : Ref (Option Owner)` closed when each inner ends or parent closes!
    let inner_done = b.block(
        [
            b.rset(b.loc("cur"), b.none()),
            Stmt::Expr(close_active_owner(b, b.none())),
        ],
        b.unit(),
    );
    let pull_inner = b.match_(
        b.app(b.loc("cp"), b.unit()),
        [
            arm(b.ok_pat("v"), b.break_("fm", b.ok(b.loc("v")))),
            arm(b.err_pat("_d"), inner_done),
        ],
    );
    let start_inner = b.block(
        [
            b.let_("co", b.app(rt(b, "Layer.OwnerNew"), b.unit())),
            b.rset(b.loc("aown"), b.some(b.loc("co"))),
            b.let_("ich", b.app(b.loc("f"), b.loc("o"))),
            b.let_("ip", b.app2(b.loc("ich"), b.loc("up"), b.loc("co"))),
            b.rset(b.loc("cur"), b.some(b.loc("ip"))),
        ],
        b.unit(),
    );
    let pull_outer = b.match_(
        b.app(b.loc("outer"), b.unit()),
        [
            arm(b.ok_pat("o"), start_inner),
            arm(b.err_pat("d"), b.break_("fm", b.err(b.loc("d")))),
        ],
    );
    let step = b.match_(
        b.rget(b.loc("cur")),
        [
            arm(b.some_pat("cp"), pull_inner),
            arm(Pat::Wild, pull_outer),
        ],
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
                b.let_("outer", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
                b.let_("cur", b.rmake(b.none())),
                b.let_("aown", b.rmake(b.none())),
                Stmt::Expr(reg_fin),
            ],
            b.lam(&["_u"], b.loop_ctrl("fm", step)),
        ),
    )
}

pub(super) fn ch_switch_map_helper(b: &ExprBuilder) -> Expr {
    let opts = b.at(ExprKind::Record(vec![
        ("concurrency".into(), b.some(b.int(1))),
        ("bufferSize".into(), b.none()),
        ("switch".into(), b.bool_(true)),
    ]));
    let mapped = b.app2(rt(b, "Helper.ch_map"), b.loc("f"), b.loc("self"));
    b.lam(
        &["f", "self"],
        b.app2(rt(b, "Helper.ch_merge_all"), opts, mapped),
    )
}

pub(super) fn ch_flatten_array_helper(b: &ExprBuilder) -> Expr {
    let cons_pat = Pat::List(
        vec![Pat::Bind(b.name("h"))],
        Some(Box::new(Pat::Bind(b.name("tl")))),
    );
    let emit_head = b.block(
        [b.rset(b.loc("buf"), b.loc("tl"))],
        b.break_("fa", b.ok(b.loc("h"))),
    );
    let refill = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(
                b.ok_pat("xs"),
                b.block([b.rset(b.loc("buf"), b.loc("xs"))], b.unit()),
            ),
            arm(b.err_pat("d"), b.break_("fa", b.err(b.loc("d")))),
        ],
    );
    let step = b.match_(
        b.rget(b.loc("buf")),
        [arm(cons_pat, emit_head), arm(Pat::Wild, refill)],
    );
    b.lam(
        &["self", "up", "own"],
        b.block(
            [
                b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
                b.let_("buf", b.rmake(b.list(vec![]))),
            ],
            b.lam(&["_u"], b.loop_ctrl("fa", step)),
        ),
    )
}
