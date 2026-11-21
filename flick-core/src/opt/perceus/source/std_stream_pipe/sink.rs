//! `Stream.Pipe` sink execution, transduction, and peeling helpers.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat, Stmt};
use super::run_sink_on_pull;

pub(super) fn st_run_sink_helper(b: &ExprBuilder) -> Expr {
    let unwrap_end = b.match_(
        b.loc("end"),
        [arm(pair_pat(b, "v", "_"), b.break_("srs", b.loc("v")))],
    );
    let step = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("_o"), b.unit()),
            arm(b.err_pat("end"), unwrap_end),
        ],
    );
    let piped = b.app2(
        rt(b, "Helper.ch_pipe_to_or_fail"),
        b.loc("sink"),
        b.loc("self"),
    );
    b.lam(
        &["sink", "self"],
        b.app2(
            rt(b, "Helper.ch_run_scoped"),
            piped,
            b.lam(&["pull"], b.loop_ctrl("srs", step)),
        ),
    )
}

fn transduce_with_left(b: &ExprBuilder) -> Expr {
    let record_err = b.block(
        [b.rset(b.loc("done_r"), b.some(b.err(b.loc("c"))))],
        b.err(b.unit()),
    );
    let on_cause = b.lam(
        &["c"],
        b.match_(
            b.app(b.global("Cause", "find_error_option"), b.loc("c")),
            [
                arm(b.some_pat("_e"), record_err),
                arm(Pat::Wild, b.app(b.global("Failure", "rethrow"), b.loc("c"))),
            ],
        ),
    );
    let record_done = b.block(
        [b.rset(b.loc("done_r"), b.some(b.ok(b.loc("d"))))],
        b.err(b.unit()),
    );
    let pull_task = b.task(b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("xs"), b.ok(b.loc("xs"))),
            arm(b.err_pat("d"), record_done),
        ],
    ));
    let pull_fresh = b.app2(
        b.global("Effect.Errors", "catch_cause"),
        on_cause,
        pull_task,
    );
    let take_left = b.block([b.rset(b.loc("left"), b.none())], b.ok(b.loc("xs")));
    b.lam(
        &["_u"],
        b.match_(
            b.rget(b.loc("left")),
            [arm(b.some_pat("xs"), take_left), arm(Pat::Wild, pull_fresh)],
        ),
    )
}

pub(super) fn st_transduce_helper(b: &ExprBuilder) -> Expr {
    let replay_end = b.match_(
        b.loc("er"),
        [
            arm(b.ok_pat("d"), b.err(b.loc("d"))),
            arm(
                b.err_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
        ],
    );
    let save_left = b.match_(
        b.loc("lo"),
        [
            arm(
                b.some_pat("xs"),
                b.block([b.rset(b.loc("left"), b.some(b.loc("xs")))], b.unit()),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let run_once = b.match_(
        run_sink_on_pull(b, b.loc("sink"), b.loc("with_left"), b.loc("own")),
        [arm(
            pair_pat(b, "v", "lo"),
            b.block([Stmt::Expr(save_left)], b.ok(b.list([b.loc("v")]))),
        )],
    );
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.rget(b.loc("done_r")),
            [arm(b.some_pat("er"), replay_end), arm(Pat::Wild, run_once)],
        ),
    );
    let init = [
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("left", b.rmake(b.none())),
        b.let_("done_r", b.rmake(b.none())),
        b.let_("with_left", transduce_with_left(b)),
    ];
    b.lam(&["sink", "self", "up", "own"], b.block(init, pull))
}

fn peel_run_masked(b: &ExprBuilder) -> Expr {
    let close_on_exit = b.lam(
        &["ex"],
        b.some(b.task(b.app2(
            rt(b, "Layer.OwnerClose"),
            b.loc("sown"),
            b.app(b.global("Exit", "failure"), b.loc("ex")),
        ))),
    );
    let run_sink = b.app(
        b.loc("restore"),
        b.task(run_sink_on_pull(
            b,
            b.loc("sink"),
            b.loc("wrapped"),
            b.loc("sown"),
        )),
    );
    let guarded = b.app3(
        b.global("Effect.Errors", "on_exit_primitive"),
        b.bool_(false),
        close_on_exit,
        run_sink,
    );
    let mask_body = b.lam(
        &["restore"],
        b.block(
            [b.let_("sown", b.app(rt(b, "Layer.OwnerNew"), b.unit()))],
            guarded,
        ),
    );
    b.app(
        b.global("Effect.Concurrency", "uninterruptible_mask"),
        mask_body,
    )
}

pub(super) fn st_peel_helper(b: &ExprBuilder) -> Expr {
    let wrapped = b.lam(
        &["_u"],
        b.match_(
            b.app(b.loc("orig"), b.unit()),
            [
                arm(b.ok_pat("xs"), b.ok(b.loc("xs"))),
                arm(
                    b.err_pat("d"),
                    b.block([b.rset(b.loc("ended"), b.bool_(true))], b.err(b.loc("d"))),
                ),
            ],
        ),
    );
    let tail = b.if_(
        b.rget(b.loc("ended")),
        b.lam(&["_up", "_own"], rt(b, "Helper.ch_empty_pull")),
        b.lam(&["_up", "_own"], b.loc("orig")),
    );
    let prepend_left = b.app2(
        rt(b, "Helper.ch_concat_with"),
        b.lam(&["_d"], b.loc("tail")),
        b.app(rt(b, "Helper.ch_from_list"), b.list([b.loc("xs")])),
    );
    let rest = b.match_(
        b.loc("lo"),
        [
            arm(b.some_pat("xs"), prepend_left),
            arm(Pat::Wild, b.loc("tail")),
        ],
    );
    let finish = b.match_(
        b.loc("end"),
        [arm(
            pair_pat(b, "v", "lo"),
            b.block(
                [b.let_("tail", tail), b.let_("rest", rest)],
                b.tuple([b.loc("v"), b.loc("rest")]),
            ),
        )],
    );
    let init = [
        b.let_("ow", b.app(b.global("Scope", "current"), b.unit())),
        b.let_(
            "orig",
            b.app2(b.loc("self"), rt(b, "Helper.ch_empty_pull"), b.loc("ow")),
        ),
        b.let_("ended", b.rmake(b.bool_(false))),
        b.let_("wrapped", wrapped),
        b.let_("end", peel_run_masked(b)),
    ];
    b.lam(&["sink", "self"], b.block(init, finish))
}
