//! Audited `Stream.Errors` pure IR adapters (Stage 4E).

use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::{rt, str_lit};
use super::{Expr, ExprKind, Pat};

pub(super) fn stream_pipe_effect_template(
    b: &ExprBuilder,
    bind: &str,
    stages: usize,
) -> Option<Expr> {
    super::std_stream_pipe::stream_pipe_effect_template(b, bind, stages)
}

pub(super) fn stream_errors_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    super::std_stream_pipe::stream_pipe_pure_template(b, bind, stages)
        .or_else(|| stream_catch_error_template(b, bind, stages))
        .or_else(|| stream_catch_cause_template(b, bind, stages))
        .or_else(|| stream_tap_and_fallback_template(b, bind, stages))
        .or_else(|| stream_lifecycle_template(b, bind, stages))
}

fn fail_stream(b: &ExprBuilder, err_expr: Expr) -> Expr {
    b.lam(
        &["_up", "_own"],
        b.lam(&["_u"], b.app(b.global("Fail", "fail"), err_expr)),
    )
}

fn fail_cause_stream(b: &ExprBuilder, cause_expr: Expr) -> Expr {
    b.lam(
        &["_up", "_own"],
        b.lam(&["_u"], b.app(b.global("Failure", "rethrow"), cause_expr)),
    )
}

fn stream_catch_error_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("catch", 2) => Some(rt(b, "Helper.ch_catch")),
        ("catchIf", 3) => Some(b.lam(
            &["p", "f", "self"],
            b.app2(
                rt(b, "Helper.ch_catch"),
                b.lam(
                    &["e"],
                    b.if_(
                        b.app(b.loc("p"), b.loc("e")),
                        b.app(b.loc("f"), b.loc("e")),
                        fail_stream(b, b.loc("e")),
                    ),
                ),
                b.loc("self"),
            ),
        )),
        ("catchFilter", 3) => Some(b.lam(
            &["flt", "f", "self"],
            b.app2(
                rt(b, "Helper.ch_catch"),
                b.lam(
                    &["e"],
                    b.match_(
                        b.app(b.loc("flt"), b.loc("e")),
                        vec![
                            arm(b.ok_pat("v"), b.app(b.loc("f"), b.loc("v"))),
                            arm(b.err_pat("e2"), fail_stream(b, b.loc("e2"))),
                        ],
                    ),
                ),
                b.loc("self"),
            ),
        )),
        ("orElseSucceed", 2) => Some(b.lam(
            &["f", "self"],
            b.app2(
                rt(b, "Helper.ch_catch"),
                b.lam(
                    &["e"],
                    b.app(
                        rt(b, "Helper.ch_from_list"),
                        b.list(vec![b.list(vec![b.app(b.loc("f"), b.loc("e"))])]),
                    ),
                ),
                b.loc("self"),
            ),
        )),
        ("orDie", 1) => Some(b.app(
            rt(b, "Helper.ch_catch"),
            b.lam(
                &["e", "_up", "_own"],
                b.lam(&["_u"], b.app(b.global("Effect.Core", "die"), b.loc("e"))),
            ),
        )),
        ("ignore", 1) => Some(b.app(
            rt(b, "Helper.ch_catch"),
            b.lam(&["_e", "_up", "_own"], rt(b, "Helper.ch_empty_pull")),
        )),
        _ => None,
    }
}

fn stream_catch_cause_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickCatchCause", 2) => Some(rt(b, "Helper.ch_catch_cause")),
        ("flickCatchCauseIf", 3) => Some(b.lam(
            &["p", "f", "self"],
            b.app2(
                rt(b, "Helper.ch_catch_cause"),
                b.lam(
                    &["c"],
                    b.if_(
                        b.app(b.loc("p"), b.loc("c")),
                        b.app(b.loc("f"), b.loc("c")),
                        fail_cause_stream(b, b.loc("c")),
                    ),
                ),
                b.loc("self"),
            ),
        )),
        ("flickCatchCauseFilter", 3) => Some(b.lam(
            &["flt", "f", "self"],
            b.app2(
                rt(b, "Helper.ch_catch_cause"),
                b.lam(
                    &["c"],
                    b.match_(
                        b.app(b.loc("flt"), b.loc("c")),
                        vec![
                            arm(b.ok_pat("v"), b.app2(b.loc("f"), b.loc("v"), b.loc("c"))),
                            arm(b.err_pat("c2"), fail_cause_stream(b, b.loc("c2"))),
                        ],
                    ),
                ),
                b.loc("self"),
            ),
        )),
        ("flickCatchDefect", 2) => Some(b.lam(
            &["f", "self"],
            b.app2(
                rt(b, "Helper.ch_catch_cause"),
                b.lam(
                    &["c"],
                    b.match_(
                        b.app(b.global("Cause", "find_die"), b.loc("c")),
                        vec![
                            arm(
                                b.ok_pat("r"),
                                b.app(
                                    b.loc("f"),
                                    b.app(
                                        b.global("Cause", "from_reasons"),
                                        b.list(vec![b.loc("r")]),
                                    ),
                                ),
                            ),
                            arm(Pat::Wild, fail_cause_stream(b, b.loc("c"))),
                        ],
                    ),
                ),
                b.loc("self"),
            ),
        )),
        ("ignoreCause", 1) => Some(b.app(
            rt(b, "Helper.ch_catch_cause"),
            b.lam(&["_c", "_up", "_own"], rt(b, "Helper.ch_empty_pull")),
        )),
        _ => None,
    }
}

fn stream_tap_and_fallback_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("tapError", 2) => Some(rt(b, "Helper.ch_tap_error")),
        ("flickTapCause", 2) => Some(rt(b, "Helper.ch_tap_cause")),
        ("flickTapDefect", 2) => Some(b.lam(
            &["f", "self"],
            b.app2(
                rt(b, "Helper.ch_tap_cause"),
                b.lam(
                    &["c"],
                    b.match_(
                        b.app(b.global("Cause", "find_die"), b.loc("c")),
                        vec![
                            arm(
                                b.ok_pat("r"),
                                b.block(
                                    vec![b.let_(
                                        "_x",
                                        b.app(
                                            b.loc("f"),
                                            b.app(
                                                b.global("Cause", "from_reasons"),
                                                b.list(vec![b.loc("r")]),
                                            ),
                                        ),
                                    )],
                                    b.unit(),
                                ),
                            ),
                            arm(Pat::Wild, b.unit()),
                        ],
                    ),
                ),
                b.loc("self"),
            ),
        )),
        ("flickOrElseIfEmpty", 2) => Some(b.lam(
            &["fb", "self"],
            b.app2(
                rt(b, "Helper.ch_or_else_if_empty"),
                b.lam(&["_d"], b.loc("fb")),
                b.loc("self"),
            ),
        )),
        ("retry", 2) => Some(rt(b, "Helper.ch_retry")),
        _ => None,
    }
}

fn stream_lifecycle_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickOnExit", 2) => Some(rt(b, "Helper.ch_on_exit")),
        ("flickOnError", 2) => Some(b.lam(
            &["f", "self"],
            b.app2(
                rt(b, "Helper.ch_on_exit"),
                b.lam(
                    &["ex"],
                    b.match_(
                        b.app(b.global("Exit", "failure"), b.loc("ex")),
                        vec![
                            arm(b.some_pat("c"), b.app(b.loc("f"), b.loc("c"))),
                            arm(Pat::Wild, b.unit()),
                        ],
                    ),
                ),
                b.loc("self"),
            ),
        )),
        ("onStart", 2) => Some(b.lam(
            &["t", "self"],
            b.app(
                rt(b, "Helper.ch_unwrap"),
                b.task(b.block(vec![b.let_("_x", b.await_(b.loc("t")))], b.loc("self"))),
            ),
        )),
        ("onFirst", 2) => Some(b.lam(
            &["f", "self"],
            b.app2(
                rt(b, "Helper.ch_on_first"),
                b.lam(
                    &["xs"],
                    b.app(
                        b.loc("f"),
                        b.app2(rt(b, "Helper.list_at"), b.int(0), b.loc("xs")),
                    ),
                ),
                b.loc("self"),
            ),
        )),
        ("onEnd", 2) => Some(rt(b, "Helper.ch_on_end")),
        ("ensuring", 2) => Some(b.lam(
            &["fin", "self"],
            b.app2(
                rt(b, "Helper.ch_on_exit"),
                b.lam(&["_ex"], b.await_(b.loc("fin"))),
                b.loc("self"),
            ),
        )),
        ("provide", 2) => Some(rt(b, "Helper.ch_provide")),
        ("updateService", 3) => Some(rt(b, "Helper.ch_update_service")),
        ("withSpan", 2) => Some(b.lam(
            &["nm", "self"],
            b.app3(
                rt(b, "Helper.ch_with_span"),
                b.loc("nm"),
                b.at(ExprKind::Record(vec![
                    ("attributes".into(), b.list(vec![])),
                    ("captureStackTrace".into(), b.bool_(false)),
                    ("disablePropagation".into(), b.bool_(false)),
                    ("kind_".into(), str_lit(b, "internal")),
                    ("level".into(), b.none()),
                    ("links".into(), b.list(vec![])),
                    ("parent".into(), b.none()),
                    ("root".into(), b.bool_(false)),
                    ("sampled".into(), b.none()),
                ])),
                b.loc("self"),
            ),
        )),
        _ => None,
    }
}
