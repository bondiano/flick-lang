//! `TxPubSub` pure IR adapter templates.

use flick_ir::{BinOp, ForeignMode};

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};

// TxPubSub: `(strat : Int, cap : Int, subs : TxRef (List TxQueue), shut : TxRef Bool)`
fn tps_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["strat", "cap", "subs_r", "shut_r"])
}

fn make_ps(b: &ExprBuilder, strat: i32, cap: Expr) -> Expr {
    b.tuple([
        b.int(strat),
        cap,
        b.app(b.global("TxRef", "make"), b.list(vec![])),
        b.app(b.global("TxRef", "make"), b.bool_(false)),
    ])
}

fn in_tx(b: &ExprBuilder, body: Expr) -> Expr {
    b.app(
        b.global("Tx", "atomically"),
        b.task(b.match_(b.loc("p"), [arm(tps_pat(b), body)])),
    )
}

fn pubsub_publish(b: &ExprBuilder) -> Expr {
    let offer_one = b.if_(
        b.app2(b.global("TxQueue", "offer"), b.loc("x"), b.loc("sq")),
        b.unit(),
        b.block([b.assign("all_ok", b.bool_(false))], b.unit()),
    );
    let loop_subs = b.for_ctrl(
        "tpspb",
        "sq",
        b.app(b.global("TxRef", "get"), b.loc("subs_r")),
        offer_one,
    );
    let do_pub = b.block(
        [
            b.var_decl("all_ok", b.bool_(true)),
            b.let_("_pb", loop_subs),
        ],
        b.var("all_ok"),
    );
    let body = b.if_(
        b.app(b.global("TxRef", "get"), b.loc("shut_r")),
        b.bool_(false),
        do_pub,
    );
    b.lam(&["x", "p"], in_tx(b, body))
}

fn pubsub_publish_all(b: &ExprBuilder) -> Expr {
    let pub_one = b.if_(
        b.app2(b.global("TxPubSub", "publish"), b.loc("x"), b.loc("p")),
        b.unit(),
        b.block([b.assign("all_ok", b.bool_(false))], b.unit()),
    );
    let do_pub = b.block(
        [
            b.var_decl("all_ok", b.bool_(true)),
            b.let_("_pba", b.for_ctrl("tpspba", "x", b.loc("xs"), pub_one)),
        ],
        b.var("all_ok"),
    );
    let body = b.if_(
        b.app(b.global("TxRef", "get"), b.loc("shut_r")),
        b.bool_(false),
        do_pub,
    );
    b.lam(&["xs", "p"], in_tx(b, body))
}

fn pubsub_subscribe(b: &ExprBuilder) -> Expr {
    let new_sq = b.if_(
        b.bin(BinOp::Lt, b.loc("cap"), b.int(0)),
        b.app(b.global("TxQueue", "unbounded"), b.unit()),
        b.if_(
            b.bin(BinOp::Eq, b.loc("strat"), b.int(1)),
            b.app(b.global("TxQueue", "dropping"), b.loc("cap")),
            b.if_(
                b.bin(BinOp::Eq, b.loc("strat"), b.int(2)),
                b.app(b.global("TxQueue", "sliding"), b.loc("cap")),
                b.app(b.global("TxQueue", "bounded"), b.loc("cap")),
            ),
        ),
    );
    let add_sub = b.app(
        b.global("Tx", "atomically"),
        b.task(b.app2(
            b.global("TxRef", "update"),
            b.lam(
                &["ss"],
                b.app2(
                    b.global("List.Core", "append"),
                    b.loc("ss"),
                    b.list([b.loc("sq")]),
                ),
            ),
            b.loc("subs_r"),
        )),
    );
    let not_same_sq = b.match_(
        b.loc("sq"),
        [arm(
            b.tup_pat(&["_s2", "_c2", "ir2", "_sr2"]),
            b.not(b.app2(b.global("Ref", "same"), b.loc("ir1"), b.loc("ir2"))),
        )],
    );
    let keep_pred = b.lam(
        &["e"],
        b.match_(
            b.loc("e"),
            [arm(b.tup_pat(&["_s1", "_c1", "ir1", "_sr1"]), not_same_sq)],
        ),
    );
    let remove_sub = b.app2(
        b.global("TxRef", "update"),
        b.lam(
            &["ss"],
            b.app2(b.global("List.Core", "filter"), keep_pred, b.loc("ss")),
        ),
        b.loc("subs_r"),
    );
    let cleanup = b.app(
        b.global("Tx", "atomically"),
        b.task(b.block(
            [
                Stmt::Expr(remove_sub),
                b.let_("_sh", b.app(b.global("TxQueue", "shutdown"), b.loc("sq"))),
            ],
            b.unit(),
        )),
    );
    let body = b.block(
        [
            b.let_("sq", new_sq),
            Stmt::Expr(add_sub),
            Stmt::Expr(b.app(b.global("Scope", "add_finalizer"), b.task(cleanup))),
        ],
        b.loc("sq"),
    );
    b.lam(&["p"], b.match_(b.loc("p"), [arm(tps_pat(b), body)]))
}

fn pubsub_shutdown(b: &ExprBuilder) -> Expr {
    let shut_one = b.block(
        [b.let_("_s", b.app(b.global("TxQueue", "shutdown"), b.loc("sq")))],
        b.unit(),
    );
    let loop_subs = b.for_ctrl(
        "tpssh",
        "sq",
        b.app(b.global("TxRef", "get"), b.loc("subs_r")),
        shut_one,
    );
    let do_shut = b.block(
        [
            Stmt::Expr(b.app2(b.global("TxRef", "set"), b.bool_(true), b.loc("shut_r"))),
            b.let_("_sh", loop_subs),
        ],
        b.unit(),
    );
    let body = b.if_(
        b.app(b.global("TxRef", "get"), b.loc("shut_r")),
        b.unit(),
        do_shut,
    );
    b.lam(&["p"], in_tx(b, body))
}

pub(super) fn tx_pubsub_template(
    b: &ExprBuilder,
    bind: &str,
    mode: ForeignMode,
    stages: usize,
) -> Option<Expr> {
    if mode == ForeignMode::Pure && bind == "capacity" && stages == 1 {
        let cap_val = b.if_(
            b.bin(BinOp::Lt, b.loc("cap"), b.int(0)),
            b.float(f64::INFINITY),
            b.loc("cap"),
        );
        return Some(b.lam(&["p"], b.match_(b.loc("p"), [arm(tps_pat(b), cap_val)])));
    }
    if mode != ForeignMode::Effect {
        return None;
    }
    match (bind, stages) {
        ("bounded", 1) => Some(b.lam(&["c"], make_ps(b, 0, b.loc("c")))),
        ("dropping", 1) => Some(b.lam(&["c"], make_ps(b, 1, b.loc("c")))),
        ("sliding", 1) => Some(b.lam(&["c"], make_ps(b, 2, b.loc("c")))),
        ("unbounded", 1) => Some(b.lam(&["_u"], make_ps(b, 0, b.int(-1)))),
        ("size", 1) => {
            let max_step = b.lam(
                &["mx", "sq"],
                b.block(
                    [b.let_("n", b.app(b.global("TxQueue", "size"), b.loc("sq")))],
                    b.if_(
                        b.bin(BinOp::Gt, b.loc("n"), b.loc("mx")),
                        b.loc("n"),
                        b.loc("mx"),
                    ),
                ),
            );
            let max_sz = b.app3(
                b.global("List.Core", "fold_left"),
                max_step,
                b.int(0),
                b.app(b.global("TxRef", "get"), b.loc("subs_r")),
            );
            Some(b.lam(&["p"], in_tx(b, max_sz)))
        }
        ("isEmpty", 1) => Some(b.lam(
            &["p"],
            b.bin(
                BinOp::Eq,
                b.app(b.global("TxPubSub", "size"), b.loc("p")),
                b.int(0),
            ),
        )),
        ("isNonEmpty", 1) => Some(b.lam(
            &["p"],
            b.bin(
                BinOp::Gt,
                b.app(b.global("TxPubSub", "size"), b.loc("p")),
                b.int(0),
            ),
        )),
        ("isFull", 1) => {
            let any_full = b.app2(
                b.global("List.Core", "any"),
                b.lam(&["sq"], b.app(b.global("TxQueue", "is_full"), b.loc("sq"))),
                b.app(b.global("TxRef", "get"), b.loc("subs_r")),
            );
            let body = b.if_(
                b.bin(BinOp::Lt, b.loc("cap"), b.int(0)),
                b.bool_(false),
                any_full,
            );
            Some(b.lam(&["p"], in_tx(b, body)))
        }
        ("isShutdown", 1) => Some(b.lam(
            &["p"],
            in_tx(b, b.app(b.global("TxRef", "get"), b.loc("shut_r"))),
        )),
        ("publish", 2) => Some(pubsub_publish(b)),
        ("publishAll", 2) => Some(pubsub_publish_all(b)),
        ("subscribe", 1) => Some(pubsub_subscribe(b)),
        ("shutdown", 1) => Some(pubsub_shutdown(b)),
        ("awaitShutdown", 1) => Some(b.lam(
            &["p"],
            in_tx(
                b,
                b.if_(
                    b.app(b.global("TxRef", "get"), b.loc("shut_r")),
                    b.unit(),
                    b.app(b.global("Tx", "retry"), b.unit()),
                ),
            ),
        )),
        _ => None,
    }
}
