//! Audited `Sink` constructors & combinators pure IR adapters (Stage 4E).
//!
//! A `Sink r e i l a` is represented directly as a `Channel` reading `NonEmpty i`
//! chunks and ending with `End a l = (a, Option (NonEmpty l))`.

mod helpers;

use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::std_schema_rt::{pair_pat, rt};
use super::{Code, Env, Expr, Local, Lower, LowerError, Pat, Span};

pub(super) use helpers::sink_core_helper;

impl Lower<'_> {
    pub(super) fn std_sink_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || value.mode != ForeignMode::Pure || !has_standard_shape(value) {
            return None;
        }
        if value.binding.from.as_deref()? != "flick:Sink" {
            return None;
        }
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$sk");
        let expr = sink_core_template(&b, bind, stages)
            .or_else(|| super::std_sink_folds::sink_folds_template(&b, bind, stages))?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

fn sink_core_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    sink_constructors_template(b, bind, stages)
        .or_else(|| sink_map_template(b, bind, stages))
        .or_else(|| sink_control_template(b, bind, stages))
}

fn sink_map_val(b: &ExprBuilder, f_expr: Expr, self_expr: Expr) -> Expr {
    b.app2(
        rt(b, "Helper.ch_map_done"),
        b.lam(
            &["end"],
            b.match_(
                b.loc("end"),
                vec![arm(
                    pair_pat(b, "a", "lo"),
                    b.tuple(vec![b.app(f_expr, b.loc("a")), b.loc("lo")]),
                )],
            ),
        ),
        self_expr,
    )
}

fn sink_constructors_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickFromChannel" | "flickToChannel", 1) => Some(b.lam(&["x"], b.loc("x"))),
        ("flickMake", 1) => Some(rt(b, "Helper.sk_make")),
        ("flickFromEffectEnd", 1) => Some(b.lam(
            &["t", "_up", "_own"],
            b.lam(&["_u"], b.err(b.await_(b.loc("t")))),
        )),
        ("fromEffect", 1) => Some(b.lam(
            &["t", "_up", "_own"],
            b.lam(
                &["_u"],
                b.err(b.tuple(vec![b.await_(b.loc("t")), b.none()])),
            ),
        )),
        ("fromQueue", 1) => Some(rt(b, "Helper.sk_from_queue")),
        ("fromPubSub", 1) => Some(rt(b, "Helper.sk_from_pubsub")),
        ("succeed", 1) => Some(b.lam(
            &["v", "_up", "_own"],
            b.lam(&["_u"], b.err(b.tuple(vec![b.loc("v"), b.none()]))),
        )),
        ("sync", 1) => Some(b.lam(
            &["f", "_up", "_own"],
            b.lam(
                &["_u"],
                b.err(b.tuple(vec![b.app(b.loc("f"), b.unit()), b.none()])),
            ),
        )),
        ("suspend", 1) => Some(b.lam(
            &["f", "up", "own"],
            b.app2(b.app(b.loc("f"), b.unit()), b.loc("up"), b.loc("own")),
        )),
        ("fail", 1) => Some(b.lam(
            &["e", "_up", "_own"],
            b.lam(&["_u"], b.app(b.global("Fail", "fail"), b.loc("e"))),
        )),
        ("failSync", 1) => Some(b.lam(
            &["f", "_up", "_own"],
            b.lam(
                &["_u"],
                b.app(b.global("Fail", "fail"), b.app(b.loc("f"), b.unit())),
            ),
        )),
        ("flickFailCause", 1) => Some(b.lam(
            &["c", "_up", "_own"],
            b.lam(&["_u"], b.app(b.global("Failure", "rethrow"), b.loc("c"))),
        )),
        ("flickFailCauseSync", 1) => Some(b.lam(
            &["f", "_up", "_own"],
            b.lam(
                &["_u"],
                b.app(b.global("Failure", "rethrow"), b.app(b.loc("f"), b.unit())),
            ),
        )),
        ("never", 0) => Some(b.lam(
            &["_up", "_own"],
            b.lam(&["_u"], b.app(rt(b, "Async.EffectNever"), b.unit())),
        )),
        ("drain", 0) => Some(rt(b, "Helper.sk_drain")),
        _ => None,
    }
}

fn sink_map_leftover(b: &ExprBuilder) -> Expr {
    b.lam(
        &["f", "self"],
        b.app2(
            rt(b, "Helper.ch_map_done"),
            b.lam(
                &["end"],
                b.match_(
                    b.loc("end"),
                    vec![arm(
                        pair_pat(b, "a", "lo"),
                        b.tuple(vec![
                            b.loc("a"),
                            b.match_(
                                b.loc("lo"),
                                vec![
                                    arm(
                                        b.some_pat("xs"),
                                        b.some(b.gen_strict(b.each(
                                            "x",
                                            b.loc("xs"),
                                            b.yield_(b.app(b.loc("f"), b.loc("x"))),
                                        ))),
                                    ),
                                    arm(Pat::Wild, b.none()),
                                ],
                            ),
                        ]),
                    )],
                ),
            ),
            b.loc("self"),
        ),
    )
}

fn sink_map_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("ignoreLeftover", 1) => Some(b.lam(
            &["self"],
            b.app2(
                rt(b, "Helper.ch_map_done"),
                b.lam(
                    &["end"],
                    b.match_(
                        b.loc("end"),
                        vec![arm(
                            pair_pat(b, "a", "_"),
                            b.tuple(vec![b.loc("a"), b.none()]),
                        )],
                    ),
                ),
                b.loc("self"),
            ),
        )),
        ("map" | "mapEffect", 2) => {
            Some(b.lam(&["f", "self"], sink_map_val(b, b.loc("f"), b.loc("self"))))
        }
        ("as", 2) => Some(b.lam(
            &["v", "self"],
            sink_map_val(b, b.lam(&["_a"], b.loc("v")), b.loc("self")),
        )),
        ("mapInput" | "mapInputEffect", 2) => Some(b.lam(
            &["f", "self"],
            b.app2(
                rt(b, "Helper.ch_map_input"),
                b.lam(
                    &["chunk"],
                    b.gen_strict(b.each(
                        "x",
                        b.loc("chunk"),
                        b.yield_(b.app(b.loc("f"), b.loc("x"))),
                    )),
                ),
                b.loc("self"),
            ),
        )),
        ("mapInputArray" | "mapInputArrayEffect", 2) => Some(rt(b, "Helper.ch_map_input")),
        ("flickMapEnd" | "flickMapEffectEnd", 2) => Some(rt(b, "Helper.ch_map_done")),
        ("mapError", 2) => Some(b.lam(
            &["f", "self"],
            b.app2(
                rt(b, "Helper.ch_catch"),
                b.lam(
                    &["e", "_up", "_own"],
                    b.lam(
                        &["_u"],
                        b.app(b.global("Fail", "fail"), b.app(b.loc("f"), b.loc("e"))),
                    ),
                ),
                b.loc("self"),
            ),
        )),
        ("mapLeftover", 2) => Some(sink_map_leftover(b)),
        _ => None,
    }
}

fn sink_control_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flatMap", 2) => Some(rt(b, "Helper.sk_flat_map")),
        ("unwrap", 1) => Some(rt(b, "Helper.ch_unwrap")),
        ("summarized", 3) => Some(rt(b, "Helper.sk_summarized")),
        ("withDuration", 1) => Some(rt(b, "Helper.sk_with_duration")),
        ("timed", 0) => Some(b.app2(
            rt(b, "Helper.ch_map_done"),
            b.lam(
                &["end"],
                b.match_(
                    b.loc("end"),
                    vec![arm(
                        pair_pat(b, "pair", "lo"),
                        b.match_(
                            b.loc("pair"),
                            vec![arm(
                                pair_pat(b, "_", "dur"),
                                b.tuple(vec![b.loc("dur"), b.loc("lo")]),
                            )],
                        ),
                    )],
                ),
            ),
            b.app(rt(b, "Helper.sk_with_duration"), rt(b, "Helper.sk_drain")),
        )),
        ("provideService", 3) => Some(rt(b, "Helper.ch_provide_service")),
        ("orElse", 2) => Some(rt(b, "Helper.ch_catch")),
        ("catch", 2) => Some(b.lam(
            &["f", "self"],
            b.app2(
                rt(b, "Helper.ch_catch"),
                b.lam(
                    &["e", "_up", "_own"],
                    b.lam(
                        &["_u"],
                        b.err(b.tuple(vec![b.app(b.loc("f"), b.loc("e")), b.none()])),
                    ),
                ),
                b.loc("self"),
            ),
        )),
        ("flickCatchCause", 2) => Some(rt(b, "Helper.sk_catch_cause")),
        ("flickOnExit", 2) => Some(rt(b, "Helper.sk_on_exit")),
        ("ensuring", 2) => Some(b.lam(
            &["fin", "self"],
            b.app2(
                rt(b, "Helper.ch_on_exit"),
                b.lam(&["_ex"], b.await_(b.loc("fin"))),
                b.loc("self"),
            ),
        )),
        _ => None,
    }
}
