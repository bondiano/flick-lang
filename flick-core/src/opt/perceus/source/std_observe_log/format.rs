//! `Logger` formatters, stream writers, and layer templates.

use flick_ir::{Arith, BinOp, ForeignMode, Num};

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_observe_refs::{
    KEY_LOG_ANNOTATIONS, KEY_LOG_SPANS, KEY_LOG_STDERR, KEY_LOGGERS, KEY_PARENT_SPAN, obs_read,
    obs_ref, obs_with,
};
use super::super::{Expr, ExprKind, Pat, Stmt};
use super::{default_loggers_expr, emit_log_expr, entry_pat};

fn fiber_label_expr(b: &ExprBuilder) -> Expr {
    let id_num = b.app(
        b.global("Int", "truncate"),
        b.app(b.global("Effect.Concurrency", "fiber_id"), b.unit()),
    );
    b.bin(
        BinOp::Concat,
        b.str_("#"),
        b.app(b.global("Int", "to_string"), id_num),
    )
}

fn structured_record_expr(b: &ExprBuilder) -> Expr {
    let map_span = b.lam(
        &["sp"],
        b.match_(
            b.loc("sp"),
            [arm(
                b.tup_pat(&["lbl", "st"]),
                b.tuple([
                    b.loc("lbl"),
                    b.bin(
                        BinOp::Arith(Arith::Sub, Num::Float),
                        b.loc("edate"),
                        b.loc("st"),
                    ),
                ]),
            )],
        ),
    );
    let spans = b.app2(
        b.global("List", "map"),
        map_span,
        obs_read(b, KEY_LOG_SPANS, b.list(vec![])),
    );
    let cause_opt = b.match_(
        b.loc("ec"),
        [
            arm(
                b.some_pat("c"),
                b.some(b.app(b.global("Cause", "pretty"), b.loc("c"))),
            ),
            arm(Pat::Wild, b.none()),
        ],
    );
    let annots = obs_read(b, KEY_LOG_ANNOTATIONS, b.list(vec![]));
    b.at(ExprKind::Record(vec![
        (
            "level".into(),
            b.app(b.global("String", "to_upper"), b.loc("elvl")),
        ),
        ("fiberId".into(), fiber_label_expr(b)),
        ("timestamp".into(), b.str_("1970-01-01T00:00:00.000Z")),
        ("message".into(), b.loc("emsg")),
        ("cause".into(), cause_opt),
        ("annotations".into(), annots),
        ("spans".into(), spans),
    ]))
}

fn format_logfmt_expr(b: &ExprBuilder) -> Expr {
    let msg_txt = b.app2(b.global("String", "join"), b.str_(" "), b.loc("emsg"));
    let fiber_msg = b.bin(
        BinOp::Concat,
        fiber_label_expr(b),
        b.bin(BinOp::Concat, b.str_(" message="), msg_txt),
    );
    let lvl_part = b.bin(
        BinOp::Concat,
        b.app(b.global("String", "to_upper"), b.loc("elvl")),
        b.bin(BinOp::Concat, b.str_(" fiber="), fiber_msg),
    );
    b.bin(
        BinOp::Concat,
        b.str_("timestamp=1970-01-01T00:00:00.000Z level="),
        lvl_part,
    )
}

fn format_json_expr(b: &ExprBuilder) -> Expr {
    let msg_txt = b.app2(b.global("String", "join"), b.str_(" "), b.loc("emsg"));
    let msg_json = b.app(b.global("Json", "stringify"), msg_txt);
    let suffix = b.bin(
        BinOp::Concat,
        b.str_(
            "\",\"timestamp\":\"1970-01-01T00:00:00.000Z\",\"annotations\":{},\"spans\":{},\"fiberId\":\"",
        ),
        b.bin(BinOp::Concat, fiber_label_expr(b), b.str_("\"}")),
    );
    let lvl_tail = b.bin(
        BinOp::Concat,
        b.str_(",\"level\":\""),
        b.bin(
            BinOp::Concat,
            b.app(b.global("String", "to_upper"), b.loc("elvl")),
            suffix,
        ),
    );
    b.bin(
        BinOp::Concat,
        b.str_("{\"message\":"),
        b.bin(BinOp::Concat, msg_json, lvl_tail),
    )
}

fn default_line_expr(b: &ExprBuilder) -> Expr {
    let msg_txt = b.app2(b.global("String", "join"), b.str_(" "), b.loc("emsg"));
    let tail = b.bin(
        BinOp::Concat,
        fiber_label_expr(b),
        b.bin(BinOp::Concat, b.str_("): "), msg_txt),
    );
    let mid = b.bin(
        BinOp::Concat,
        b.app(b.global("String", "to_upper"), b.loc("elvl")),
        b.bin(BinOp::Concat, b.str_(" ("), tail),
    );
    b.bin(BinOp::Concat, b.str_("[00:00:00.000] "), mid)
}

fn write_stream_expr(b: &ExprBuilder, text: Expr, to_stderr: Expr) -> Expr {
    b.block(
        [b.let_("txt", text)],
        b.if_(
            to_stderr,
            b.app(b.global("Host", "raw_stderr_line"), b.loc("txt")),
            b.app(b.global("Host", "raw_stdout_line"), b.loc("txt")),
        ),
    )
}

fn entry_lam(b: &ExprBuilder, body: Expr) -> Expr {
    b.lam(&["e"], b.match_(b.loc("e"), [arm(entry_pat(b), body)]))
}

fn leveled_console_template(b: &ExprBuilder) -> Expr {
    let is_err_lvl = b.bin(
        BinOp::Or,
        b.bin(BinOp::Eq, b.loc("elvl"), b.str_("Warn")),
        b.bin(
            BinOp::Or,
            b.bin(BinOp::Eq, b.loc("elvl"), b.str_("Error")),
            b.bin(BinOp::Eq, b.loc("elvl"), b.str_("Fatal")),
        ),
    );
    let non_trace = b.if_(
        is_err_lvl,
        b.app(b.global("Host", "raw_stderr_line"), b.loc("txt")),
        b.app(b.global("Host", "raw_stdout_line"), b.loc("txt")),
    );
    let trace_line = b.app(
        b.global("Host", "raw_stderr_line"),
        b.bin(BinOp::Concat, b.str_("Trace: "), b.loc("txt")),
    );
    let body = b.block(
        [b.let_("txt", b.app(b.loc("l"), b.loc("e")))],
        b.if_(
            b.bin(BinOp::Eq, b.loc("elvl"), b.str_("Trace")),
            trace_line,
            non_trace,
        ),
    );
    b.lam(&["l"], entry_lam(b, body))
}

fn logger_layer_template(b: &ExprBuilder) -> Expr {
    let base = b.if_(
        b.field(b.loc("opts"), "mergeWithExisting"),
        b.app(b.global("Context", "read"), b.loc("lref")),
        b.list(vec![]),
    );
    let next = b.app2(b.global("List.Core", "append"), b.loc("base"), b.loc("lgs"));
    let build = b.lam(
        &["_m"],
        b.block(
            [
                b.let_("lref", obs_ref(b, KEY_LOGGERS, default_loggers_expr(b))),
                b.let_("base", base),
                b.let_("next", next),
            ],
            b.app2(b.global("Context", "make"), b.loc("lref"), b.loc("next")),
        ),
    );
    b.lam(
        &["opts", "lgs"],
        b.app(b.global("Layer", "from_build"), build),
    )
}

fn logger_batched_template(b: &ExprBuilder) -> Expr {
    let flush_task = b.task(b.block(
        [
            b.let_("items", b.rget(b.loc("buf"))),
            b.rset(b.loc("buf"), b.list(vec![])),
        ],
        b.match_(
            b.loc("items"),
            [
                arm(Pat::List(vec![], None), b.unit()),
                arm(Pat::Wild, b.app(b.loc("flush_fn"), b.loc("items"))),
            ],
        ),
    ));
    let blg = b.lam(
        &["e"],
        b.block(
            [
                b.let_("out", b.app(b.loc("inner"), b.loc("e"))),
                b.rset(
                    b.loc("buf"),
                    b.app2(
                        b.global("List.Core", "append"),
                        b.rget(b.loc("buf")),
                        b.list([b.loc("out")]),
                    ),
                ),
            ],
            b.unit(),
        ),
    );
    let base = b.if_(
        b.field(b.loc("opts"), "mergeWithExisting"),
        b.app(b.global("Context", "read"), b.loc("lref")),
        b.list(vec![]),
    );
    let next = b.app2(
        b.global("List.Core", "append"),
        b.loc("base"),
        b.list([b.loc("blg")]),
    );
    let build = b.lam(
        &["_m"],
        b.block(
            [
                b.let_("buf", b.rmake(b.list(vec![]))),
                b.let_("flush_fn", b.field(b.loc("batch"), "flush")),
                Stmt::Expr(b.app(b.global("Scope", "add_finalizer"), flush_task)),
                b.let_("blg", blg),
                b.let_("lref", obs_ref(b, KEY_LOGGERS, default_loggers_expr(b))),
                b.let_("base", base),
                b.let_("next", next),
            ],
            b.app2(b.global("Context", "make"), b.loc("lref"), b.loc("next")),
        ),
    );
    b.lam(
        &["batch", "opts", "inner"],
        b.app(b.global("Layer", "from_build"), build),
    )
}

fn log_write_template(b: &ExprBuilder) -> Expr {
    let err_or_info = b.if_(
        b.bin(BinOp::Eq, b.loc("lvl"), b.str_("error")),
        b.str_("Error"),
        b.str_("Info"),
    );
    let warn_or = b.if_(
        b.bin(BinOp::Eq, b.loc("lvl"), b.str_("warn")),
        b.str_("Warn"),
        err_or_info,
    );
    let mapped = b.if_(
        b.bin(BinOp::Eq, b.loc("lvl"), b.str_("debug")),
        b.str_("Debug"),
        warn_or,
    );
    b.lam(
        &["lvl", "msg"],
        b.block(
            [b.let_("mapped", mapped)],
            emit_log_expr(b, b.loc("mapped"), b.loc("msg"), b.none()),
        ),
    )
}

fn logger_format_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("defaultLogger", 0) => Some(entry_lam(
            b,
            write_stream_expr(
                b,
                default_line_expr(b),
                obs_read(b, KEY_LOG_STDERR, b.bool_(false)),
            ),
        )),
        ("formatSimple" | "formatLogFmt", 0) => Some(entry_lam(b, format_logfmt_expr(b))),
        ("flickFormatStructured", 0) => Some(entry_lam(b, structured_record_expr(b))),
        ("formatJson", 0) => Some(entry_lam(b, format_json_expr(b))),
        ("flickConsolePretty" | "flickConsolePrettyBrowser" | "flickConsolePrettyTty", 1) => {
            Some(b.lam(
                &["_opts"],
                entry_lam(
                    b,
                    write_stream_expr(
                        b,
                        default_line_expr(b),
                        obs_read(b, KEY_LOG_STDERR, b.bool_(false)),
                    ),
                ),
            ))
        }
        ("consoleLogFmt", 0) => Some(entry_lam(
            b,
            b.app(b.global("Host", "raw_stdout_line"), format_logfmt_expr(b)),
        )),
        ("consoleJson" | "consoleStructured", 0) => Some(entry_lam(
            b,
            b.app(b.global("Host", "raw_stdout_line"), format_json_expr(b)),
        )),
        ("tracerLogger", 0) => Some(entry_lam(
            b,
            b.block(
                [b.let_("_sp", obs_read(b, KEY_PARENT_SPAN, b.none()))],
                b.unit(),
            ),
        )),
        _ => None,
    }
}

pub(super) fn logger_template(
    b: &ExprBuilder,
    bind: &str,
    mode: ForeignMode,
    stages: usize,
) -> Option<Expr> {
    if mode == ForeignMode::Pure
        && let Some(expr) = logger_format_template(b, bind, stages)
    {
        return Some(expr);
    }
    match (bind, mode, stages) {
        ("flickMake", ForeignMode::Pure, 1) => {
            let cause = b.match_(
                b.loc("ec"),
                [
                    arm(b.some_pat("c"), b.loc("c")),
                    arm(Pat::Wild, b.global("Cause", "empty")),
                ],
            );
            let rec = b.at(ExprKind::Record(vec![
                ("message".into(), b.loc("emsg")),
                ("logLevel".into(), b.loc("elvl")),
                ("cause".into(), cause),
                ("date".into(), b.loc("edate")),
            ]));
            Some(b.lam(&["f"], entry_lam(b, b.app(b.loc("f"), rec))))
        }
        ("map", ForeignMode::Pure, 2) => Some(b.lam(
            &["f", "l"],
            b.lam(&["e"], b.app(b.loc("f"), b.app(b.loc("l"), b.loc("e")))),
        )),
        ("withConsoleLog", ForeignMode::Pure, 1) => Some(b.lam(
            &["l"],
            b.lam(
                &["e"],
                b.app(
                    b.global("Host", "raw_stdout_line"),
                    b.app(b.loc("l"), b.loc("e")),
                ),
            ),
        )),
        ("withConsoleError", ForeignMode::Pure, 1) => Some(b.lam(
            &["l"],
            b.lam(
                &["e"],
                b.app(
                    b.global("Host", "raw_stderr_line"),
                    b.app(b.loc("l"), b.loc("e")),
                ),
            ),
        )),
        ("withLeveledConsole", ForeignMode::Pure, 1) => Some(leveled_console_template(b)),
        ("flickLayer", ForeignMode::Pure, 2) => Some(logger_layer_template(b)),
        ("flickBatched", ForeignMode::Pure, 3) => Some(logger_batched_template(b)),
        ("flickWithLoggers", ForeignMode::Effect, 2) => Some(b.lam(
            &["lgs", "t"],
            obs_with(
                b,
                KEY_LOGGERS,
                default_loggers_expr(b),
                b.loc("lgs"),
                b.loc("t"),
            ),
        )),
        ("flickWithLogToStderr", ForeignMode::Effect, 2) => Some(b.lam(
            &["flag", "t"],
            obs_with(b, KEY_LOG_STDERR, b.bool_(false), b.loc("flag"), b.loc("t")),
        )),
        ("flickLogWrite", ForeignMode::Effect, 2) => Some(log_write_template(b)),
        _ => None,
    }
}
