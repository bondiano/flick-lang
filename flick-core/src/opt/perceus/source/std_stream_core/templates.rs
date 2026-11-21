//! `Stream.Core`, `Stream.Run`, and `Stream.Platform` foreign dispatch templates.

use flick_ir::{Arith, BinOp, Num};

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_channel_core::DEFAULT_CHUNK_SIZE;
use super::super::std_schema_rt::rt;
use super::super::{Expr, ExprKind, Pat};

pub(super) fn stream_core_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("DefaultChunkSize", 0) => Some(b.int(DEFAULT_CHUNK_SIZE)),
        ("empty", 0) => Some(b.lam(&["_up", "_own"], b.lam(&["_u"], b.err(b.unit())))),
        ("never", 0) => {
            let pull = b.lam(&["_u"], b.app(rt(b, "Async.EffectNever"), b.unit()));
            Some(b.lam(&["_up", "_own"], pull))
        }
        ("succeed", 1) => {
            let chunk = b.list([b.loc("v")]);
            let body = b.app(rt(b, "Helper.ch_from_list"), b.list([chunk]));
            Some(b.lam(&["v"], body))
        }
        ("make" | "fromArray", 1) => {
            let body = b.app2(
                rt(b, "Helper.ch_from_list_chunked"),
                b.int(DEFAULT_CHUNK_SIZE),
                b.loc("xs"),
            );
            Some(b.lam(&["xs"], body))
        }
        ("fromIterable", 1) => {
            let items = b.app(b.global("Seq", "to_list"), b.loc("sq"));
            let body = b.app2(
                rt(b, "Helper.ch_from_list_chunked"),
                b.int(DEFAULT_CHUNK_SIZE),
                items,
            );
            Some(b.lam(&["sq"], body))
        }
        ("fromArrays", 1) => Some(rt(b, "Helper.st_from_lists")),
        ("suspend", 1) => {
            let inner = b.app(b.loc("f"), b.unit());
            Some(b.lam(
                &["f", "up", "own"],
                b.app2(inner, b.loc("up"), b.loc("own")),
            ))
        }
        ("fail", 1) => {
            let pull = b.lam(&["_u"], b.app(b.global("Fail", "fail"), b.loc("e")));
            Some(b.lam(&["e", "_up", "_own"], pull))
        }
        ("flickFailCause", 1) => {
            let pull = b.lam(&["_u"], b.app(b.global("Failure", "rethrow"), b.loc("c")));
            Some(b.lam(&["c", "_up", "_own"], pull))
        }
        ("flickDie", 1) => {
            let pull = b.lam(&["_u"], b.app(b.global("Effect.Core", "die"), b.loc("msg")));
            Some(b.lam(&["msg", "_up", "_own"], pull))
        }
        _ => stream_core_effect_template(b, bind, stages),
    }
}

fn stream_core_effect_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("fromEffect", 1) => {
            let task = b.task(b.list([b.await_(b.loc("t"))]));
            Some(b.lam(&["t"], b.app(rt(b, "Helper.ch_from_effect"), task)))
        }
        ("fromEffectDrain", 1) => {
            let body = b.block([b.let_("_x", b.await_(b.loc("t")))], b.err(b.unit()));
            Some(b.lam(&["t", "_up", "_own"], b.lam(&["_u"], body)))
        }
        ("fromEffectRepeat", 1) => Some(rt(b, "Helper.st_from_effect_repeat")),
        ("flickFromEffectSchedule", 2) => Some(rt(b, "Helper.st_from_effect_schedule")),
        ("fromArrayEffect", 1) => Some(rt(b, "Helper.st_from_list_effect")),
        ("fromIterableEffectRepeat", 1) => Some(rt(b, "Helper.st_from_list_effect_repeat")),
        ("flickUnwrap", 1) => Some(rt(b, "Helper.ch_unwrap")),
        ("flickCallback", 1) => {
            let opts = b.at(ExprKind::Record(vec![
                ("bufferSize".into(), b.some(b.int(16))),
                ("strategy".into(), b.none()),
            ]));
            let body = b.app3(rt(b, "Helper.ch_callback"), b.bool_(true), opts, b.loc("f"));
            Some(b.lam(&["f"], body))
        }
        ("tick", 1) => Some(rt(b, "Helper.st_tick")),
        ("fromSchedule", 1) => Some(rt(b, "Helper.st_from_schedule")),
        ("fromQueue", 1) => Some(b.app(rt(b, "Helper.ch_from_queue"), b.bool_(true))),
        ("fromPubSub", 1) => Some(b.app(rt(b, "Helper.ch_from_pubsub"), b.bool_(true))),
        ("fromSubscription", 1) => Some(b.app(rt(b, "Helper.ch_from_sub"), b.bool_(true))),
        ("flickUnfold", 2) => Some(rt(b, "Helper.st_unfold")),
        ("paginate", 2) => Some(rt(b, "Helper.st_paginate")),
        ("iterate", 2) => Some(rt(b, "Helper.st_iterate")),
        ("range", 2) => Some(rt(b, "Helper.st_range")),
        // Stream.Platform pure ops
        ("flickDecodeText", 1) => Some(b.lam(
            &["self"],
            b.app2(
                rt(b, "Helper.ch_pipe_to"),
                rt(b, "Helper.ch_decode_text"),
                b.loc("self"),
            ),
        )),
        ("flickEncodeText", 1) => Some(b.lam(
            &["self"],
            b.app2(
                rt(b, "Helper.ch_pipe_to"),
                rt(b, "Helper.ch_encode_text"),
                b.loc("self"),
            ),
        )),
        ("splitLines", 1) => Some(b.lam(
            &["self"],
            b.app2(
                rt(b, "Helper.ch_pipe_to"),
                rt(b, "Helper.ch_split_lines"),
                b.loc("self"),
            ),
        )),
        ("flickLimitBytes", 3) => Some(rt(b, "Helper.st_limit_bytes")),
        _ => None,
    }
}

pub(super) fn stream_run_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("runCollect", 1) => Some(rt(b, "Helper.st_run_collect")),
        ("runCount", 1) => {
            let add_len = b.lam(
                &["n", "xs"],
                b.add_i(
                    b.loc("n"),
                    b.app(b.global("List.Core", "length"), b.loc("xs")),
                ),
            );
            let body = b.app3(
                rt(b, "Helper.ch_run_fold"),
                b.lam(&["_u"], b.int(0)),
                add_len,
                b.loc("self"),
            );
            Some(b.lam(&["self"], body))
        }
        ("runSum", 1) => {
            let add_f = b.lam(
                &["a", "x"],
                b.bin(BinOp::Arith(Arith::Add, Num::Float), b.loc("a"), b.loc("x")),
            );
            let body = b.app3(
                rt(b, "Helper.st_run_fold"),
                b.float(0.0),
                add_f,
                b.loc("self"),
            );
            Some(b.lam(&["self"], body))
        }
        ("runDrain", 1) => {
            let drained = b.let_("_d", b.app(rt(b, "Helper.ch_run_drain"), b.loc("self")));
            Some(b.lam(&["self"], b.block([drained], b.unit())))
        }
        ("runHead", 1) => {
            let first = b.app2(rt(b, "Helper.list_at"), b.int(0), b.loc("xs"));
            let body = b.match_(
                b.app(rt(b, "Helper.ch_run_head"), b.loc("self")),
                [
                    arm(b.some_pat("xs"), b.some(first)),
                    arm(Pat::Wild, b.none()),
                ],
            );
            Some(b.lam(&["self"], body))
        }
        ("runLast", 1) => {
            let last_idx = b.sub_i(
                b.app(b.global("List.Core", "length"), b.loc("xs")),
                b.int(1),
            );
            let last = b.app2(rt(b, "Helper.list_at"), last_idx, b.loc("xs"));
            let body = b.match_(
                b.app(rt(b, "Helper.ch_run_last"), b.loc("self")),
                [
                    arm(b.some_pat("xs"), b.some(last)),
                    arm(Pat::Wild, b.none()),
                ],
            );
            Some(b.lam(&["self"], body))
        }
        ("flickRunFold" | "flickRunFoldEffect", 3) => Some(rt(b, "Helper.st_run_fold")),
        _ => stream_run_sink_template(b, bind, stages),
    }
}

fn stream_run_sink_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("runForEach", 2) => Some(rt(b, "Helper.st_run_for_each")),
        ("runForEachWhile", 2) => Some(rt(b, "Helper.st_run_for_each_while")),
        ("runForEachArray", 2) => {
            let each_chunk = b.lam(
                &["xs"],
                b.block([b.let_("_r", b.app(b.loc("f"), b.loc("xs")))], b.unit()),
            );
            let run = b.app2(rt(b, "Helper.ch_run_for_each"), each_chunk, b.loc("self"));
            Some(b.lam(&["f", "self"], b.block([b.let_("_d", run)], b.unit())))
        }
        ("mkString", 1) => Some(rt(b, "Helper.st_mk_string")),
        ("flickMkBytes", 1) => Some(rt(b, "Helper.ch_mk_bytes")),
        ("runIntoPubSub", 2) => {
            let opts = b.at(ExprKind::Record(vec![(
                "shutdownOnEnd".into(),
                b.bool_(false),
            )]));
            let run = b.app4(
                rt(b, "Helper.ch_run_into_pubsub"),
                b.bool_(true),
                opts,
                b.loc("hub"),
                b.loc("self"),
            );
            Some(b.lam(&["hub", "self"], b.block([b.let_("_d", run)], b.unit())))
        }
        ("runIntoQueue", 2) => Some(b.app(rt(b, "Helper.ch_run_into_queue"), b.bool_(true))),
        ("flickToPubSub", 2) => {
            let opts = b.at(ExprKind::Record(vec![
                ("capacity".into(), b.field(b.loc("opts"), "capacity")),
                ("strategy".into(), b.field(b.loc("opts"), "strategy")),
                ("replay".into(), b.field(b.loc("opts"), "replay")),
                ("shutdownOnEnd".into(), b.bool_(true)),
            ]));
            let body = b.app3(
                rt(b, "Helper.ch_to_pubsub"),
                b.bool_(true),
                opts,
                b.loc("self"),
            );
            Some(b.lam(&["opts", "self"], body))
        }
        ("flickToQueue", 2) => Some(b.app(rt(b, "Helper.ch_to_queue"), b.bool_(true))),
        _ => None,
    }
}
