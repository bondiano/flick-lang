//! Audited `Layer.Trace` and `Effect.Observe` span pure IR adapters (Stage 4D).

use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::std_observe_refs::{
    KEY_PARENT_SPAN, KEY_SPAN_ANNOTATIONS, KEY_SPAN_LINKS, KEY_TRACER, obs_read, obs_ref, obs_with,
    upsert_pair_list,
};
use super::std_observe_trace::{
    end_span_expr, open_span_expr, sp_pat, sst_pat, with_parent_span_expr,
};
use super::{Code, Env, Expr, ExprKind, Local, Lower, LowerError, Pat, Span, Stmt};

impl Lower<'_> {
    pub(super) fn std_observe_span_foreign(
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
        let b = ExprBuilder::new(span, "$spn");
        let expr = match from {
            "flick:Layer" => layer_trace_template(&b, bind, value.mode, stages)?,
            "flick:Effect" => effect_span_template(&b, bind, value.mode, stages)?,
            _ => return None,
        };
        Some(self.expr(&expr, &Env::new(), block))
    }
}

fn ending_scoped_span(b: &ExprBuilder, name: Expr, layer_opts: Expr) -> Expr {
    b.block(
        vec![
            b.let_("lo", layer_opts),
            b.let_("sp", open_span_expr(b, name, b.field(b.loc("lo"), "span"))),
            Stmt::Expr(b.app(
                b.global("Scope", "add_finalizer_exit"),
                b.lam(
                    &["opt_c"],
                    b.block(
                        vec![Stmt::Expr(end_span_expr(b, b.loc("sp")))],
                        b.match_(
                            b.field(b.loc("lo"), "onEnd"),
                            vec![
                                arm(
                                    b.some_pat("cb"),
                                    b.app2(b.loc("cb"), b.loc("sp"), b.loc("opt_c")),
                                ),
                                arm(Pat::Wild, b.unit()),
                            ],
                        ),
                    ),
                ),
            )),
        ],
        b.loc("sp"),
    )
}

fn layer_trace_template(
    b: &ExprBuilder,
    bind: &str,
    mode: ForeignMode,
    stages: usize,
) -> Option<Expr> {
    if mode != ForeignMode::Pure {
        return None;
    }
    match (bind, stages) {
        ("flickSpan", 2) => Some(b.lam(
            &["nm", "lo"],
            b.app(
                b.global("Layer", "from_build"),
                b.lam(
                    &["_m"],
                    b.app2(
                        b.global("Context", "make"),
                        obs_ref(b, KEY_PARENT_SPAN, b.none()),
                        b.some(ending_scoped_span(b, b.loc("nm"), b.loc("lo"))),
                    ),
                ),
            ),
        )),
        ("parentSpan", 1) => Some(b.lam(
            &["sp"],
            b.app(
                b.global("Layer", "from_build"),
                b.lam(
                    &["_m"],
                    b.app2(
                        b.global("Context", "make"),
                        obs_ref(b, KEY_PARENT_SPAN, b.none()),
                        b.some(b.loc("sp")),
                    ),
                ),
            ),
        )),
        ("flickWithSpan", 3) => Some(b.lam(
            &["nm", "lo", "self_l"],
            b.app(
                b.global("Layer", "from_build"),
                b.lam(
                    &["memo"],
                    b.block(
                        vec![b.let_("sp", ending_scoped_span(b, b.loc("nm"), b.loc("lo")))],
                        obs_with(
                            b,
                            KEY_PARENT_SPAN,
                            b.none(),
                            b.some(b.loc("sp")),
                            b.task(b.app2(
                                b.global("Layer", "build_with_memo_map"),
                                b.loc("memo"),
                                b.loc("self_l"),
                            )),
                        ),
                    ),
                ),
            ),
        )),
        ("withParentSpan", 2) => Some(b.lam(
            &["sp", "self_l"],
            b.app(
                b.global("Layer", "from_build"),
                b.lam(
                    &["memo"],
                    obs_with(
                        b,
                        KEY_PARENT_SPAN,
                        b.none(),
                        b.some(b.loc("sp")),
                        b.task(b.app2(
                            b.global("Layer", "build_with_memo_map"),
                            b.loc("memo"),
                            b.loc("self_l"),
                        )),
                    ),
                ),
            ),
        )),
        _ => None,
    }
}

fn no_such_element(b: &ExprBuilder) -> Expr {
    b.app(
        b.global("Fail", "fail"),
        b.app(
            b.at(ExprKind::Con(ExprBuilder::con(
                "Cause",
                "NoSuchElementError",
                "NoSuchElementError",
                1,
            ))),
            b.str_(""),
        ),
    )
}

fn annotate_spans_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["k", "v", "t"],
        b.block(
            vec![b.let_(
                "next",
                upsert_pair_list(
                    b,
                    obs_read(b, KEY_SPAN_ANNOTATIONS, b.list(vec![])),
                    b.loc("k"),
                    b.loc("v"),
                ),
            )],
            obs_with(
                b,
                KEY_SPAN_ANNOTATIONS,
                b.list(vec![]),
                b.loc("next"),
                b.loc("t"),
            ),
        ),
    )
}

fn annotate_current_span_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["k", "v"],
        b.match_(
            obs_read(b, KEY_PARENT_SPAN, b.none()),
            vec![
                arm(
                    b.some_pat("sp"),
                    b.match_(
                        b.loc("sp"),
                        vec![arm(
                            sp_pat(b),
                            b.if_(
                                b.bin(BinOp::Or, b.loc("sext"), b.loc("snoop")),
                                b.unit(),
                                b.match_(
                                    b.rget(b.loc("sst")),
                                    vec![arm(
                                        sst_pat(b),
                                        b.block(
                                            vec![b.rset(
                                                b.loc("sst"),
                                                b.tuple(vec![
                                                    b.loc("ended"),
                                                    b.loc("end_ns"),
                                                    upsert_pair_list(
                                                        b,
                                                        b.loc("attrs"),
                                                        b.loc("k"),
                                                        b.loc("v"),
                                                    ),
                                                    b.loc("links"),
                                                    b.loc("events"),
                                                ]),
                                            )],
                                            b.unit(),
                                        ),
                                    )],
                                ),
                            ),
                        )],
                    ),
                ),
                arm(Pat::Wild, b.unit()),
            ],
        ),
    )
}

fn current_span_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["_u"],
        b.match_(
            obs_read(b, KEY_PARENT_SPAN, b.none()),
            vec![
                arm(
                    b.some_pat("sp"),
                    b.match_(
                        b.loc("sp"),
                        vec![arm(
                            sp_pat(b),
                            b.if_(b.loc("sext"), no_such_element(b), b.loc("sp")),
                        )],
                    ),
                ),
                arm(Pat::Wild, no_such_element(b)),
            ],
        ),
    )
}

fn current_parent_span_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["_u"],
        b.match_(
            obs_read(b, KEY_PARENT_SPAN, b.none()),
            vec![
                arm(b.some_pat("sp"), b.loc("sp")),
                arm(Pat::Wild, no_such_element(b)),
            ],
        ),
    )
}

fn link_spans_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["sps", "attrs", "t"],
        b.block(
            vec![
                b.let_(
                    "added",
                    b.app2(
                        b.global("List", "map"),
                        b.lam(
                            &["sp"],
                            b.at(ExprKind::Record(vec![
                                ("span".into(), b.loc("sp")),
                                ("attributes".into(), b.loc("attrs")),
                            ])),
                        ),
                        b.loc("sps"),
                    ),
                ),
                b.let_(
                    "next",
                    b.app2(
                        b.global("List.Core", "append"),
                        obs_read(b, KEY_SPAN_LINKS, b.list(vec![])),
                        b.loc("added"),
                    ),
                ),
            ],
            obs_with(b, KEY_SPAN_LINKS, b.list(vec![]), b.loc("next"), b.loc("t")),
        ),
    )
}

fn make_span_scoped_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["nm", "opts"],
        b.block(
            vec![
                b.let_("sp", open_span_expr(b, b.loc("nm"), b.loc("opts"))),
                Stmt::Expr(b.app(
                    b.global("Scope", "add_finalizer"),
                    b.task(end_span_expr(b, b.loc("sp"))),
                )),
            ],
            b.loc("sp"),
        ),
    )
}

fn use_span_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["nm", "opts", "f"],
        b.block(
            vec![b.let_("sp", open_span_expr(b, b.loc("nm"), b.loc("opts")))],
            b.app2(
                b.global("Effect.Errors", "ensuring"),
                b.task(end_span_expr(b, b.loc("sp"))),
                b.task(b.app(b.loc("f"), b.loc("sp"))),
            ),
        ),
    )
}

fn with_span_scoped_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["nm", "opts", "t"],
        b.block(
            vec![b.let_(
                "sp",
                b.app2(
                    b.global("Effect.Observe", "make_span_scoped"),
                    b.loc("nm"),
                    b.loc("opts"),
                ),
            )],
            with_parent_span_expr(b, b.loc("sp"), b.loc("t")),
        ),
    )
}

fn effect_span_template(
    b: &ExprBuilder,
    bind: &str,
    mode: ForeignMode,
    stages: usize,
) -> Option<Expr> {
    if mode != ForeignMode::Effect {
        return None;
    }
    match (bind, stages) {
        ("flickTracer", 1) => Some(b.lam(
            &["_u"],
            obs_read(b, KEY_TRACER, b.tuple(vec![b.str_("native")])),
        )),
        ("annotateSpans", 3) => Some(annotate_spans_template(b)),
        ("annotateCurrentSpan", 2) => Some(annotate_current_span_template(b)),
        ("flickCurrentSpan", 1) => Some(current_span_template(b)),
        ("flickCurrentParentSpan", 1) => Some(current_parent_span_template(b)),
        ("flickSpanLinksNow", 1) => {
            Some(b.lam(&["_u"], obs_read(b, KEY_SPAN_LINKS, b.list(vec![]))))
        }
        ("flickLinkSpans", 3) => Some(link_spans_template(b)),
        ("flickMakeSpan", 2) => Some(b.lam(
            &["nm", "opts"],
            open_span_expr(b, b.loc("nm"), b.loc("opts")),
        )),
        ("flickMakeSpanScoped", 2) => Some(make_span_scoped_template(b)),
        ("flickUseSpan", 3) => Some(use_span_template(b)),
        ("flickWithSpanScoped", 3) => Some(with_span_scoped_template(b)),
        ("withParentSpan", 2) => Some(b.lam(
            &["sp", "t"],
            with_parent_span_expr(b, b.loc("sp"), b.loc("t")),
        )),
        _ => None,
    }
}
