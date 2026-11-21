//! Runtime helper templates for `Sink` core operations (`Helper.sk_*`).

use flick_ir::{Arith, BinOp, Num};

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat, Span};

pub(in super::super) fn sink_core_helper(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$skc");
    match name {
        "sk_drain" => Some(sk_drain_helper(&b)),
        "sk_from_queue" => Some(sk_from_queue_helper(&b)),
        "sk_from_pubsub" => Some(sk_from_pubsub_helper(&b)),
        "sk_make" => Some(sk_make_helper(&b)),
        "sk_flat_map" => Some(sk_flat_map_helper(&b)),
        "sk_summarized" => Some(sk_summarized_helper(&b)),
        "sk_with_duration" => Some(sk_with_duration_helper(&b)),
        "sk_catch_cause" => Some(sk_catch_cause_helper(&b)),
        "sk_on_exit" => Some(sk_on_exit_helper(&b)),
        _ => None,
    }
}

fn sk_drain_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["up", "_own"],
        b.lam(
            &["_u"],
            b.loop_ctrl(
                "sd",
                b.match_(
                    b.app(b.loc("up"), b.unit()),
                    vec![
                        arm(b.ok_pat("_c"), b.unit()),
                        arm(
                            b.err_pat("_d"),
                            b.break_("sd", b.err(b.tuple(vec![b.unit(), b.none()]))),
                        ),
                    ],
                ),
            ),
        ),
    )
}

fn sk_from_queue_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["q", "up", "_own"],
        b.lam(
            &["_u"],
            b.loop_ctrl(
                "sq",
                b.match_(
                    b.app(b.loc("up"), b.unit()),
                    vec![
                        arm(
                            b.ok_pat("xs"),
                            b.block(
                                vec![b.let_(
                                    "_o",
                                    b.app2(b.global("Queue", "offer_all"), b.loc("xs"), b.loc("q")),
                                )],
                                b.unit(),
                            ),
                        ),
                        arm(
                            b.err_pat("_d"),
                            b.block(
                                vec![b.let_("_e", b.app(b.global("Queue", "end"), b.loc("q")))],
                                b.break_("sq", b.err(b.tuple(vec![b.unit(), b.none()]))),
                            ),
                        ),
                    ],
                ),
            ),
        ),
    )
}

fn sk_from_pubsub_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["hub", "up", "_own"],
        b.lam(
            &["_u"],
            b.loop_ctrl(
                "sp",
                b.match_(
                    b.app(b.loc("up"), b.unit()),
                    vec![
                        arm(
                            b.ok_pat("xs"),
                            b.block(
                                vec![b.let_(
                                    "_p",
                                    b.app2(
                                        b.global("PubSub", "publish_all"),
                                        b.loc("xs"),
                                        b.loc("hub"),
                                    ),
                                )],
                                b.unit(),
                            ),
                        ),
                        arm(
                            b.err_pat("_d"),
                            b.break_("sp", b.err(b.tuple(vec![b.unit(), b.none()]))),
                        ),
                    ],
                ),
            ),
        ),
    )
}

fn sk_make_helper(b: &ExprBuilder) -> Expr {
    // f : Stream none none i ->{r; e} a
    b.lam(
        &["f", "up", "_own"],
        b.lam(
            &["_u"],
            b.block(
                vec![
                    b.let_("st", b.lam(&["_u2", "_o2"], b.loc("up"))),
                    b.let_("val", b.app(b.loc("f"), b.loc("st"))),
                ],
                b.err(b.tuple(vec![b.loc("val"), b.none()])),
            ),
        ),
    )
}

fn sk_flat_map_helper(b: &ExprBuilder) -> Expr {
    let run_sink = |pull_name: &str| {
        b.loop_ctrl(
            "sfm",
            b.match_(
                b.app(b.loc(pull_name), b.unit()),
                vec![
                    arm(b.ok_pat("_o"), b.unit()),
                    arm(b.err_pat("end"), b.break_("sfm", b.loc("end"))),
                ],
            ),
        )
    };
    b.lam(
        &["f", "self", "up", "own"],
        b.lam(
            &["_u"],
            b.block(
                vec![b.let_("p1", b.app2(b.loc("self"), b.loc("up"), b.loc("own")))],
                b.match_(
                    run_sink("p1"),
                    vec![arm(
                        pair_pat(b, "a", "lo1"),
                        b.block(
                            vec![
                                b.let_("lor", b.rmake(b.loc("lo1"))),
                                b.let_(
                                    "up2",
                                    b.lam(
                                        &["_u2"],
                                        b.match_(
                                            b.rget(b.loc("lor")),
                                            vec![
                                                arm(
                                                    b.some_pat("xs"),
                                                    b.block(
                                                        vec![b.rset(b.loc("lor"), b.none())],
                                                        b.ok(b.loc("xs")),
                                                    ),
                                                ),
                                                arm(Pat::Wild, b.app(b.loc("up"), b.unit())),
                                            ],
                                        ),
                                    ),
                                ),
                                b.let_(
                                    "p2",
                                    b.app2(
                                        b.app(b.loc("f"), b.loc("a")),
                                        b.loc("up2"),
                                        b.loc("own"),
                                    ),
                                ),
                            ],
                            b.err(b.loop_ctrl(
                                "sfm2",
                                b.match_(
                                    b.app(b.loc("p2"), b.unit()),
                                    vec![
                                        arm(b.ok_pat("_o"), b.unit()),
                                        arm(b.err_pat("end2"), b.break_("sfm2", b.loc("end2"))),
                                    ],
                                ),
                            )),
                        ),
                    )],
                ),
            ),
        ),
    )
}

fn sk_summarized_helper(b: &ExprBuilder) -> Expr {
    // (summary_task, combine_fn, self) -> Sink
    b.lam(
        &["sum_t", "comb", "self", "up", "own"],
        b.lam(
            &["_u"],
            b.block(
                vec![
                    b.let_("s0", b.await_(b.loc("sum_t"))),
                    b.let_("p", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
                ],
                b.match_(
                    b.loop_ctrl(
                        "ssm",
                        b.match_(
                            b.app(b.loc("p"), b.unit()),
                            vec![
                                arm(b.ok_pat("_o"), b.unit()),
                                arm(b.err_pat("end"), b.break_("ssm", b.loc("end"))),
                            ],
                        ),
                    ),
                    vec![arm(
                        pair_pat(b, "a", "lo"),
                        b.block(
                            vec![b.let_("s1", b.await_(b.loc("sum_t")))],
                            b.err(b.tuple(vec![
                                b.tuple(vec![
                                    b.loc("a"),
                                    b.app2(b.loc("comb"), b.loc("s0"), b.loc("s1")),
                                ]),
                                b.loc("lo"),
                            ])),
                        ),
                    )],
                ),
            ),
        ),
    )
}

fn sk_with_duration_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["self"],
        b.app3(
            rt(b, "Helper.sk_summarized"),
            b.task(b.app(rt(b, "Host.CurrentTimeMillis"), b.unit())),
            b.lam(
                &["t0", "t1"],
                b.match_(
                    b.app(
                        rt(b, "Duration.Millis"),
                        b.bin(
                            BinOp::Arith(Arith::Sub, Num::Float),
                            b.loc("t1"),
                            b.loc("t0"),
                        ),
                    ),
                    vec![
                        arm(b.ok_pat("d"), b.loc("d")),
                        arm(Pat::Wild, b.global("Duration", "zero")),
                    ],
                ),
            ),
            b.loc("self"),
        ),
    )
}

fn sk_catch_cause_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["f", "self"],
        b.app2(
            rt(b, "Helper.ch_catch_cause"),
            b.lam(
                &["c", "_up", "_own"],
                b.lam(
                    &["_u"],
                    b.err(b.tuple(vec![b.app(b.loc("f"), b.loc("c")), b.none()])),
                ),
            ),
            b.loc("self"),
        ),
    )
}

fn sk_on_exit_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["f", "self"],
        b.app2(
            rt(b, "Helper.ch_on_exit"),
            b.lam(
                &["ex"],
                b.match_(
                    b.app(b.global("Exit", "value"), b.loc("ex")),
                    vec![
                        arm(
                            b.some_pat("end"),
                            b.match_(
                                b.loc("end"),
                                vec![arm(
                                    pair_pat(b, "a", "_"),
                                    b.app(
                                        b.loc("f"),
                                        b.app(b.global("Exit", "succeed"), b.loc("a")),
                                    ),
                                )],
                            ),
                        ),
                        arm(Pat::Wild, b.app(b.loc("f"), b.loc("ex"))),
                    ],
                ),
            ),
            b.loc("self"),
        ),
    )
}
