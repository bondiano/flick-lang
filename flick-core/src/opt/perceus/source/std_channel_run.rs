//! Audited `Channel.Run` pure IR adapters (Stage 4E).

use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::rt;
use super::{Expr, Span, Stmt};

pub(super) fn channel_run_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("runCollect", 1) => Some(rt(b, "Helper.ch_run_collect")),
        ("runCount", 1) => Some(rt(b, "Helper.ch_run_count")),
        ("runDrain", 1) => Some(rt(b, "Helper.ch_run_drain")),
        ("runHead", 1) => Some(rt(b, "Helper.ch_run_head")),
        ("runLast", 1) => Some(rt(b, "Helper.ch_run_last")),
        ("runFold" | "runFoldEffect", 3) => Some(rt(b, "Helper.ch_run_fold")),
        ("runForEach", 2) => Some(rt(b, "Helper.ch_run_for_each")),
        ("runForEachWhile", 2) => Some(rt(b, "Helper.ch_run_for_each_while")),
        ("flickMkUint8Array", 1) => Some(rt(b, "Helper.ch_mk_bytes")),
        ("runIntoQueue", 2) => Some(b.app(rt(b, "Helper.ch_run_into_queue"), b.bool_(false))),
        ("runIntoQueueArray", 2) => Some(b.app(rt(b, "Helper.ch_run_into_queue"), b.bool_(true))),
        ("flickToQueue", 2) => Some(b.app(rt(b, "Helper.ch_to_queue"), b.bool_(false))),
        ("flickToQueueArray", 2) => Some(b.app(rt(b, "Helper.ch_to_queue"), b.bool_(true))),
        ("flickToPubSub", 2) => Some(b.app(rt(b, "Helper.ch_to_pubsub"), b.bool_(false))),
        ("flickToPubSubArray", 2) => Some(b.app(rt(b, "Helper.ch_to_pubsub"), b.bool_(true))),
        ("flickRunIntoPubSub", 3) => {
            Some(b.app(rt(b, "Helper.ch_run_into_pubsub"), b.bool_(false)))
        }
        ("flickRunIntoPubSubArray", 3) => {
            Some(b.app(rt(b, "Helper.ch_run_into_pubsub"), b.bool_(true)))
        }
        _ => None,
    }
}

pub(super) fn channel_run_helper(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$chr");
    match name {
        "ch_run_collect" => Some(ch_run_collect_helper(&b)),
        "ch_run_count" => Some(ch_run_count_helper(&b)),
        "ch_run_drain" => Some(ch_run_drain_helper(&b)),
        "ch_run_head" => Some(ch_run_head_helper(&b)),
        "ch_run_last" => Some(ch_run_last_helper(&b)),
        "ch_run_fold" => Some(ch_run_fold_helper(&b)),
        "ch_run_for_each" => Some(ch_run_for_each_helper(&b)),
        "ch_run_for_each_while" => Some(ch_run_for_each_while_helper(&b)),
        "ch_mk_bytes" => Some(ch_mk_bytes_helper(&b)),
        "ch_run_into_queue" => Some(ch_run_into_queue_helper(&b)),
        "ch_to_queue" => Some(ch_to_queue_helper(&b)),
        "ch_to_pubsub" => Some(ch_to_pubsub_helper(&b)),
        "ch_run_into_pubsub" => Some(ch_run_into_pubsub_helper(&b)),
        _ => None,
    }
}

fn ch_run_collect_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["ch"],
        b.app2(
            rt(b, "Helper.ch_run_scoped"),
            b.loc("ch"),
            b.lam(
                &["pull"],
                b.gen_strict(b.loop_ctrl(
                    "rc",
                    b.match_(
                        b.app(b.loc("pull"), b.unit()),
                        vec![
                            arm(b.ok_pat("v"), b.yield_(b.loc("v"))),
                            arm(b.err_pat("_d"), b.break_("rc", b.unit())),
                        ],
                    ),
                )),
            ),
        ),
    )
}

fn ch_run_count_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["ch"],
        b.app2(
            rt(b, "Helper.ch_run_scoped"),
            b.loc("ch"),
            b.lam(
                &["pull"],
                b.block(
                    vec![b.var_decl("n", b.int(0))],
                    b.loop_ctrl(
                        "rcn",
                        b.match_(
                            b.app(b.loc("pull"), b.unit()),
                            vec![
                                arm(
                                    b.ok_pat("_v"),
                                    b.assign_e("n", b.add_i(b.var("n"), b.int(1))),
                                ),
                                arm(b.err_pat("_d"), b.break_("rcn", b.var("n"))),
                            ],
                        ),
                    ),
                ),
            ),
        ),
    )
}

fn ch_run_drain_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["ch"],
        b.app2(
            rt(b, "Helper.ch_run_scoped"),
            b.loc("ch"),
            b.lam(
                &["pull"],
                b.loop_ctrl(
                    "rdr",
                    b.match_(
                        b.app(b.loc("pull"), b.unit()),
                        vec![
                            arm(b.ok_pat("_v"), b.unit()),
                            arm(b.err_pat("d"), b.break_("rdr", b.loc("d"))),
                        ],
                    ),
                ),
            ),
        ),
    )
}

fn ch_run_head_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["ch"],
        b.app2(
            rt(b, "Helper.ch_run_scoped"),
            b.loc("ch"),
            b.lam(
                &["pull"],
                b.match_(
                    b.app(b.loc("pull"), b.unit()),
                    vec![
                        arm(b.ok_pat("v"), b.some(b.loc("v"))),
                        arm(b.err_pat("_d"), b.none()),
                    ],
                ),
            ),
        ),
    )
}

fn ch_run_last_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["ch"],
        b.app2(
            rt(b, "Helper.ch_run_scoped"),
            b.loc("ch"),
            b.lam(
                &["pull"],
                b.block(
                    vec![b.var_decl("last", b.none())],
                    b.loop_ctrl(
                        "rls",
                        b.match_(
                            b.app(b.loc("pull"), b.unit()),
                            vec![
                                arm(b.ok_pat("v"), b.assign_e("last", b.some(b.loc("v")))),
                                arm(b.err_pat("_d"), b.break_("rls", b.var("last"))),
                            ],
                        ),
                    ),
                ),
            ),
        ),
    )
}

fn ch_run_fold_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["init", "f", "ch"],
        b.app2(
            rt(b, "Helper.ch_run_scoped"),
            b.loc("ch"),
            b.lam(
                &["pull"],
                b.block(
                    vec![b.var_decl("acc", b.app(b.loc("init"), b.unit()))],
                    b.loop_ctrl(
                        "rfl",
                        b.match_(
                            b.app(b.loc("pull"), b.unit()),
                            vec![
                                arm(
                                    b.ok_pat("v"),
                                    b.assign_e("acc", b.app2(b.loc("f"), b.var("acc"), b.loc("v"))),
                                ),
                                arm(b.err_pat("_d"), b.break_("rfl", b.var("acc"))),
                            ],
                        ),
                    ),
                ),
            ),
        ),
    )
}

fn ch_run_for_each_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["f", "ch"],
        b.app2(
            rt(b, "Helper.ch_run_scoped"),
            b.loc("ch"),
            b.lam(
                &["pull"],
                b.loop_ctrl(
                    "rfe",
                    b.match_(
                        b.app(b.loc("pull"), b.unit()),
                        vec![
                            arm(
                                b.ok_pat("v"),
                                b.block(
                                    vec![b.let_("_x", b.app(b.loc("f"), b.loc("v")))],
                                    b.unit(),
                                ),
                            ),
                            arm(b.err_pat("d"), b.break_("rfe", b.loc("d"))),
                        ],
                    ),
                ),
            ),
        ),
    )
}

fn ch_run_for_each_while_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["f", "ch"],
        b.app2(
            rt(b, "Helper.ch_run_scoped"),
            b.loc("ch"),
            b.lam(
                &["pull"],
                b.loop_ctrl(
                    "rfw",
                    b.match_(
                        b.app(b.loc("pull"), b.unit()),
                        vec![
                            arm(
                                b.ok_pat("v"),
                                b.if_(
                                    b.app(b.loc("f"), b.loc("v")),
                                    b.unit(),
                                    b.break_("rfw", b.unit()),
                                ),
                            ),
                            arm(b.err_pat("_d"), b.break_("rfw", b.unit())),
                        ],
                    ),
                ),
            ),
        ),
    )
}

fn ch_mk_bytes_helper(b: &ExprBuilder) -> Expr {
    // Channel emitting `NonEmpty (List Int)` -> flattened `List Int`
    b.lam(
        &["ch"],
        b.app2(
            rt(b, "Helper.ch_run_scoped"),
            b.loc("ch"),
            b.lam(
                &["pull"],
                b.gen_strict(b.loop_ctrl(
                    "rmb",
                    b.match_(
                        b.app(b.loc("pull"), b.unit()),
                        vec![
                            arm(
                                b.ok_pat("chunks"),
                                b.each(
                                    "bs",
                                    b.loc("chunks"),
                                    b.each("byte", b.loc("bs"), b.yield_(b.loc("byte"))),
                                ),
                            ),
                            arm(b.err_pat("_d"), b.break_("rmb", b.unit())),
                        ],
                    ),
                )),
            ),
        ),
    )
}

fn ch_run_into_queue_helper(b: &ExprBuilder) -> Expr {
    let body = b.app2(
        rt(b, "Helper.ch_run_for_each"),
        b.lam(
            &["v"],
            b.block(
                vec![b.let_(
                    "_o",
                    b.if_(
                        b.loc("arr"),
                        b.app2(b.global("Queue", "offer_all"), b.loc("v"), b.loc("q")),
                        b.app2(b.global("Queue", "offer"), b.loc("v"), b.loc("q")),
                    ),
                )],
                b.unit(),
            ),
        ),
        b.loc("ch"),
    );
    b.lam(
        &["arr", "q", "ch"],
        b.app2(
            b.global("Effect.Errors", "catch_cause"),
            b.lam(
                &["c"],
                b.block(
                    vec![b.let_(
                        "_f",
                        b.app2(b.global("Queue", "fail_cause"), b.loc("c"), b.loc("q")),
                    )],
                    b.unit(),
                ),
            ),
            b.task(b.block(
                vec![
                    b.let_("_d", body),
                    b.let_("_e", b.app(b.global("Queue", "end"), b.loc("q"))),
                ],
                b.unit(),
            )),
        ),
    )
}

fn ch_to_queue_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["arr", "opts", "ch"],
        b.block(
            vec![
                b.let_("q", b.app(b.global("Queue", "make"), b.loc("opts"))),
                b.let_("ow", b.app(b.global("Scope", "current"), b.unit())),
                Stmt::Expr(b.app(
                    b.global("Scope", "add_finalizer"),
                    b.task(b.block(
                        vec![b.let_("_sh", b.app(b.global("Queue", "shutdown"), b.loc("q")))],
                        b.unit(),
                    )),
                )),
                b.let_(
                    "_fib",
                    b.app3(
                        b.global("Fiber", "spawn"),
                        b.loc("ow"),
                        b.lam(&["_c"], b.unit()),
                        b.task(b.app3(
                            rt(b, "Helper.ch_run_into_queue"),
                            b.loc("arr"),
                            b.loc("q"),
                            b.loc("ch"),
                        )),
                    ),
                ),
            ],
            b.loc("q"),
        ),
    )
}

fn ch_run_into_pubsub_helper(b: &ExprBuilder) -> Expr {
    let fin = b.if_(
        b.field(b.loc("opts"), "shutdownOnEnd"),
        b.app(b.global("PubSub", "shutdown"), b.loc("hub")),
        b.unit(),
    );
    let body = b.app2(
        rt(b, "Helper.ch_run_for_each"),
        b.lam(
            &["v"],
            b.block(
                vec![b.let_(
                    "_p",
                    b.if_(
                        b.loc("arr"),
                        b.app2(b.global("PubSub", "publish_all"), b.loc("v"), b.loc("hub")),
                        b.app2(b.global("PubSub", "publish"), b.loc("v"), b.loc("hub")),
                    ),
                )],
                b.unit(),
            ),
        ),
        b.loc("ch"),
    );
    b.lam(
        &["arr", "opts", "hub", "ch"],
        b.app2(
            b.global("Effect.Errors", "ensuring"),
            b.task(fin),
            b.task(b.if_(
                b.loc("arr"),
                body.clone(),
                b.block(vec![b.let_("_d", body)], b.unit()),
            )),
        ),
    )
}

fn ch_to_pubsub_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["arr", "opts", "ch"],
        b.block(
            vec![
                b.let_("hub", b.app(rt(b, "Helper.ch_make_pubsub"), b.loc("opts"))),
                b.let_("ow", b.app(b.global("Scope", "current"), b.unit())),
                Stmt::Expr(b.app(
                    b.global("Scope", "add_finalizer"),
                    b.task(b.app(b.global("PubSub", "shutdown"), b.loc("hub"))),
                )),
                b.let_(
                    "_fib",
                    b.app3(
                        b.global("Fiber", "spawn"),
                        b.loc("ow"),
                        b.lam(&["_c"], b.unit()),
                        b.task(b.block(
                            vec![b.let_(
                                "_r",
                                b.app4(
                                    rt(b, "Helper.ch_run_into_pubsub"),
                                    b.loc("arr"),
                                    b.loc("opts"),
                                    b.loc("hub"),
                                    b.loc("ch"),
                                ),
                            )],
                            b.unit(),
                        )),
                    ),
                ),
            ],
            b.loc("hub"),
        ),
    )
}
