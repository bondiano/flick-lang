//! `Schedule` metadata/duration helpers and `Effect.Core.{repeat,retry,schedule}*` runners.

use flick_ir::{Arith, BinOp, Lit, Num};

use super::expr_builder::{ExprBuilder, arm};
use super::{Expr, ExprKind, Pat, Stmt};

pub(super) fn from_millis(b: &ExprBuilder, ms: Expr) -> Expr {
    b.block(
        vec![b.let_("fm", ms)],
        b.match_(
            b.app(b.global("Duration", "millis"), b.loc("fm")),
            vec![
                arm(b.ok_pat("d"), b.loc("d")),
                arm(
                    Pat::Wild,
                    b.if_(
                        b.bin(BinOp::Ge, b.loc("fm"), b.float(0.0)),
                        b.global("Duration", "infinity"),
                        b.global("Duration", "zero"),
                    ),
                ),
            ],
        ),
    )
}

pub(super) fn millis_of(b: &ExprBuilder, d: Expr) -> Expr {
    b.match_(
        b.app(b.global("Duration", "to_millis"), d),
        vec![
            arm(b.some_pat("m"), b.loc("m")),
            arm(Pat::Wild, b.global("Float", "infinity")),
        ],
    )
}

pub(super) fn zero_millis(b: &ExprBuilder, ms: Expr) -> Expr {
    b.app(b.global("Duration", "is_zero"), from_millis(b, ms))
}

#[expect(clippy::too_many_arguments, reason = "8 fields of Schedule.Metadata")]
fn meta_rec(
    b: &ExprBuilder,
    inp: Expr,
    out: Expr,
    dur: Expr,
    att: Expr,
    start: Expr,
    now: Expr,
    el: Expr,
    since: Expr,
) -> Expr {
    b.at(ExprKind::Record(vec![
        ("attempt".into(), att),
        ("duration".into(), dur),
        ("elapsed".into(), el),
        ("elapsedSincePrevious".into(), since),
        ("input".into(), inp),
        ("now".into(), now),
        ("output".into(), out),
        ("start".into(), start),
    ]))
}

pub(super) fn default_meta(b: &ExprBuilder) -> Expr {
    meta_rec(
        b,
        b.unit(),
        b.unit(),
        b.global("Duration", "zero"),
        b.int(0),
        b.float(0.0),
        b.float(0.0),
        b.float(0.0),
        b.float(0.0),
    )
}

pub(super) fn meta_ref(b: &ExprBuilder) -> Expr {
    b.app2(
        b.global("Context", "reference"),
        b.at(ExprKind::Lit(Lit::String(
            "effect/Schedule/CurrentMetadata".into(),
        ))),
        b.lam(&["_u"], default_meta(b)),
    )
}

pub(super) fn make_counter(b: &ExprBuilder) -> Expr {
    b.app(
        b.global("Ref", "make"),
        b.tuple(vec![b.int(0), b.float(0.0), b.float(0.0)]),
    )
}

pub(super) fn step_counter(
    b: &ExprBuilder,
    c: Expr,
    now: Expr,
    inp: Expr,
    out: Expr,
    dur: Expr,
    body: Expr,
) -> Expr {
    let fsub = |l: Expr, r: Expr| b.bin(BinOp::Arith(Arith::Sub, Num::Float), l, r);
    let inner = b.block(
        vec![
            b.let_("catt", b.add_i(b.loc("cn"), b.int(1))),
            b.let_(
                "cst",
                b.if_(
                    b.bin(BinOp::Eq, b.loc("cn"), b.int(0)),
                    b.loc("cnow"),
                    b.loc("cs0"),
                ),
            ),
            b.let_(
                "csp",
                b.if_(
                    b.bin(BinOp::Eq, b.loc("cn"), b.int(0)),
                    b.float(0.0),
                    fsub(b.loc("cnow"), b.loc("cprev")),
                ),
            ),
            Stmt::Expr(b.app2(
                b.global("Ref", "set"),
                b.loc("cref"),
                b.tuple(vec![b.loc("catt"), b.loc("cst"), b.loc("cnow")]),
            )),
            b.let_(
                "m",
                meta_rec(
                    b,
                    inp,
                    out,
                    dur,
                    b.loc("catt"),
                    b.loc("cst"),
                    b.loc("cnow"),
                    fsub(b.loc("cnow"), b.loc("cst")),
                    b.loc("csp"),
                ),
            ),
        ],
        body,
    );
    b.block(
        vec![b.let_("cref", c), b.let_("cnow", now)],
        b.match_(
            b.app(b.global("Ref", "get"), b.loc("cref")),
            vec![arm(
                Pat::Tuple(vec![
                    Pat::Bind(b.name("cn")),
                    Pat::Bind(b.name("cs0")),
                    Pat::Bind(b.name("cprev")),
                ]),
                inner,
            )],
        ),
    )
}

pub(super) fn effect_schedule_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("repeat", 2) => Some(b.lam(
            &["sched", "t"],
            b.app3(
                b.global("Effect.Core", "repeat_or_else"),
                b.loc("sched"),
                b.lam(
                    &["err", "_m"],
                    b.app(b.global("Fail", "fail"), b.loc("err")),
                ),
                b.loc("t"),
            ),
        )),
        ("repeatOrElse", 3) => Some(repeat_or_else_template(b)),
        ("retry", 2) => Some(b.lam(
            &["sched", "t"],
            b.app3(
                b.global("Effect.Core", "retry_or_else"),
                b.loc("sched"),
                b.lam(
                    &["err", "_o"],
                    b.app(b.global("Fail", "fail"), b.loc("err")),
                ),
                b.loc("t"),
            ),
        )),
        ("retryOrElse", 3) => Some(retry_or_else_template(b)),
        ("schedule", 2) => Some(b.lam(
            &["sched", "t"],
            b.app3(
                b.global("Effect.Core", "schedule_from"),
                b.unit(),
                b.loc("sched"),
                b.loc("t"),
            ),
        )),
        ("scheduleFrom", 3) => Some(schedule_from_template(b)),
        _ => None,
    }
}

fn error_only_branch(b: &ExprBuilder, cause_name: &str, on_err: Expr) -> Expr {
    let is_err_only = b.bin(
        BinOp::And,
        b.app(b.global("Cause", "has_fails"), b.loc(cause_name)),
        b.bin(
            BinOp::And,
            b.not(b.app(b.global("Cause", "has_dies"), b.loc(cause_name))),
            b.not(b.app(b.global("Cause", "has_interrupts"), b.loc(cause_name))),
        ),
    );
    b.if_(
        is_err_only,
        b.match_(
            b.app(b.global("Cause", "find_error_option"), b.loc(cause_name)),
            vec![
                arm(b.some_pat("err"), on_err),
                arm(
                    Pat::Wild,
                    b.app(b.global("Failure", "rethrow"), b.loc(cause_name)),
                ),
            ],
        ),
        b.app(b.global("Failure", "rethrow"), b.loc(cause_name)),
    )
}

fn repeat_or_else_template(b: &ExprBuilder) -> Expr {
    let run_loop_step = b.block(
        vec![
            b.let_(
                "v",
                b.app3(
                    b.global("Context", "with_reference"),
                    b.loc("mref"),
                    b.app(b.global("Ref", "get"), b.loc("lmeta")),
                    b.loc("t"),
                ),
            ),
            b.let_(
                "now",
                b.app(b.global("Clock", "current_time_millis"), b.unit()),
            ),
        ],
        b.match_(
            b.app2(b.loc("step"), b.loc("now"), b.loc("v")),
            vec![arm(
                Pat::Tuple(vec![
                    Pat::Bind(b.name("out")),
                    Pat::Bind(b.name("del")),
                    Pat::Bind(b.name("done")),
                ]),
                b.if_(
                    b.loc("done"),
                    b.break_("rep", b.loc("out")),
                    step_counter(
                        b,
                        b.loc("cnt"),
                        b.loc("now"),
                        b.loc("v"),
                        b.loc("out"),
                        b.loc("del"),
                        b.block(
                            vec![
                                Stmt::Expr(b.app2(
                                    b.global("Ref", "set"),
                                    b.loc("lmeta"),
                                    b.loc("m"),
                                )),
                                Stmt::Expr(b.app2(
                                    b.global("Ref", "set"),
                                    b.loc("hmeta"),
                                    b.bool_(true),
                                )),
                            ],
                            b.app(b.global("Effect.Core", "sleep"), b.loc("del")),
                        ),
                    ),
                ),
            )],
        ),
    );
    let body_task = b.at(ExprKind::Task(Box::new(b.at(ExprKind::ControlLoop {
        id: "rep".into(),
        condition: None,
        body: Box::new(run_loop_step),
        iterable: false,
    }))));
    let on_cause = b.lam(
        &["cause"],
        error_only_branch(
            b,
            "cause",
            b.app2(
                b.loc("fb"),
                b.loc("err"),
                b.if_(
                    b.app(b.global("Ref", "get"), b.loc("hmeta")),
                    b.some(b.app(b.global("Ref", "get"), b.loc("lmeta"))),
                    b.none(),
                ),
            ),
        ),
    );
    b.lam(
        &["sched", "fb", "t"],
        b.block(
            vec![
                b.let_("step", b.app(b.loc("sched"), b.unit())),
                b.let_("cnt", make_counter(b)),
                b.let_("mref", meta_ref(b)),
                b.let_("lmeta", b.app(b.global("Ref", "make"), default_meta(b))),
                b.let_("hmeta", b.app(b.global("Ref", "make"), b.bool_(false))),
            ],
            b.app2(
                b.global("Effect.Errors", "catch_cause"),
                on_cause,
                body_task,
            ),
        ),
    )
}

fn retry_or_else_template(b: &ExprBuilder) -> Expr {
    let on_err = b.block(
        vec![b.let_(
            "now",
            b.app(b.global("Clock", "current_time_millis"), b.unit()),
        )],
        b.match_(
            b.app2(b.loc("step"), b.loc("now"), b.loc("err")),
            vec![arm(
                Pat::Tuple(vec![
                    Pat::Bind(b.name("out")),
                    Pat::Bind(b.name("del")),
                    Pat::Bind(b.name("done")),
                ]),
                b.if_(
                    b.loc("done"),
                    b.break_("ret", b.app2(b.loc("fb"), b.loc("err"), b.loc("out"))),
                    step_counter(
                        b,
                        b.loc("cnt"),
                        b.loc("now"),
                        b.loc("err"),
                        b.loc("out"),
                        b.loc("del"),
                        b.block(
                            vec![Stmt::Expr(b.app2(
                                b.global("Ref", "set"),
                                b.loc("lmeta"),
                                b.loc("m"),
                            ))],
                            b.app(b.global("Effect.Core", "sleep"), b.loc("del")),
                        ),
                    ),
                ),
            )],
        ),
    );
    let on_fail = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("ex")),
        vec![
            arm(b.some_pat("cause"), error_only_branch(b, "cause", on_err)),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let loop_step = b.block(
        vec![b.let_(
            "ex",
            b.app(
                b.global("Effect.Core", "exit"),
                b.at(ExprKind::Task(Box::new(b.app3(
                    b.global("Context", "with_reference"),
                    b.loc("mref"),
                    b.app(b.global("Ref", "get"), b.loc("lmeta")),
                    b.loc("t"),
                )))),
            ),
        )],
        b.match_(
            b.app(b.global("Exit", "value"), b.loc("ex")),
            vec![
                arm(b.some_pat("v"), b.break_("ret", b.loc("v"))),
                arm(Pat::Wild, on_fail),
            ],
        ),
    );
    b.lam(
        &["sched", "fb", "t"],
        b.block(
            vec![
                b.let_("step", b.app(b.loc("sched"), b.unit())),
                b.let_("cnt", make_counter(b)),
                b.let_("mref", meta_ref(b)),
                b.let_("lmeta", b.app(b.global("Ref", "make"), default_meta(b))),
            ],
            b.at(ExprKind::ControlLoop {
                id: "ret".into(),
                condition: None,
                body: Box::new(loop_step),
                iterable: false,
            }),
        ),
    )
}

fn schedule_from_template(b: &ExprBuilder) -> Expr {
    let loop_step = b.block(
        vec![
            b.let_("inp", b.app(b.global("Ref", "get"), b.loc("iref"))),
            b.let_(
                "now",
                b.app(b.global("Clock", "current_time_millis"), b.unit()),
            ),
        ],
        b.match_(
            b.app2(b.loc("step"), b.loc("now"), b.loc("inp")),
            vec![arm(
                Pat::Tuple(vec![
                    Pat::Bind(b.name("out")),
                    Pat::Bind(b.name("del")),
                    Pat::Bind(b.name("done")),
                ]),
                b.if_(
                    b.loc("done"),
                    b.break_("sfr", b.loc("out")),
                    step_counter(
                        b,
                        b.loc("cnt"),
                        b.loc("now"),
                        b.loc("inp"),
                        b.loc("out"),
                        b.loc("del"),
                        b.block(
                            vec![
                                Stmt::Expr(b.app(b.global("Effect.Core", "sleep"), b.loc("del"))),
                                b.let_(
                                    "v",
                                    b.app3(
                                        b.global("Context", "with_reference"),
                                        b.loc("mref"),
                                        b.loc("m"),
                                        b.loc("t"),
                                    ),
                                ),
                            ],
                            b.app2(b.global("Ref", "set"), b.loc("iref"), b.loc("v")),
                        ),
                    ),
                ),
            )],
        ),
    );
    b.lam(
        &["init", "sched", "t"],
        b.block(
            vec![
                b.let_("step", b.app(b.loc("sched"), b.unit())),
                b.let_("cnt", make_counter(b)),
                b.let_("mref", meta_ref(b)),
                b.let_("iref", b.app(b.global("Ref", "make"), b.loc("init"))),
            ],
            b.at(ExprKind::ControlLoop {
                id: "sfr".into(),
                condition: None,
                body: Box::new(loop_step),
                iterable: false,
            }),
        ),
    )
}
