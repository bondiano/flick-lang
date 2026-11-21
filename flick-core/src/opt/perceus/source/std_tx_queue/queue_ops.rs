//! `TxQueue` batch take, peek, status query, and shutdown/close templates.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};
use super::queue::{fail_if_done, in_tx, read_state, tq_rest_stmt};

fn close_with_cause(b: &ExprBuilder, cause_expr: Expr, drop_items: bool) -> Expr {
    let clear_items = if drop_items {
        b.app2(b.global("TxRef", "set"), b.list(vec![]), b.loc("iref"))
    } else {
        b.unit()
    };
    let ntag = if drop_items {
        b.int(2)
    } else {
        let len = b.app(
            b.global("List.Core", "length"),
            b.app(b.global("TxRef", "get"), b.loc("iref")),
        );
        b.if_(b.bin(BinOp::Eq, len, b.int(0)), b.int(2), b.int(1))
    };
    let do_close = b.block(
        [
            b.let_("c", cause_expr),
            Stmt::Expr(clear_items),
            b.let_("ntag", ntag),
            Stmt::Expr(b.app2(
                b.global("TxRef", "set"),
                b.tuple([b.loc("ntag"), b.some(b.loc("c"))]),
                b.loc("sref"),
            )),
        ],
        b.bool_(true),
    );
    in_tx(
        b,
        read_state(
            b,
            b.if_(
                b.not(b.bin(BinOp::Eq, b.loc("tag"), b.int(0))),
                b.bool_(false),
                do_close,
            ),
        ),
    )
}

fn tx_queue_take_between(b: &ExprBuilder) -> Expr {
    let invalid_range = b.bin(
        BinOp::Or,
        b.bin(BinOp::Le, b.loc("lo"), b.int(0)),
        b.bin(
            BinOp::Or,
            b.bin(BinOp::Le, b.loc("hi"), b.int(0)),
            b.bin(BinOp::Gt, b.loc("lo"), b.loc("hi")),
        ),
    );
    let under_min = b.if_(
        b.not(b.bin(BinOp::Eq, b.loc("tag"), b.int(1))),
        b.app(b.global("Tx", "retry"), b.unit()),
        b.block([tq_rest_stmt(b, b.loc("xs"), b.loc("len"))], b.loc("xs")),
    );
    let take_cnt = b.block(
        [
            b.let_(
                "cnt",
                b.if_(
                    b.bin(BinOp::Lt, b.loc("len"), b.loc("hi")),
                    b.loc("len"),
                    b.loc("hi"),
                ),
            ),
            tq_rest_stmt(b, b.loc("xs"), b.loc("cnt")),
        ],
        b.app2(b.global("List.Core", "take"), b.loc("cnt"), b.loc("xs")),
    );
    let read_and_take = b.block(
        [
            b.let_("xs", b.app(b.global("TxRef", "get"), b.loc("iref"))),
            b.let_("len", b.app(b.global("List.Core", "length"), b.loc("xs"))),
        ],
        b.if_(
            b.bin(BinOp::Lt, b.loc("len"), b.loc("lo")),
            under_min,
            take_cnt,
        ),
    );
    let body = b.block(
        [fail_if_done(b)],
        b.if_(invalid_range, b.list(vec![]), read_and_take),
    );
    b.lam(&["lo", "hi", "q"], in_tx(b, read_state(b, body)))
}

fn tx_queue_shutdown(b: &ExprBuilder) -> Expr {
    let drain = b.block(
        [
            b.let_("xs", b.app(b.global("TxRef", "get"), b.loc("iref"))),
            tq_rest_stmt(
                b,
                b.loc("xs"),
                b.app(b.global("List.Core", "length"), b.loc("xs")),
            ),
        ],
        b.unit(),
    );
    let maybe_drain = b.if_(
        b.not(b.bin(BinOp::Eq, b.loc("tag"), b.int(2))),
        drain,
        b.unit(),
    );
    let body = b.block(
        [Stmt::Expr(maybe_drain)],
        b.app(b.global("TxQueue", "interrupt"), b.loc("q")),
    );
    b.lam(&["q"], in_tx(b, read_state(b, body)))
}

fn tx_queue_peek(b: &ExprBuilder) -> Expr {
    let head_pat = Pat::List(vec![Pat::Bind(b.name("h"))], Some(Box::new(Pat::Wild)));
    let body = b.block(
        [
            fail_if_done(b),
            b.let_("xs", b.app(b.global("TxRef", "get"), b.loc("iref"))),
        ],
        b.match_(
            b.loc("xs"),
            [
                arm(head_pat, b.loc("h")),
                arm(Pat::Wild, b.app(b.global("Tx", "retry"), b.unit())),
            ],
        ),
    );
    b.lam(&["q"], in_tx(b, read_state(b, body)))
}

fn tx_queue_take_n(b: &ExprBuilder) -> Expr {
    let clamped = b.if_(
        b.bin(
            BinOp::And,
            b.bin(BinOp::Ge, b.loc("cap"), b.int(0)),
            b.bin(BinOp::Gt, b.loc("n"), b.loc("cap")),
        ),
        b.loc("cap"),
        b.loc("n"),
    );
    let body = b.block(
        [b.let_("k", clamped)],
        b.app3(
            b.global("TxQueue", "take_between"),
            b.loc("k"),
            b.loc("k"),
            b.loc("q"),
        ),
    );
    b.lam(&["n", "q"], in_tx(b, body))
}

fn tx_queue_clear(b: &ExprBuilder) -> Expr {
    let body = b.block(
        [
            fail_if_done(b),
            b.let_("xs", b.app(b.global("TxRef", "get"), b.loc("iref"))),
            tq_rest_stmt(
                b,
                b.loc("xs"),
                b.app(b.global("List.Core", "length"), b.loc("xs")),
            ),
        ],
        b.loc("xs"),
    );
    b.lam(&["q"], in_tx(b, read_state(b, body)))
}

pub(super) fn tx_queue_ops_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("takeN", 2) => Some(tx_queue_take_n(b)),
        ("takeBetween", 3) => Some(tx_queue_take_between(b)),
        ("peek", 1) => Some(tx_queue_peek(b)),
        ("size", 1) => Some(b.lam(
            &["q"],
            in_tx(
                b,
                b.app(
                    b.global("List.Core", "length"),
                    b.app(b.global("TxRef", "get"), b.loc("iref")),
                ),
            ),
        )),
        ("isEmpty", 1) => Some(b.lam(
            &["q"],
            b.bin(
                BinOp::Eq,
                b.app(b.global("TxQueue", "size"), b.loc("q")),
                b.int(0),
            ),
        )),
        ("isNonEmpty", 1) => Some(b.lam(
            &["q"],
            b.bin(
                BinOp::Gt,
                b.app(b.global("TxQueue", "size"), b.loc("q")),
                b.int(0),
            ),
        )),
        ("isFull", 1) => {
            let len = b.app(
                b.global("List.Core", "length"),
                b.app(b.global("TxRef", "get"), b.loc("iref")),
            );
            let full = b.bin(
                BinOp::And,
                b.bin(BinOp::Ge, b.loc("cap"), b.int(0)),
                b.bin(BinOp::Ge, len, b.loc("cap")),
            );
            Some(b.lam(&["q"], in_tx(b, full)))
        }
        ("interrupt", 1) => Some(b.lam(
            &["q"],
            close_with_cause(b, b.app(b.global("Cause", "interrupt"), b.none()), false),
        )),
        ("fail", 2) => Some(b.lam(
            &["e", "q"],
            close_with_cause(b, b.app(b.global("Cause", "fail"), b.loc("e")), true),
        )),
        ("flickFailCause", 2) => Some(b.lam(&["c", "q"], close_with_cause(b, b.loc("c"), false))),
        ("clear", 1) => Some(tx_queue_clear(b)),
        ("shutdown", 1) => Some(tx_queue_shutdown(b)),
        ("isOpen", 1) => Some(b.lam(
            &["q"],
            in_tx(b, read_state(b, b.bin(BinOp::Eq, b.loc("tag"), b.int(0)))),
        )),
        ("isClosing", 1) => Some(b.lam(
            &["q"],
            in_tx(b, read_state(b, b.bin(BinOp::Eq, b.loc("tag"), b.int(1)))),
        )),
        ("isDone" | "isShutdown", 1) => Some(b.lam(
            &["q"],
            in_tx(b, read_state(b, b.bin(BinOp::Eq, b.loc("tag"), b.int(2)))),
        )),
        ("awaitCompletion", 1) => Some(b.lam(
            &["q"],
            in_tx(
                b,
                read_state(
                    b,
                    b.if_(
                        b.bin(BinOp::Eq, b.loc("tag"), b.int(2)),
                        b.unit(),
                        b.app(b.global("Tx", "retry"), b.unit()),
                    ),
                ),
            ),
        )),
        _ => None,
    }
}
