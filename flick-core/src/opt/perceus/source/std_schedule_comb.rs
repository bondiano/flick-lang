//! `Schedule` combinators (`map`, `while`, `tap`, `delay`, `jittered`, `up_to`,
//! `concat`, `max`/`min`, `cron`).

use flick_ir::{BinOp, Lit};

use super::expr_builder::{ExprBuilder, arm};
use super::std_schedule::{fadd, fmul, fsub, ref_get, ref_make, ref_set};
use super::std_schedule_run::{from_millis, make_counter, millis_of, step_counter, zero_millis};
use super::{Expr, ExprKind, Pat, Stmt};

fn step_around(b: &ExprBuilder, body: Expr) -> Expr {
    let step = b.lam(
        &["now", "inp"],
        b.match_(
            b.app2(b.loc("st"), b.loc("now"), b.loc("inp")),
            vec![arm(
                Pat::Tuple(vec![
                    Pat::Bind(b.name("out")),
                    Pat::Bind(b.name("del")),
                    Pat::Bind(b.name("done")),
                ]),
                body,
            )],
        ),
    );
    b.lam(
        &["_s"],
        b.block(
            vec![
                b.let_("st", b.app(b.loc("self"), b.unit())),
                b.let_("c", make_counter(b)),
            ],
            step,
        ),
    )
}

pub(super) fn map_template(b: &ExprBuilder) -> Expr {
    let dur = b.if_(b.loc("done"), b.global("Duration", "zero"), b.loc("del"));
    let inner = step_counter(
        b,
        b.loc("c"),
        b.loc("now"),
        b.loc("inp"),
        b.loc("out"),
        dur,
        b.tuple(vec![
            b.app(b.loc("f"), b.loc("m")),
            b.loc("del"),
            b.loc("done"),
        ]),
    );
    b.lam(&["f", "self"], step_around(b, inner))
}

pub(super) fn while_template(b: &ExprBuilder) -> Expr {
    let active = step_counter(
        b,
        b.loc("c"),
        b.loc("now"),
        b.loc("inp"),
        b.loc("out"),
        b.loc("del"),
        b.tuple(vec![
            b.loc("out"),
            b.loc("del"),
            b.not(b.app(b.loc("f"), b.loc("m"))),
        ]),
    );
    let inner = b.if_(
        b.loc("done"),
        b.tuple(vec![b.loc("out"), b.loc("del"), b.bool_(true)]),
        active,
    );
    b.lam(&["f", "self"], step_around(b, inner))
}

pub(super) fn tap_template(b: &ExprBuilder) -> Expr {
    let active = step_counter(
        b,
        b.loc("c"),
        b.loc("now"),
        b.loc("inp"),
        b.loc("out"),
        b.loc("del"),
        b.block(
            vec![b.let_("_u", b.app(b.loc("f"), b.loc("m")))],
            b.tuple(vec![b.loc("out"), b.loc("del"), b.bool_(false)]),
        ),
    );
    let inner = b.if_(
        b.loc("done"),
        b.tuple(vec![b.loc("out"), b.loc("del"), b.bool_(true)]),
        active,
    );
    b.lam(&["f", "self"], step_around(b, inner))
}

pub(super) fn modify_delay_template(b: &ExprBuilder) -> Expr {
    let active = step_counter(
        b,
        b.loc("c"),
        b.loc("now"),
        b.loc("inp"),
        b.loc("out"),
        b.loc("del"),
        b.tuple(vec![
            b.loc("out"),
            b.app(b.loc("f"), b.loc("m")),
            b.bool_(false),
        ]),
    );
    let inner = b.if_(
        b.loc("done"),
        b.tuple(vec![b.loc("out"), b.loc("del"), b.bool_(true)]),
        active,
    );
    b.lam(&["f", "self"], step_around(b, inner))
}

pub(super) fn add_delay_template(b: &ExprBuilder) -> Expr {
    let active = step_counter(
        b,
        b.loc("c"),
        b.loc("now"),
        b.loc("inp"),
        b.loc("out"),
        b.loc("del"),
        b.tuple(vec![
            b.loc("out"),
            b.app2(
                b.global("Duration", "add"),
                b.app(b.loc("f"), b.loc("m")),
                b.loc("del"),
            ),
            b.bool_(false),
        ]),
    );
    let inner = b.if_(
        b.loc("done"),
        b.tuple(vec![b.loc("out"), b.loc("del"), b.bool_(true)]),
        active,
    );
    b.lam(&["f", "self"], step_around(b, inner))
}

pub(super) fn jittered_template(b: &ExprBuilder) -> Expr {
    let active = b.block(
        vec![
            b.let_("r", b.app(b.global("Random", "next"), b.unit())),
            b.let_("ms", millis_of(b, b.loc("del"))),
            b.let_(
                "scaled",
                fadd(
                    b,
                    fmul(
                        b,
                        fmul(b, b.loc("ms"), b.float(0.8)),
                        fsub(b, b.float(1.0), b.loc("r")),
                    ),
                    fmul(b, fmul(b, b.loc("ms"), b.float(1.2)), b.loc("r")),
                ),
            ),
        ],
        b.tuple(vec![
            b.loc("out"),
            from_millis(b, b.loc("scaled")),
            b.bool_(false),
        ]),
    );
    let inner = b.if_(
        b.loc("done"),
        b.tuple(vec![b.loc("out"), b.loc("del"), b.bool_(true)]),
        active,
    );
    b.lam(&["self"], step_around(b, inner))
}

pub(super) fn passthrough_template(b: &ExprBuilder) -> Expr {
    let step = b.lam(
        &["now", "inp"],
        b.match_(
            b.app2(b.loc("st"), b.loc("now"), b.loc("inp")),
            vec![arm(
                Pat::Tuple(vec![
                    Pat::Wild,
                    Pat::Bind(b.name("del")),
                    Pat::Bind(b.name("done")),
                ]),
                b.tuple(vec![b.loc("inp"), b.loc("del"), b.loc("done")]),
            )],
        ),
    );
    b.lam(
        &["self"],
        b.lam(
            &["_s"],
            b.block(vec![b.let_("st", b.app(b.loc("self"), b.unit()))], step),
        ),
    )
}

pub(super) fn up_to_template(b: &ExprBuilder) -> Expr {
    let ok_t = b.match_(
        b.field(b.loc("opts"), "times"),
        vec![
            arm(
                b.some_pat("t"),
                b.bin(BinOp::Le, b.field(b.loc("m"), "attempt"), b.loc("t")),
            ),
            arm(Pat::Wild, b.bool_(true)),
        ],
    );
    let ok_d = b.match_(
        b.field(b.loc("opts"), "duration"),
        vec![
            arm(
                b.some_pat("lim"),
                b.app2(
                    b.global("Duration", "is_less_than_or_equal_to"),
                    b.loc("lim"),
                    from_millis(b, b.field(b.loc("m"), "elapsed")),
                ),
            ),
            arm(Pat::Wild, b.bool_(true)),
        ],
    );
    let active = step_counter(
        b,
        b.loc("c"),
        b.loc("now"),
        b.loc("inp"),
        b.loc("out"),
        b.loc("del"),
        b.tuple(vec![
            b.loc("out"),
            b.loc("del"),
            b.not(b.bin(BinOp::And, ok_t, ok_d)),
        ]),
    );
    let inner = b.if_(
        b.loc("done"),
        b.tuple(vec![b.loc("out"), b.loc("del"), b.bool_(true)]),
        active,
    );
    b.lam(&["opts", "self"], step_around(b, inner))
}

pub(super) fn concat_template(b: &ExprBuilder, tagged: bool) -> Expr {
    let wrap_l = |e: Expr| if tagged { b.err(e) } else { e };
    let wrap_r = |e: Expr| if tagged { b.ok(e) } else { e };
    let run_right = |rstep: Expr| {
        b.match_(
            b.app2(rstep, b.loc("now"), b.loc("inp")),
            vec![arm(
                Pat::Tuple(vec![
                    Pat::Bind(b.name("ro")),
                    Pat::Bind(b.name("rd")),
                    Pat::Bind(b.name("rdone")),
                ]),
                b.tuple(vec![wrap_r(b.loc("ro")), b.loc("rd"), b.loc("rdone")]),
            )],
        )
    };
    let on_left = b.match_(
        b.app2(b.loc("lstep"), b.loc("now"), b.loc("inp")),
        vec![arm(
            Pat::Tuple(vec![
                Pat::Bind(b.name("lo")),
                Pat::Bind(b.name("ld")),
                Pat::Bind(b.name("ldone")),
            ]),
            b.if_(
                b.not(b.loc("ldone")),
                b.tuple(vec![wrap_l(b.loc("lo")), b.loc("ld"), b.bool_(false)]),
                b.block(
                    vec![
                        b.let_("rstep", b.app(b.loc("other"), b.unit())),
                        ref_set(b, b.loc("st"), b.ok(b.loc("rstep"))),
                    ],
                    run_right(b.loc("rstep")),
                ),
            ),
        )],
    );
    let step = b.lam(
        &["now", "inp"],
        b.match_(
            ref_get(b, b.loc("st")),
            vec![
                arm(b.err_pat("lstep"), on_left),
                arm(b.ok_pat("rstep"), run_right(b.loc("rstep"))),
            ],
        ),
    );
    b.lam(
        &["other", "self"],
        b.lam(
            &["_s"],
            b.block(
                vec![b.let_("st", ref_make(b, b.err(b.app(b.loc("self"), b.unit()))))],
                step,
            ),
        ),
    )
}

pub(super) fn combined_template(b: &ExprBuilder, is_max: bool) -> Expr {
    let pick_fn = if is_max { "max" } else { "min" };
    let update_best = b.if_(
        b.not(b.var("has")),
        b.block(
            vec![b.assign("best", b.loc("d")), b.assign("has", b.bool_(true))],
            b.unit(),
        ),
        b.block(
            vec![b.assign(
                "best",
                b.app2(b.global("Duration", pick_fn), b.loc("d"), b.var("best")),
            )],
            b.unit(),
        ),
    );
    let on_step = b.match_(
        b.app2(b.loc("s"), b.loc("now"), b.loc("inp")),
        vec![arm(
            Pat::Tuple(vec![
                Pat::Wild,
                Pat::Bind(b.name("d")),
                Pat::Bind(b.name("done")),
            ]),
            if is_max {
                b.if_(
                    b.loc("done"),
                    b.block(vec![b.assign("stopped", b.bool_(true))], b.unit()),
                    update_best,
                )
            } else {
                b.if_(b.loc("done"), b.unit(), update_best)
            },
        )],
    );
    let finish = if is_max {
        b.if_(
            b.bin(BinOp::Or, b.var("stopped"), b.not(b.var("has"))),
            b.tuple(vec![
                b.global("Duration", "zero"),
                b.global("Duration", "zero"),
                b.bool_(true),
            ]),
            b.tuple(vec![b.var("best"), b.var("best"), b.bool_(false)]),
        )
    } else {
        b.if_(
            b.not(b.var("has")),
            b.tuple(vec![
                b.global("Duration", "zero"),
                b.global("Duration", "zero"),
                b.bool_(true),
            ]),
            b.tuple(vec![b.var("best"), b.var("best"), b.bool_(false)]),
        )
    };
    let step = b.lam(
        &["now", "inp"],
        b.block(
            vec![
                b.var_decl("stopped", b.bool_(false)),
                b.var_decl("has", b.bool_(false)),
                b.var_decl("best", b.global("Duration", "zero")),
                Stmt::Expr(b.each("s", b.loc("steps"), on_step)),
            ],
            finish,
        ),
    );
    let start = b.lam(
        &["_s"],
        b.block(
            vec![b.let_(
                "steps",
                b.gen_strict(b.each(
                    "sc",
                    b.app(b.global("NonEmpty", "to_list"), b.loc("all")),
                    b.yield_(b.app(b.loc("sc"), b.unit())),
                )),
            )],
            step,
        ),
    );
    b.lam(&["all"], start)
}

pub(super) fn cron_template(b: &ExprBuilder) -> Expr {
    let die_missing = || {
        b.app(
            b.global("Effect.Core", "die"),
            b.at(ExprKind::Lit(Lit::String(
                "Unable to find cron date".into(),
            ))),
        )
    };
    let check_next = b.block(
        vec![
            b.let_(
                "ms1",
                b.app(b.global("DateTime", "to_epoch_millis"), b.loc("n1")),
            ),
            b.let_(
                "ms2",
                b.if_(
                    zero_millis(b, fsub(b, b.loc("ms1"), b.loc("now"))),
                    b.match_(
                        b.app2(b.global("Cron", "next"), b.loc("n1"), b.loc("cr")),
                        vec![
                            arm(
                                b.some_pat("n2"),
                                b.app(b.global("DateTime", "to_epoch_millis"), b.loc("n2")),
                            ),
                            arm(Pat::Wild, die_missing()),
                        ],
                    ),
                    b.loc("ms1"),
                ),
            ),
            b.let_("d", from_millis(b, fsub(b, b.loc("ms2"), b.loc("now")))),
        ],
        b.tuple(vec![b.loc("d"), b.loc("d"), b.bool_(false)]),
    );
    let finite_step = b.block(
        vec![b.let_(
            "dt",
            b.app(
                b.global("DateTime", "unsafe_from_epoch_millis"),
                b.loc("now"),
            ),
        )],
        b.match_(
            b.app2(b.global("Cron", "next"), b.loc("dt"), b.loc("cr")),
            vec![
                arm(b.some_pat("n1"), check_next),
                arm(Pat::Wild, die_missing()),
            ],
        ),
    );
    let step = b.lam(
        &["now", "_inp"],
        b.if_(
            b.not(b.app(b.global("Float", "is_finite"), b.loc("now"))),
            b.tuple(vec![
                b.global("Duration", "zero"),
                b.global("Duration", "zero"),
                b.bool_(true),
            ]),
            finite_step,
        ),
    );
    b.lam(&["cr"], b.lam(&["_s"], step))
}
