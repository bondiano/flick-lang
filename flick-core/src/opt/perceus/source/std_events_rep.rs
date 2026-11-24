//! Audited `Events` and `ErrorReporter` pure IR adapters (Stage 4D).

use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Expr, ExprKind, Local, Lower, LowerError, Pat, Span, Stmt};

const REPORTERS_KEY: &str = "effect/ErrorReporter/CurrentErrorReporters";

impl Lower<'_> {
    pub(super) fn std_events_rep_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        let (from, bind) = value.binding.module_export()?;
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$ev");
        let expr = match from {
            "flick:rt" => events_template(&b, bind, value.mode, stages)?,
            "flick:ErrorReporter" => error_reporter_template(&b, bind, value.mode, stages)?,
            _ => return None,
        };
        Some(self.lower_with_manual_scope(&expr, "$ev", block, span))
    }
}

fn events_worker_step(b: &ExprBuilder) -> Expr {
    b.match_(
        b.app(b.global("Queue", "take"), b.loc("q")),
        vec![
            arm(
                b.some_pat("ev"),
                b.match_(
                    b.app(
                        b.global("Exit", "failure"),
                        b.app(
                            b.global("Effect.Core", "exit"),
                            b.task(b.app2(
                                b.loc("env_run"),
                                b.loc("env"),
                                b.app(b.loc("h"), b.loc("ev")),
                            )),
                        ),
                    ),
                    vec![
                        arm(
                            b.some_pat("c"),
                            b.if_(
                                b.bin(
                                    BinOp::Or,
                                    b.app(b.global("Cause", "has_fails"), b.loc("c")),
                                    b.app(b.global("Cause", "has_dies"), b.loc("c")),
                                ),
                                b.block(
                                    vec![b.let_(
                                        "_s",
                                        b.app(
                                            b.global("Effect.Core", "exit"),
                                            b.task(b.app2(
                                                b.loc("env_run"),
                                                b.loc("env"),
                                                b.task(b.app(b.loc("sup"), b.loc("c"))),
                                            )),
                                        ),
                                    )],
                                    b.unit(),
                                ),
                                b.unit(),
                            ),
                        ),
                        arm(Pat::Wild, b.unit()),
                    ],
                ),
            ),
            arm(Pat::Wild, b.break_("evw", b.unit())),
        ],
    )
}

fn events_listen_template(b: &ExprBuilder) -> Expr {
    let worker_step = events_worker_step(b);
    let overflow_msg = b.bin(
        BinOp::Concat,
        b.bin(
            BinOp::Concat,
            b.str_("error event queue full: "),
            b.app(b.loc("show_val"), b.loc("cap")),
        ),
        b.str_(" events are waiting"),
    );
    let on_overflow = b.block(
        vec![b.let_(
            "_sf",
            b.app(
                b.global("Effect.Core", "exit"),
                b.task(b.app2(
                    b.loc("env_run"),
                    b.loc("env"),
                    b.task(b.app(b.loc("sup"), b.app(b.global("Cause", "die"), overflow_msg))),
                )),
            ),
        )],
        b.unit(),
    );
    let listener_fn = b.lam(
        &["ev"],
        b.if_(
            b.rget(b.loc("cl")),
            b.unit(),
            b.if_(
                b.app2(b.global("Queue", "offer"), b.loc("ev"), b.loc("q")),
                b.unit(),
                on_overflow,
            ),
        ),
    );
    b.lam(
        &["owner", "cap", "sup", "h"],
        b.block(
            vec![
                b.let_("q", b.app(b.global("Queue", "dropping"), b.loc("cap"))),
                b.let_("cl", b.rmake(b.bool_(false))),
                b.let_("env", b.app(b.loc("env_capture"), b.unit())),
                Stmt::Expr(b.app2(
                    b.loc("owner_run"),
                    b.loc("owner"),
                    b.task(b.app(
                        b.global("Scope", "add_finalizer"),
                        b.task(b.block(vec![b.rset(b.loc("cl"), b.bool_(true))], b.unit())),
                    )),
                )),
                b.let_(
                    "_wf",
                    b.app3(
                        b.global("Fiber", "spawn"),
                        b.loc("owner"),
                        b.lam(&["_f"], b.unit()),
                        b.task(b.loop_ctrl("evw", worker_step)),
                    ),
                ),
            ],
            listener_fn,
        ),
    )
}

fn events_attach_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["l", "add", "rem"],
        b.block(
            vec![
                Stmt::Expr(b.app(b.loc("add"), b.loc("l"))),
                Stmt::Expr(b.app(
                    b.global("Scope", "add_finalizer"),
                    b.task(b.app(b.loc("rem"), b.loc("l"))),
                )),
            ],
            b.unit(),
        ),
    )
}

fn events_template(b: &ExprBuilder, bind: &str, mode: ForeignMode, stages: usize) -> Option<Expr> {
    match (bind, mode, stages) {
        ("eventsQueue", ForeignMode::Pure, 1) => Some(b.lam(&["cap"], b.loc("cap"))),
        ("eventsListen", ForeignMode::Effect, 4) => Some(events_listen_template(b)),
        ("eventsAttach", ForeignMode::Effect, 3) => Some(events_attach_template(b)),
        _ => None,
    }
}

fn reporters_ref(b: &ExprBuilder) -> Expr {
    b.app2(
        b.global("Context", "reference"),
        b.str_(REPORTERS_KEY),
        b.lam(&["_u"], b.list(vec![])),
    )
}

fn error_reporter_layer_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["reps", "opts"],
        b.app(
            b.global("Layer", "from_build"),
            b.lam(
                &["_m"],
                b.block(
                    vec![
                        b.let_("rref", reporters_ref(b)),
                        b.let_(
                            "cur",
                            b.if_(
                                b.field(b.loc("opts"), "mergeWithExisting"),
                                b.app(b.global("Context", "read"), b.loc("rref")),
                                b.list(vec![]),
                            ),
                        ),
                        b.let_(
                            "next",
                            b.app2(b.global("List.Core", "append"), b.loc("cur"), b.loc("reps")),
                        ),
                    ],
                    b.app2(b.global("Context", "make"), b.loc("rref"), b.loc("next")),
                ),
            ),
        ),
    )
}

fn report_reason_expr(b: &ExprBuilder) -> Expr {
    b.if_(
        b.app(b.global("Cause", "is_interrupt_reason"), b.loc("r")),
        b.unit(),
        b.block(
            vec![
                b.let_(
                    "rc",
                    b.app(b.global("Cause", "from_reasons"), b.list(vec![b.loc("r")])),
                ),
                b.let_("msg", b.app(b.global("Cause", "pretty"), b.loc("rc"))),
                b.let_(
                    "rep",
                    b.at(ExprKind::Record(vec![
                        ("cause".into(), b.loc("rc")),
                        ("message".into(), b.loc("msg")),
                        ("severity".into(), b.str_("Info")),
                        ("attributes".into(), b.list(vec![])),
                        (
                            "timestamp".into(),
                            b.app(b.global("Clock", "current_time_nanos"), b.unit()),
                        ),
                    ])),
                ),
            ],
            b.app(b.loc("l"), b.loc("rep")),
        ),
    )
}

fn error_reporter_report_template(b: &ExprBuilder) -> Expr {
    let for_reasons = b.for_ctrl(
        "er_r",
        "r",
        b.app(b.global("Cause", "reasons"), b.loc("failure")),
        report_reason_expr(b),
    );
    let for_reps = b.for_ctrl(
        "er_l",
        "l",
        b.app(b.global("Context", "read"), reporters_ref(b)),
        for_reasons,
    );
    b.lam(&["failure"], for_reps)
}

fn error_reporter_template(
    b: &ExprBuilder,
    bind: &str,
    mode: ForeignMode,
    stages: usize,
) -> Option<Expr> {
    match (bind, mode, stages) {
        ("flickMake", ForeignMode::Pure, 1) => Some(b.lam(&["l"], b.loc("l"))),
        ("flickLayer", ForeignMode::Pure, 2) => Some(error_reporter_layer_template(b)),
        ("flickReport", ForeignMode::Effect, 1) => Some(error_reporter_report_template(b)),
        _ => None,
    }
}
