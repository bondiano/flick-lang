//! Audited `Logger` and `Effect.Observe` logging pure IR adapters (Stage 4D).

mod format;

use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::ExprBuilder;
use super::lazy_scope::has_standard_shape;
use super::std_observe_refs::{
    KEY_CUR_LOG_LEVEL, KEY_LOG_ANNOTATIONS, KEY_LOG_SPANS, KEY_LOGGERS, KEY_MIN_LOG_LEVEL,
    log_level_ordinal, obs_read, obs_ref, obs_with, upsert_pair_list,
};
use super::{Code, Expr, Local, Lower, LowerError, Pat, Span, Stmt};

impl Lower<'_> {
    pub(super) fn std_observe_log_foreign(
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
        let b = ExprBuilder::new(span, "$lg");
        let expr = match from {
            "flick:Logger" => format::logger_template(&b, bind, value.mode, stages)?,
            "flick:Effect" => effect_log_template(&b, bind, value.mode, stages)?,
            _ => return None,
        };
        Some(self.lower_with_manual_scope(&expr, "$lg", block, span))
    }
}

// LogEntry: `(message : List String, level : String, opt_cause : Option RawCause, date : Float)`
// Logger o: `LogEntry ->{} o`
fn entry_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["emsg", "elvl", "ec", "edate"])
}

fn default_loggers_expr(b: &ExprBuilder) -> Expr {
    b.list([
        b.global("Logger", "default_logger"),
        b.global("Logger", "tracer_logger"),
    ])
}

pub(super) fn emit_log_expr(b: &ExprBuilder, level: Expr, msg: Expr, opt_cause: Expr) -> Expr {
    let run_loggers = b.for_ctrl(
        "elg",
        "lg",
        obs_read(b, KEY_LOGGERS, default_loggers_expr(b)),
        b.block([b.let_("_lr", b.app(b.loc("lg"), b.loc("ent")))], b.unit()),
    );
    let below_min = b.bin(
        BinOp::Gt,
        log_level_ordinal(b, obs_read(b, KEY_MIN_LOG_LEVEL, b.str_("Info"))),
        log_level_ordinal(b, b.loc("lvl")),
    );
    let entry_tup = b.tuple([
        b.list([msg]),
        b.loc("lvl"),
        opt_cause,
        b.app(b.global("Clock", "current_time_millis"), b.unit()),
    ]);
    let emit_body = b.block([b.let_("ent", entry_tup)], run_loggers);
    b.block(
        [b.let_("lvl", level)],
        b.if_(below_min, b.unit(), emit_body),
    )
}

fn annotate_logs_scoped_template(b: &ExprBuilder) -> Expr {
    let updated = upsert_pair_list(b, b.loc("prev"), b.loc("k"), b.loc("v"));
    let restore = b.task(b.app2(b.loc("ctx_set_ref"), b.loc("aref"), b.loc("prev")));
    b.lam(
        &["k", "v"],
        b.block(
            [
                b.let_("aref", obs_ref(b, KEY_LOG_ANNOTATIONS, b.list(vec![]))),
                b.let_("prev", b.app(b.global("Context", "read"), b.loc("aref"))),
                Stmt::Expr(b.app2(b.loc("ctx_set_ref"), b.loc("aref"), updated)),
                Stmt::Expr(b.app(b.global("Scope", "add_finalizer"), restore)),
            ],
            b.unit(),
        ),
    )
}

fn with_log_span_template(b: &ExprBuilder) -> Expr {
    let entry = b.tuple([
        b.loc("lbl"),
        b.app(b.global("Clock", "current_time_millis"), b.unit()),
    ]);
    let next = b.app2(
        b.global("List.Core", "append"),
        b.list([b.loc("entry")]),
        obs_read(b, KEY_LOG_SPANS, b.list(vec![])),
    );
    b.lam(
        &["lbl", "t"],
        b.block(
            [b.let_("entry", entry), b.let_("next", next)],
            obs_with(b, KEY_LOG_SPANS, b.list(vec![]), b.loc("next"), b.loc("t")),
        ),
    )
}

fn effect_log_template(
    b: &ExprBuilder,
    bind: &str,
    mode: ForeignMode,
    stages: usize,
) -> Option<Expr> {
    if mode != ForeignMode::Effect {
        return None;
    }
    let at_level = |lvl: &str| {
        b.lam(
            &["msg"],
            emit_log_expr(b, b.str_(lvl), b.loc("msg"), b.none()),
        )
    };
    match (bind, stages) {
        ("log", 1) => Some(b.lam(
            &["msg"],
            emit_log_expr(
                b,
                obs_read(b, KEY_CUR_LOG_LEVEL, b.str_("Info")),
                b.loc("msg"),
                b.none(),
            ),
        )),
        ("logFatal", 1) => Some(at_level("Fatal")),
        ("logError", 1) => Some(at_level("Error")),
        ("logWarning", 1) => Some(at_level("Warn")),
        ("logInfo", 1) => Some(at_level("Info")),
        ("logDebug", 1) => Some(at_level("Debug")),
        ("logTrace", 1) => Some(at_level("Trace")),
        ("flickLogWithLevel", 2) => Some(b.lam(
            &["lvl", "msg"],
            emit_log_expr(b, b.loc("lvl"), b.loc("msg"), b.none()),
        )),
        ("withLogger", 2) => {
            let next = b.app2(
                b.global("List.Core", "append"),
                obs_read(b, KEY_LOGGERS, default_loggers_expr(b)),
                b.list([b.loc("lg")]),
            );
            Some(b.lam(
                &["lg", "t"],
                b.block(
                    [b.let_("next", next)],
                    obs_with(
                        b,
                        KEY_LOGGERS,
                        default_loggers_expr(b),
                        b.loc("next"),
                        b.loc("t"),
                    ),
                ),
            ))
        }
        ("annotateLogs", 3) => {
            let next = upsert_pair_list(
                b,
                obs_read(b, KEY_LOG_ANNOTATIONS, b.list(vec![])),
                b.loc("k"),
                b.loc("v"),
            );
            Some(b.lam(
                &["k", "v", "t"],
                b.block(
                    [b.let_("next", next)],
                    obs_with(
                        b,
                        KEY_LOG_ANNOTATIONS,
                        b.list(vec![]),
                        b.loc("next"),
                        b.loc("t"),
                    ),
                ),
            ))
        }
        ("annotateLogsScoped", 2) => Some(annotate_logs_scoped_template(b)),
        ("withLogSpan", 2) => Some(with_log_span_template(b)),
        _ => None,
    }
}
