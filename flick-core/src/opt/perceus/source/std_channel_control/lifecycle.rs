//! Lifecycle hooks and interruption `Channel.Control` IR helpers.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{rt, str_lit};
use super::super::{Expr, Pat, Stmt};

pub(super) fn ch_on_exit_helper(b: &ExprBuilder) -> Expr {
    let exit_val = b.match_(
        b.loc("oc"),
        [
            arm(
                b.some_pat("c"),
                b.app(b.global("Exit", "fail_cause"), b.loc("c")),
            ),
            arm(
                Pat::Wild,
                b.app(b.global("Exit", "succeed"), b.rget(b.loc("ended"))),
            ),
        ],
    );
    let fin = b.lam(
        &["oc"],
        b.block(
            [
                b.let_("ex", exit_val),
                b.let_("_r", b.app(b.loc("f"), b.loc("ex"))),
            ],
            b.unit(),
        ),
    );
    let reg_fin = b.app2(
        rt(b, "Layer.OwnerRun"),
        b.loc("forked"),
        b.task(b.app(b.global("Scope", "add_finalizer_exit"), fin)),
    );
    let on_done = b.block([b.rset(b.loc("ended"), b.loc("d"))], b.err(b.loc("d")));
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.app(b.loc("pull"), b.unit()),
            [
                arm(b.ok_pat("o"), b.ok(b.loc("o"))),
                arm(b.err_pat("d"), on_done),
            ],
        ),
    );
    let build = b.lam(
        &["up", "own", "forked"],
        b.block(
            [
                b.let_("ended", b.rmake(b.unit())),
                Stmt::Expr(reg_fin),
                b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
            ],
            pull,
        ),
    );
    b.lam(&["f", "self"], b.app(rt(b, "Helper.ch_bracket"), build))
}

pub(super) fn ch_on_first_helper(b: &ExprBuilder) -> Expr {
    let run_first = b.block(
        [
            b.rset(b.loc("first"), b.bool_(false)),
            b.let_("_x", b.app(b.loc("f"), b.loc("o"))),
        ],
        b.unit(),
    );
    let on_ok = b.block(
        [Stmt::Expr(b.if_(
            b.rget(b.loc("first")),
            run_first,
            b.unit(),
        ))],
        b.ok(b.loc("o")),
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
        &["f", "self", "up", "own"],
        b.block(
            [
                b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
                b.let_("first", b.rmake(b.bool_(true))),
            ],
            pull,
        ),
    )
}

pub(super) fn ch_on_end_helper(b: &ExprBuilder) -> Expr {
    let on_done = b.block([b.let_("_x", b.await_(b.loc("t")))], b.err(b.loc("d")));
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.app(b.loc("pull"), b.unit()),
            [
                arm(b.ok_pat("o"), b.ok(b.loc("o"))),
                arm(b.err_pat("d"), on_done),
            ],
        ),
    );
    b.lam(
        &["t", "self", "up", "own"],
        b.block(
            [b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own")))],
            pull,
        ),
    )
}

pub(super) fn ch_interrupt_when_helper(b: &ExprBuilder) -> Expr {
    let right_ch = b.lam(
        &["_up", "_own"],
        b.lam(&["_u"], b.err(b.await_(b.loc("t")))),
    );
    b.lam(
        &["t", "self"],
        b.app3(
            rt(b, "Helper.ch_merge"),
            str_lit(b, "either"),
            right_ch,
            b.loc("self"),
        ),
    )
}

pub(super) fn ch_halt_when_helper(b: &ExprBuilder) -> Expr {
    let on_cause = b.lam(
        &["c"],
        b.block(
            [b.rset(b.loc("done_r"), b.some(b.err(b.loc("c"))))],
            b.unit(),
        ),
    );
    let await_halt = b.task(b.block(
        [
            b.let_("v", b.await_(b.loc("t"))),
            b.rset(b.loc("done_r"), b.some(b.ok(b.loc("v")))),
        ],
        b.unit(),
    ));
    let spawn = b.app3(
        b.global("Fiber", "spawn"),
        b.loc("forked"),
        b.lam(&["_c"], b.unit()),
        b.task(b.app2(
            b.global("Effect.Errors", "catch_cause"),
            on_cause,
            await_halt,
        )),
    );
    let on_res = b.match_(
        b.loc("res"),
        [
            arm(b.ok_pat("dv"), b.err(b.loc("dv"))),
            arm(
                b.err_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
        ],
    );
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.rget(b.loc("done_r")),
            [
                arm(b.some_pat("res"), on_res),
                arm(Pat::Wild, b.app(b.loc("pull"), b.unit())),
            ],
        ),
    );
    let build = b.lam(
        &["up", "own", "forked"],
        b.block(
            [
                b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
                b.let_("done_r", b.rmake(b.none())),
                b.let_("_f", spawn),
            ],
            pull,
        ),
    );
    b.lam(&["t", "self"], b.app(rt(b, "Helper.ch_bracket"), build))
}
