//! Audited `Tracer` pure IR adapter + shared span helpers (Stage 4D).

mod span;

use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::std_observe_refs::{
    KEY_MIN_TRACE_LEVEL, KEY_PARENT_SPAN, KEY_TRACE_LEVEL, KEY_TRACER, obs_read, obs_with,
};
use super::{Code, Env, Expr, ExprKind, Local, Lower, LowerError, Pat, Span};

pub(super) use span::{end_span_expr, open_span_expr, with_parent_span_expr};

impl Lower<'_> {
    pub(super) fn std_observe_trace_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        if value.binding.from()? != "flick:Tracer" {
            return None;
        }
        let b = ExprBuilder::new(span, "$tr");
        let expr = tracer_template(
            &b,
            value.binding.export_name().unwrap_or(""),
            value.mode,
            value.stages.len(),
        )?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

pub(super) fn sp_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&[
        "sname", "sid", "stid", "ssamp", "spar", "sext", "skind", "snoop", "sunp", "sstart", "sst",
        "scst",
    ])
}

pub(super) fn sst_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["ended", "end_ns", "attrs", "links", "events"])
}

fn external_span_template(b: &ExprBuilder) -> Expr {
    let empty_state = b.rmake(b.tuple([
        b.bool_(false),
        b.bigint("0"),
        b.list(vec![]),
        b.list(vec![]),
        b.list(vec![]),
    ]));
    b.lam(
        &["o"],
        b.tuple([
            b.none(),
            b.field(b.loc("o"), "spanId"),
            b.field(b.loc("o"), "traceId"),
            b.field(b.loc("o"), "sampled"),
            b.none(),
            b.bool_(true),
            b.none(),
            b.bool_(false),
            b.bool_(false),
            b.bigint("0"),
            empty_state,
            b.bool_(false),
        ]),
    )
}

fn span_status_template(b: &ExprBuilder) -> Expr {
    let started_con = b.at(ExprKind::Con(ExprBuilder::con(
        "Tracer",
        "SpanStatus",
        "Started",
        1,
    )));
    let ended_con = b.at(ExprKind::Con(ExprBuilder::con(
        "Tracer",
        "SpanStatus",
        "Ended",
        2,
    )));
    let ext_status = b.app(started_con.clone(), b.bigint("0"));
    let local_status = b.match_(
        b.rget(b.loc("sst")),
        [arm(
            sst_pat(b),
            b.if_(
                b.loc("ended"),
                b.app2(ended_con, b.loc("sstart"), b.loc("end_ns")),
                b.app(started_con, b.loc("sstart")),
            ),
        )],
    );
    b.lam(
        &["s"],
        b.match_(
            b.loc("s"),
            [arm(
                sp_pat(b),
                b.if_(b.loc("sext"), ext_status, local_status),
            )],
        ),
    )
}

fn tracer_template(b: &ExprBuilder, bind: &str, mode: ForeignMode, stages: usize) -> Option<Expr> {
    let field =
        |name: &'static str| b.lam(&["s"], b.match_(b.loc("s"), [arm(sp_pat(b), b.loc(name))]));
    match (bind, mode, stages) {
        ("nativeTracer", ForeignMode::Pure, 0) => Some(b.tuple([b.str_("native")])),
        ("flickExternalSpan", ForeignMode::Pure, 1) => Some(external_span_template(b)),
        ("flickSpanId", ForeignMode::Pure, 1) => Some(field("sid")),
        ("flickTraceId", ForeignMode::Pure, 1) => Some(field("stid")),
        ("flickSampled", ForeignMode::Pure, 1) => Some(field("ssamp")),
        ("flickSpanName", ForeignMode::Pure, 1) => Some(field("sname")),
        ("flickSpanKind", ForeignMode::Pure, 1) => Some(field("skind")),
        ("flickParent", ForeignMode::Pure, 1) => Some(field("spar")),
        ("flickLinks", ForeignMode::Pure, 1) => {
            let local_links = b.match_(b.rget(b.loc("sst")), [arm(sst_pat(b), b.loc("links"))]);
            Some(b.lam(
                &["s"],
                b.match_(
                    b.loc("s"),
                    [arm(
                        sp_pat(b),
                        b.if_(b.loc("sext"), b.list(vec![]), local_links),
                    )],
                ),
            ))
        }
        ("flickStatus", ForeignMode::Sync | ForeignMode::Pure, 1) => Some(span_status_template(b)),
        ("flickAttributes", ForeignMode::Sync | ForeignMode::Pure, 1) => {
            let local_attrs = b.match_(b.rget(b.loc("sst")), [arm(sst_pat(b), b.loc("attrs"))]);
            Some(b.lam(&["s"], b.match_(b.loc("s"), [arm(sp_pat(b), local_attrs)])))
        }
        ("flickParentSpan", ForeignMode::Effect, 1) => {
            Some(b.lam(&["_u"], obs_read(b, KEY_PARENT_SPAN, b.none())))
        }
        ("flickWithSpan", ForeignMode::Effect, 3) => Some(b.lam(
            &["nm", "opts", "t"],
            b.block(
                [b.let_("sp", open_span_expr(b, b.loc("nm"), b.loc("opts")))],
                b.app2(
                    b.global("Effect.Errors", "ensuring"),
                    b.task(end_span_expr(b, b.loc("sp"))),
                    b.task(with_parent_span_expr(b, b.loc("sp"), b.loc("t"))),
                ),
            ),
        )),
        ("flickWithTracer", ForeignMode::Effect, 2) => Some(b.lam(
            &["tr", "t"],
            obs_with(
                b,
                KEY_TRACER,
                b.tuple([b.str_("native")]),
                b.loc("tr"),
                b.loc("t"),
            ),
        )),
        ("flickWithTraceLevel", ForeignMode::Effect, 2) => Some(b.lam(
            &["lvl", "t"],
            obs_with(b, KEY_TRACE_LEVEL, b.str_("Info"), b.loc("lvl"), b.loc("t")),
        )),
        ("flickWithMinimumTraceLevel", ForeignMode::Effect, 2) => Some(b.lam(
            &["lvl", "t"],
            obs_with(
                b,
                KEY_MIN_TRACE_LEVEL,
                b.str_("All"),
                b.loc("lvl"),
                b.loc("t"),
            ),
        )),
        _ => None,
    }
}
