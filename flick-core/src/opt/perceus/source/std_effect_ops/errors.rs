//! `Effect.Errors` exit, cause, tap, match, and ignore adapters.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_observe_log::emit_log_expr;
use super::super::{Expr, ExprKind, Pat};

pub(super) fn error_ops_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickOnExit", 2) => Some(on_exit_template(b)),
        ("flickOnExitIf", 3) => Some(on_exit_if_template(b)),
        ("flickOnExitFilter", 3) => Some(on_exit_filter_template(b)),
        ("flickOnInterrupt", 2) => Some(on_interrupt_template(b)),
        ("flickOnError", 2) => Some(on_error_template(b)),
        ("flickOnErrorIf", 3) => Some(on_error_if_template(b)),
        ("flickOnErrorFilter", 3) => Some(on_error_filter_template(b)),
        ("flickCatchCauseIf", 3) => Some(catch_cause_if_template(b)),
        ("flickCatchCauseFilter", 3) => Some(catch_cause_filter_template(b)),
        ("flickTapCause", 2) => Some(tap_cause_template(b)),
        ("flickTapCauseIf", 3) => Some(tap_cause_if_template(b)),
        ("flickTapCauseFilter", 3) => Some(tap_cause_filter_template(b)),
        ("flickTapDefect", 2) => Some(tap_defect_template(b)),
        (
            "flickMatchCause"
            | "flickMatchCauseEager"
            | "flickMatchCauseEffect"
            | "flickMatchCauseEffectEager",
            3,
        ) => Some(match_cause_template(b)),
        ("flickIgnore", 2) => Some(ignore_template(b)),
        ("flickIgnoreCause", 2) => Some(ignore_cause_template(b)),
        ("flickWithErrorReporting", 2) => Some(b.lam(
            &["_defects_only", "t"],
            b.at(ExprKind::Await(Box::new(b.loc("t")))),
        )),
        _ => None,
    }
}

fn on_exit_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["f", "t"],
        b.app3(
            b.global("Effect.Errors", "on_exit_primitive"),
            b.bool_(false),
            b.lam(
                &["ex"],
                b.some(b.at(ExprKind::Task(Box::new(b.app(b.loc("f"), b.loc("ex")))))),
            ),
            b.loc("t"),
        ),
    )
}

fn on_exit_if_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["p", "f", "t"],
        b.app3(
            b.global("Effect.Errors", "on_exit_primitive"),
            b.bool_(false),
            b.lam(
                &["ex"],
                b.if_(
                    b.app(b.loc("p"), b.loc("ex")),
                    b.some(b.at(ExprKind::Task(Box::new(b.app(b.loc("f"), b.loc("ex")))))),
                    b.none(),
                ),
            ),
            b.loc("t"),
        ),
    )
}

fn on_exit_filter_template(b: &ExprBuilder) -> Expr {
    let decide = b.lam(
        &["ex"],
        b.match_(
            b.app(b.loc("filt"), b.loc("ex")),
            vec![
                arm(
                    b.ok_pat("v"),
                    b.some(b.at(ExprKind::Task(Box::new(b.app2(
                        b.loc("f"),
                        b.loc("v"),
                        b.loc("ex"),
                    ))))),
                ),
                arm(Pat::Wild, b.none()),
            ],
        ),
    );
    b.lam(
        &["filt", "f", "t"],
        b.app3(
            b.global("Effect.Errors", "on_exit_primitive"),
            b.bool_(false),
            decide,
            b.loc("t"),
        ),
    )
}

fn on_interrupt_template(b: &ExprBuilder) -> Expr {
    let on_fail = b.lam(
        &["ex"],
        b.match_(
            b.app(b.global("Exit", "failure"), b.loc("ex")),
            vec![
                arm(
                    b.some_pat("c"),
                    b.if_(
                        b.app(b.global("Cause", "has_interrupts"), b.loc("c")),
                        b.some(b.at(ExprKind::Task(Box::new(b.app(
                            b.loc("f"),
                            b.app(b.global("Cause", "interruptors"), b.loc("c")),
                        ))))),
                        b.none(),
                    ),
                ),
                arm(Pat::Wild, b.none()),
            ],
        ),
    );
    b.lam(
        &["f", "t"],
        b.app3(
            b.global("Effect.Errors", "on_exit_primitive"),
            b.bool_(false),
            on_fail,
            b.loc("t"),
        ),
    )
}

fn on_error_template(b: &ExprBuilder) -> Expr {
    let decide = b.lam(
        &["ex"],
        b.match_(
            b.app(b.global("Exit", "failure"), b.loc("ex")),
            vec![
                arm(
                    b.some_pat("c"),
                    b.some(b.at(ExprKind::Task(Box::new(b.block(
                        vec![b.let_("_u", b.app(b.loc("f"), b.loc("c")))],
                        b.unit(),
                    ))))),
                ),
                arm(Pat::Wild, b.none()),
            ],
        ),
    );
    b.lam(
        &["f", "t"],
        b.app3(
            b.global("Effect.Errors", "on_exit_primitive"),
            b.bool_(false),
            decide,
            b.loc("t"),
        ),
    )
}

fn on_error_if_template(b: &ExprBuilder) -> Expr {
    let decide = b.lam(
        &["ex"],
        b.match_(
            b.app(b.global("Exit", "failure"), b.loc("ex")),
            vec![
                arm(
                    b.some_pat("c"),
                    b.if_(
                        b.app(b.loc("p"), b.loc("c")),
                        b.some(b.at(ExprKind::Task(Box::new(b.app(b.loc("f"), b.loc("c")))))),
                        b.none(),
                    ),
                ),
                arm(Pat::Wild, b.none()),
            ],
        ),
    );
    b.lam(
        &["p", "f", "t"],
        b.app3(
            b.global("Effect.Errors", "on_exit_primitive"),
            b.bool_(false),
            decide,
            b.loc("t"),
        ),
    )
}

fn on_error_filter_template(b: &ExprBuilder) -> Expr {
    let match_res = b.match_(
        b.app(b.loc("filt"), b.loc("c")),
        vec![
            arm(
                b.ok_pat("v"),
                b.some(b.at(ExprKind::Task(Box::new(b.app2(
                    b.loc("f"),
                    b.loc("v"),
                    b.loc("c"),
                ))))),
            ),
            arm(Pat::Wild, b.none()),
        ],
    );
    let decide = b.lam(
        &["ex"],
        b.match_(
            b.app(b.global("Exit", "failure"), b.loc("ex")),
            vec![arm(b.some_pat("c"), match_res), arm(Pat::Wild, b.none())],
        ),
    );
    b.lam(
        &["filt", "f", "t"],
        b.app3(
            b.global("Effect.Errors", "on_exit_primitive"),
            b.bool_(false),
            decide,
            b.loc("t"),
        ),
    )
}

fn catch_cause_if_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["p", "f", "t"],
        b.app2(
            b.global("Effect.Errors", "catch_cause"),
            b.lam(
                &["c"],
                b.if_(
                    b.app(b.loc("p"), b.loc("c")),
                    b.app(b.loc("f"), b.loc("c")),
                    b.app(b.global("Failure", "rethrow"), b.loc("c")),
                ),
            ),
            b.loc("t"),
        ),
    )
}

fn catch_cause_filter_template(b: &ExprBuilder) -> Expr {
    let handle = b.lam(
        &["c"],
        b.match_(
            b.app(b.loc("filt"), b.loc("c")),
            vec![
                arm(b.ok_pat("v"), b.app2(b.loc("f"), b.loc("v"), b.loc("c"))),
                arm(
                    b.err_pat("rest"),
                    b.app(b.global("Failure", "rethrow"), b.loc("rest")),
                ),
            ],
        ),
    );
    b.lam(
        &["filt", "f", "t"],
        b.app2(b.global("Effect.Errors", "catch_cause"), handle, b.loc("t")),
    )
}

fn tap_cause_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["f", "t"],
        b.app2(
            b.global("Effect.Errors", "catch_cause"),
            b.lam(
                &["c"],
                b.block(
                    vec![b.let_("_r", b.app(b.loc("f"), b.loc("c")))],
                    b.app(b.global("Failure", "rethrow"), b.loc("c")),
                ),
            ),
            b.loc("t"),
        ),
    )
}

fn tap_cause_if_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["p", "f", "t"],
        b.app2(
            b.global("Effect.Errors", "catch_cause"),
            b.lam(
                &["c"],
                b.block(
                    vec![b.let_(
                        "_r",
                        b.if_(
                            b.app(b.loc("p"), b.loc("c")),
                            b.block(vec![b.let_("_u", b.app(b.loc("f"), b.loc("c")))], b.unit()),
                            b.unit(),
                        ),
                    )],
                    b.app(b.global("Failure", "rethrow"), b.loc("c")),
                ),
            ),
            b.loc("t"),
        ),
    )
}

fn tap_cause_filter_template(b: &ExprBuilder) -> Expr {
    let handle = b.lam(
        &["c"],
        b.block(
            vec![b.let_(
                "_u",
                b.match_(
                    b.app(b.loc("filt"), b.loc("c")),
                    vec![
                        arm(
                            b.ok_pat("v"),
                            b.block(
                                vec![b.let_("_x", b.app2(b.loc("f"), b.loc("v"), b.loc("c")))],
                                b.unit(),
                            ),
                        ),
                        arm(Pat::Wild, b.unit()),
                    ],
                ),
            )],
            b.app(b.global("Failure", "rethrow"), b.loc("c")),
        ),
    );
    b.lam(
        &["filt", "f", "t"],
        b.app2(b.global("Effect.Errors", "catch_cause"), handle, b.loc("t")),
    )
}

fn tap_defect_template(b: &ExprBuilder) -> Expr {
    let handle = b.lam(
        &["c"],
        b.block(
            vec![b.let_(
                "_u",
                b.match_(
                    b.app(b.global("Cause", "find_defect"), b.loc("c")),
                    vec![
                        arm(
                            b.ok_pat("msg"),
                            b.block(
                                vec![b.let_("_x", b.app(b.loc("f"), b.loc("msg")))],
                                b.unit(),
                            ),
                        ),
                        arm(Pat::Wild, b.unit()),
                    ],
                ),
            )],
            b.app(b.global("Failure", "rethrow"), b.loc("c")),
        ),
    );
    b.lam(
        &["f", "t"],
        b.app2(b.global("Effect.Errors", "catch_cause"), handle, b.loc("t")),
    )
}

fn match_cause_template(b: &ExprBuilder) -> Expr {
    let on_fail = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("ex")),
        vec![
            arm(b.some_pat("c"), b.app(b.loc("on_fail"), b.loc("c"))),
            arm(
                Pat::Wild,
                b.app(b.loc("on_fail"), b.global("Cause", "empty")),
            ),
        ],
    );
    b.lam(
        &["on_fail", "on_ok", "t"],
        b.block(
            vec![b.let_("ex", b.app(b.global("Effect.Core", "exit"), b.loc("t")))],
            b.match_(
                b.app(b.global("Exit", "value"), b.loc("ex")),
                vec![
                    arm(b.some_pat("v"), b.app(b.loc("on_ok"), b.loc("v"))),
                    arm(Pat::Wild, on_fail),
                ],
            ),
        ),
    )
}

fn log_ignored_cause(b: &ExprBuilder) -> Expr {
    b.match_(
        b.loc("lvl"),
        vec![
            arm(
                b.some_pat("l"),
                emit_log_expr(b, b.loc("l"), b.str_(""), b.some(b.loc("c"))),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    )
}

fn ignore_template(b: &ExprBuilder) -> Expr {
    let handle = b.lam(
        &["c"],
        b.if_(
            b.app(b.global("Cause", "has_dies"), b.loc("c")),
            b.app(b.global("Failure", "rethrow"), b.loc("c")),
            b.if_(
                b.app(b.global("Cause", "has_interrupts_only"), b.loc("c")),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
                log_ignored_cause(b),
            ),
        ),
    );
    b.lam(
        &["lvl", "t"],
        b.app2(
            b.global("Effect.Errors", "catch_cause"),
            handle,
            b.at(ExprKind::Task(Box::new(b.block(
                vec![b.let_("_v", b.at(ExprKind::Await(Box::new(b.loc("t")))))],
                b.unit(),
            )))),
        ),
    )
}

fn ignore_cause_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["lvl", "t"],
        b.block(
            vec![b.let_("ex", b.app(b.global("Effect.Core", "exit"), b.loc("t")))],
            b.match_(
                b.app(b.global("Exit", "failure"), b.loc("ex")),
                vec![
                    arm(b.some_pat("c"), log_ignored_cause(b)),
                    arm(Pat::Wild, b.unit()),
                ],
            ),
        ),
    )
}
