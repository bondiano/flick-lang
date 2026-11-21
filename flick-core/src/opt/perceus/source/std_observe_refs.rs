//! Audited `LogLevel` and `References` pure IR adapters + shared
//! observability `Context.Reference` keys (Stage 4D).

use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, Local, Lower, LowerError, Span};

pub(super) const KEY_PARENT_SPAN: &str = "effect/ParentSpan";
pub(super) const KEY_TRACER: &str = "effect/Tracer";
pub(super) const KEY_TRACE_LEVEL: &str = "effect/Tracer/CurrentTraceLevel";
pub(super) const KEY_MIN_TRACE_LEVEL: &str = "effect/Tracer/MinimumTraceLevel";
pub(super) const KEY_DISABLE_PROP: &str = "effect/Tracer/DisablePropagation";
pub(super) const KEY_TRACER_ENABLED: &str = "effect/References/TracerEnabled";
pub(super) const KEY_TRACER_TIMING: &str = "effect/References/TracerTimingEnabled";
pub(super) const KEY_SPAN_ANNOTATIONS: &str = "effect/References/TracerSpanAnnotations";
pub(super) const KEY_SPAN_LINKS: &str = "effect/References/TracerSpanLinks";
pub(super) const KEY_STACK_FRAME: &str = "effect/References/CurrentStackFrame";

pub(super) const KEY_LOGGERS: &str = "effect/Loggers";
pub(super) const KEY_LOG_STDERR: &str = "effect/Logger/LogToStderr";
pub(super) const KEY_CUR_LOG_LEVEL: &str = "effect/References/CurrentLogLevel";
pub(super) const KEY_MIN_LOG_LEVEL: &str = "effect/References/MinimumLogLevel";
pub(super) const KEY_UNHANDLED_LOG: &str = "effect/References/UnhandledLogLevel";
pub(super) const KEY_LOG_ANNOTATIONS: &str = "effect/References/CurrentLogAnnotations";
pub(super) const KEY_LOG_SPANS: &str = "effect/References/CurrentLogSpans";

impl Lower<'_> {
    pub(super) fn std_observe_refs_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        let from = value.binding.from.as_deref()?;
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$obs");
        let expr = match from {
            "flick:LogLevel" => log_level_template(&b, bind, value.mode, stages)?,
            "flick:References" => references_template(&b, bind, value.mode, stages)?,
            _ => return None,
        };
        Some(self.expr(&expr, &Env::new(), block))
    }
}

pub(super) fn obs_ref(b: &ExprBuilder, key: &str, default: Expr) -> Expr {
    b.app2(
        b.global("$rt", "Layer.ContextReference"),
        b.str_(key),
        b.lam(&["_u"], default),
    )
}

pub(super) fn obs_read(b: &ExprBuilder, key: &str, default: Expr) -> Expr {
    b.app(
        b.global("$rt", "Layer.ContextRead"),
        obs_ref(b, key, default),
    )
}

pub(super) fn obs_with(b: &ExprBuilder, key: &str, default: Expr, val: Expr, task: Expr) -> Expr {
    b.app3(
        b.global("$rt", "Layer.ContextWithReference"),
        obs_ref(b, key, default),
        val,
        task,
    )
}

pub(super) fn log_level_ordinal(b: &ExprBuilder, level: Expr) -> Expr {
    // All < Trace < Debug < Info < Warn < Error < Fatal < None
    b.block(
        vec![b.let_("lvl", level)],
        b.if_(
            b.bin(BinOp::Eq, b.loc("lvl"), b.str_("All")),
            b.int(-1),
            b.if_(
                b.bin(BinOp::Eq, b.loc("lvl"), b.str_("Trace")),
                b.int(0),
                b.if_(
                    b.bin(BinOp::Eq, b.loc("lvl"), b.str_("Debug")),
                    b.int(10000),
                    b.if_(
                        b.bin(BinOp::Eq, b.loc("lvl"), b.str_("Info")),
                        b.int(20000),
                        b.if_(
                            b.bin(BinOp::Eq, b.loc("lvl"), b.str_("Warn")),
                            b.int(30000),
                            b.if_(
                                b.bin(BinOp::Eq, b.loc("lvl"), b.str_("Error")),
                                b.int(40000),
                                b.if_(
                                    b.bin(BinOp::Eq, b.loc("lvl"), b.str_("Fatal")),
                                    b.int(50000),
                                    b.int(100_000),
                                ),
                            ),
                        ),
                    ),
                ),
            ),
        ),
    )
}

pub(super) fn upsert_pair_list(b: &ExprBuilder, list: Expr, key: Expr, val: Expr) -> Expr {
    // Replaces existing key in place (preserving order) or appends `(key, val)`.
    let step = b.match_(
        b.loc("p"),
        vec![arm(
            b.tup_pat(&["pk", "_pv"]),
            b.if_(
                b.bin(BinOp::Eq, b.loc("pk"), b.loc("uk")),
                b.block(
                    vec![
                        b.assign("found", b.bool_(true)),
                        b.assign(
                            "out",
                            b.app2(
                                b.global("List.Core", "append"),
                                b.var("out"),
                                b.list(vec![b.tuple(vec![b.loc("uk"), b.loc("uv")])]),
                            ),
                        ),
                    ],
                    b.unit(),
                ),
                b.block(
                    vec![b.assign(
                        "out",
                        b.app2(
                            b.global("List.Core", "append"),
                            b.var("out"),
                            b.list(vec![b.loc("p")]),
                        ),
                    )],
                    b.unit(),
                ),
            ),
        )],
    );
    b.block(
        vec![
            b.let_("uk", key),
            b.let_("uv", val),
            b.var_decl("found", b.bool_(false)),
            b.var_decl("out", b.list(vec![])),
            b.let_("_up", b.for_ctrl("upl", "p", list, step)),
        ],
        b.if_(
            b.var("found"),
            b.var("out"),
            b.app2(
                b.global("List.Core", "append"),
                b.var("out"),
                b.list(vec![b.tuple(vec![b.loc("uk"), b.loc("uv")])]),
            ),
        ),
    )
}

fn log_level_template(
    b: &ExprBuilder,
    bind: &str,
    mode: ForeignMode,
    stages: usize,
) -> Option<Expr> {
    let cmp = |op: BinOp| {
        b.lam(
            &["that", "self"],
            b.bin(
                op,
                log_level_ordinal(b, b.loc("self")),
                log_level_ordinal(b, b.loc("that")),
            ),
        )
    };
    match (bind, mode, stages) {
        ("isGreaterThan", ForeignMode::Pure, 2) => Some(cmp(BinOp::Gt)),
        ("isGreaterThanOrEqualTo", ForeignMode::Pure, 2) => Some(cmp(BinOp::Ge)),
        ("isLessThan", ForeignMode::Pure, 2) => Some(cmp(BinOp::Lt)),
        ("isLessThanOrEqualTo", ForeignMode::Pure, 2) => Some(cmp(BinOp::Le)),
        ("isEnabled", ForeignMode::Effect, 1) => Some(b.lam(
            &["lvl"],
            b.bin(
                BinOp::Le,
                log_level_ordinal(b, obs_read(b, KEY_MIN_LOG_LEVEL, b.str_("Info"))),
                log_level_ordinal(b, b.loc("lvl")),
            ),
        )),
        _ => None,
    }
}

fn references_template(
    b: &ExprBuilder,
    bind: &str,
    mode: ForeignMode,
    stages: usize,
) -> Option<Expr> {
    if mode != ForeignMode::Effect {
        return None;
    }
    match (bind, stages) {
        ("flickCurrentLogLevel", 1) => {
            Some(b.lam(&["_u"], obs_read(b, KEY_CUR_LOG_LEVEL, b.str_("Info"))))
        }
        ("flickWithCurrentLogLevel", 2) => Some(b.lam(
            &["lvl", "t"],
            obs_with(
                b,
                KEY_CUR_LOG_LEVEL,
                b.str_("Info"),
                b.loc("lvl"),
                b.loc("t"),
            ),
        )),
        ("flickWithMinimumLogLevel", 2) => Some(b.lam(
            &["lvl", "t"],
            obs_with(
                b,
                KEY_MIN_LOG_LEVEL,
                b.str_("Info"),
                b.loc("lvl"),
                b.loc("t"),
            ),
        )),
        ("flickWithUnhandledLogLevel", 2) => Some(b.lam(
            &["lvl", "t"],
            obs_with(
                b,
                KEY_UNHANDLED_LOG,
                b.some(b.str_("Error")),
                b.loc("lvl"),
                b.loc("t"),
            ),
        )),
        ("flickCurrentLogAnnotations", 1) => {
            Some(b.lam(&["_u"], obs_read(b, KEY_LOG_ANNOTATIONS, b.list(vec![]))))
        }
        ("flickCurrentLogSpans", 1) => Some(b.lam(
            &["_u"],
            b.app2(
                b.global("List", "map"),
                b.lam(
                    &["sp"],
                    b.match_(
                        b.loc("sp"),
                        vec![arm(b.tup_pat(&["lbl", "_st"]), b.loc("lbl"))],
                    ),
                ),
                obs_read(b, KEY_LOG_SPANS, b.list(vec![])),
            ),
        )),
        ("flickCurrentStackFrame", 1) => {
            Some(b.lam(&["_u"], obs_read(b, KEY_STACK_FRAME, b.none())))
        }
        ("flickWithDisablePropagation", 2) => Some(b.lam(
            &["flag", "t"],
            obs_with(
                b,
                KEY_DISABLE_PROP,
                b.bool_(false),
                b.loc("flag"),
                b.loc("t"),
            ),
        )),
        ("flickWithTracerEnabled", 2) => Some(b.lam(
            &["flag", "t"],
            obs_with(
                b,
                KEY_TRACER_ENABLED,
                b.bool_(true),
                b.loc("flag"),
                b.loc("t"),
            ),
        )),
        ("flickWithTracerTimingEnabled", 2) => Some(b.lam(
            &["flag", "t"],
            obs_with(
                b,
                KEY_TRACER_TIMING,
                b.bool_(true),
                b.loc("flag"),
                b.loc("t"),
            ),
        )),
        ("flickTracerSpanAnnotations", 1) => {
            Some(b.lam(&["_u"], obs_read(b, KEY_SPAN_ANNOTATIONS, b.list(vec![]))))
        }
        _ => None,
    }
}
