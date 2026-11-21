//! Tracing span lifecycle helpers (`open_span_expr`, `end_span_expr`, `with_parent_span_expr`).

use flick_ir::{Arith, BinOp, Num};

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_observe_refs::{
    KEY_DISABLE_PROP, KEY_MIN_TRACE_LEVEL, KEY_PARENT_SPAN, KEY_SPAN_ANNOTATIONS, KEY_SPAN_LINKS,
    KEY_STACK_FRAME, KEY_TRACE_LEVEL, KEY_TRACER_ENABLED, KEY_TRACER_TIMING, log_level_ordinal,
    obs_read, obs_with, upsert_pair_list,
};
use super::super::{Expr, Pat};
use super::{sp_pat, sst_pat};

fn propagating_parent_expr(b: &ExprBuilder, cur_opt: Expr) -> Expr {
    let walk_up = b.if_(
        b.loc("sext"),
        b.break_("prp", b.none()),
        b.block([b.assign("cur", b.loc("spar"))], b.unit()),
    );
    let check_sp = b.if_(
        b.not(b.loc("sunp")),
        b.break_("prp", b.some(b.loc("sp"))),
        walk_up,
    );
    let on_some = b.match_(b.loc("sp"), [arm(sp_pat(b), check_sp)]);
    let step = b.match_(
        b.var("cur"),
        [
            arm(b.some_pat("sp"), on_some),
            arm(Pat::Wild, b.break_("prp", b.none())),
        ],
    );
    b.block([b.var_decl("cur", cur_opt)], b.loop_ctrl("prp", step))
}

pub(in super::super) fn end_span_expr(b: &ExprBuilder, sp: Expr) -> Expr {
    let end_ns = b.if_(
        obs_read(b, KEY_TRACER_TIMING, b.bool_(true)),
        b.app(b.global("$rt", "Host.CurrentTimeNanos"), b.unit()),
        b.bigint("0"),
    );
    let ended_state = b.tuple([
        b.bool_(true),
        b.loc("en"),
        b.loc("attrs"),
        b.loc("links"),
        b.loc("events"),
    ]);
    let close = b.block(
        [b.let_("en", end_ns), b.rset(b.loc("sst"), ended_state)],
        b.unit(),
    );
    let update_state = b.match_(
        b.rget(b.loc("sst")),
        [arm(sst_pat(b), b.if_(b.loc("ended"), b.unit(), close))],
    );
    let body = b.if_(
        b.bin(BinOp::Or, b.loc("snoop"), b.loc("sext")),
        b.unit(),
        update_state,
    );
    b.match_(sp, [arm(sp_pat(b), body)])
}

fn random_id_expr(b: &ExprBuilder, prefix: &str) -> Expr {
    let scaled = b.bin(
        BinOp::Arith(Arith::Mul, Num::Float),
        b.app(b.global("$rt", "Host.RandomNext"), b.unit()),
        b.float(900_000.0),
    );
    let num = b.add_i(
        b.app(
            b.global("Int", "truncate"),
            b.app(b.global("Float", "floor"), scaled),
        ),
        b.int(100_000),
    );
    b.bin(
        BinOp::Concat,
        b.str_(prefix),
        b.app(b.global("Int", "to_string"), num),
    )
}

fn sampled_expr(b: &ExprBuilder) -> Expr {
    let level_ok = || {
        b.bin(
            BinOp::Le,
            log_level_ordinal(b, obs_read(b, KEY_MIN_TRACE_LEVEL, b.str_("All"))),
            log_level_ordinal(b, b.loc("lvl")),
        )
    };
    let parent_sampled = b.match_(b.loc("p"), [arm(sp_pat(b), b.loc("ssamp"))]);
    let from_parent = b.if_(b.not(parent_sampled), b.bool_(false), level_ok());
    let infer = b.match_(
        b.loc("par"),
        [
            arm(b.some_pat("p"), from_parent),
            arm(Pat::Wild, level_ok()),
        ],
    );
    b.match_(
        b.field(b.loc("opts"), "sampled"),
        [arm(b.some_pat("sb"), b.loc("sb")), arm(Pat::Wild, infer)],
    )
}

fn active_span_expr(b: &ExprBuilder) -> Expr {
    let level_expr = b.match_(
        b.field(b.loc("opts"), "level"),
        [
            arm(b.some_pat("l"), b.loc("l")),
            arm(Pat::Wild, obs_read(b, KEY_TRACE_LEVEL, b.str_("Info"))),
        ],
    );
    let tid_expr = b.match_(
        b.loc("par"),
        [
            arm(
                b.some_pat("p"),
                b.match_(b.loc("p"), [arm(sp_pat(b), b.loc("stid"))]),
            ),
            arm(Pat::Wild, random_id_expr(b, "tr-")),
        ],
    );
    let merge_step = b.match_(
        b.loc("kv"),
        [arm(
            b.tup_pat(&["ak", "av"]),
            b.block(
                [b.assign(
                    "v_attrs",
                    upsert_pair_list(b, b.var("v_attrs"), b.loc("ak"), b.loc("av")),
                )],
                b.unit(),
            ),
        )],
    );
    let merge_attrs = b.for_ctrl(
        "sma",
        "kv",
        b.field(b.loc("opts"), "attributes"),
        merge_step,
    );
    let links = b.app2(
        b.global("List.Core", "append"),
        obs_read(b, KEY_SPAN_LINKS, b.list(vec![])),
        b.field(b.loc("opts"), "links"),
    );
    let start_ns = b.if_(
        obs_read(b, KEY_TRACER_TIMING, b.bool_(true)),
        b.app(b.global("$rt", "Host.CurrentTimeNanos"), b.unit()),
        b.bigint("0"),
    );
    let state_ref = b.rmake(b.tuple([
        b.bool_(false),
        b.bigint("0"),
        b.var("v_attrs"),
        b.loc("links"),
        b.list(vec![]),
    ]));
    b.block(
        [
            b.let_("lvl", level_expr),
            b.let_("samp", sampled_expr(b)),
            b.let_("links", links),
            b.let_("start_ns", start_ns),
            b.let_("sid", random_id_expr(b, "sp-")),
            b.let_("tid", tid_expr),
            b.var_decl("v_attrs", obs_read(b, KEY_SPAN_ANNOTATIONS, b.list(vec![]))),
            b.let_("_ma", merge_attrs),
        ],
        b.tuple([
            b.some(b.loc("nm")),
            b.loc("sid"),
            b.loc("tid"),
            b.loc("samp"),
            b.loc("par"),
            b.bool_(false),
            b.some(b.field(b.loc("opts"), "kind_")),
            b.bool_(false),
            b.bool_(false),
            b.loc("start_ns"),
            state_ref,
            b.field(b.loc("opts"), "captureStackTrace"),
        ]),
    )
}

pub(in super::super) fn open_span_expr(b: &ExprBuilder, name: Expr, opts: Expr) -> Expr {
    let default_parent = b.if_(
        b.field(b.loc("opts"), "root"),
        b.none(),
        propagating_parent_expr(b, obs_read(b, KEY_PARENT_SPAN, b.none())),
    );
    let parent_expr = b.match_(
        b.field(b.loc("opts"), "parent"),
        [
            arm(b.some_pat("p"), b.some(b.loc("p"))),
            arm(Pat::Wild, default_parent),
        ],
    );
    let noop_state = b.rmake(b.tuple([
        b.bool_(true),
        b.bigint("0"),
        b.list(vec![]),
        b.list(vec![]),
        b.list(vec![]),
    ]));
    let noop_span = b.tuple([
        b.some(b.loc("nm")),
        b.str_("noop"),
        b.str_("noop"),
        b.bool_(false),
        b.loc("par"),
        b.bool_(false),
        b.some(b.str_("internal")),
        b.bool_(true),
        b.bool_(true),
        b.bigint("0"),
        noop_state,
        b.bool_(false),
    ]);
    let off = b.bin(
        BinOp::Or,
        b.not(obs_read(b, KEY_TRACER_ENABLED, b.bool_(true))),
        b.bin(
            BinOp::Or,
            b.field(b.loc("opts"), "disablePropagation"),
            obs_read(b, KEY_DISABLE_PROP, b.bool_(false)),
        ),
    );
    b.block(
        [
            b.let_("nm", name),
            b.let_("opts", opts),
            b.let_("par", parent_expr),
            b.let_("off", off),
        ],
        b.if_(b.loc("off"), noop_span, active_span_expr(b)),
    )
}

pub(in super::super) fn with_parent_span_expr(b: &ExprBuilder, sp: Expr, task: Expr) -> Expr {
    let nm = b.match_(
        b.loc("sname"),
        [arm(b.some_pat("n"), b.loc("n")), arm(Pat::Wild, b.str_(""))],
    );
    let loc_txt = b.if_(
        b.loc("scst"),
        b.bin(BinOp::Concat, b.str_("at "), b.loc("nm")),
        b.str_(""),
    );
    let frame = b.tuple([b.loc("nm"), loc_txt, obs_read(b, KEY_STACK_FRAME, b.none())]);
    let framed_task = b.task(obs_with(
        b,
        KEY_STACK_FRAME,
        b.none(),
        b.some(b.loc("frame")),
        b.loc("tsk"),
    ));
    let annotated_rethrow = b.app(
        b.global("Failure", "rethrow"),
        b.app3(
            b.global("Cause", "annotate"),
            b.bool_(false),
            b.loc("frame"),
            b.loc("c"),
        ),
    );
    let on_fail = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("ex")),
        [
            arm(b.some_pat("c"), annotated_rethrow),
            arm(
                Pat::Wild,
                b.app(b.global("Effect.Core", "die"), b.str_("span exit")),
            ),
        ],
    );
    let run_with_frame = b.block(
        [
            b.let_("nm", nm),
            b.let_("frame", frame),
            b.let_("ex", b.app(b.global("Effect.Core", "exit"), framed_task)),
        ],
        b.match_(
            b.app(b.global("Exit", "value"), b.loc("ex")),
            [arm(b.some_pat("v"), b.loc("v")), arm(Pat::Wild, on_fail)],
        ),
    );
    let body = b.task(b.match_(
        b.loc("sp"),
        [arm(
            sp_pat(b),
            b.if_(b.loc("sext"), b.await_(b.loc("tsk")), run_with_frame),
        )],
    ));
    b.block(
        [b.let_("sp", sp), b.let_("tsk", task)],
        obs_with(b, KEY_PARENT_SPAN, b.none(), b.some(b.loc("sp")), body),
    )
}
