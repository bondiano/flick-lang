//! `TxQueue` shared helpers, constructors, offer, and basic take/poll templates.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};
use super::queue_ops::tx_queue_ops_template;

// TxQueue: `(strat : Int, cap : Int, items : TxRef (List a), state : TxRef (tag : Int, opt_cause : Option Cause))`
// - `strat`: 0 Suspend, 1 Dropping, 2 Sliding; `cap`: -1 unbounded
// - `tag`: 0 Open, 1 Closing, 2 Done
pub(super) fn tq_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["strat", "cap", "iref", "sref"])
}

pub(super) fn st_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["tag", "oc"])
}

pub(super) fn in_tx(b: &ExprBuilder, body: Expr) -> Expr {
    b.app(
        b.global("Tx", "atomically"),
        b.task(b.match_(b.loc("q"), [arm(tq_pat(b), body)])),
    )
}

pub(super) fn read_state(b: &ExprBuilder, body: Expr) -> Expr {
    b.match_(
        b.app(b.global("TxRef", "get"), b.loc("sref")),
        [arm(st_pat(b), body)],
    )
}

fn tq_make(b: &ExprBuilder, strat: i32, cap: Expr) -> Expr {
    b.tuple([
        b.int(strat),
        cap,
        b.app(b.global("TxRef", "make"), b.list(vec![])),
        b.app(b.global("TxRef", "make"), b.tuple([b.int(0), b.none()])),
    ])
}

pub(super) fn fail_if_done(b: &ExprBuilder) -> Stmt {
    let rethrow = b.match_(
        b.loc("oc"),
        [
            arm(
                b.some_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
            arm(
                Pat::Wild,
                b.app(
                    b.global("Failure", "rethrow"),
                    b.app(b.global("Cause", "interrupt"), b.none()),
                ),
            ),
        ],
    );
    Stmt::Expr(b.if_(b.bin(BinOp::Eq, b.loc("tag"), b.int(2)), rethrow, b.unit()))
}

pub(super) fn tq_rest_stmt(b: &ExprBuilder, xs_expr: Expr, n_expr: Expr) -> Stmt {
    let drained_closing = b.bin(
        BinOp::And,
        b.bin(BinOp::Eq, b.loc("tag"), b.int(1)),
        b.bin(
            BinOp::Ge,
            n_expr.clone(),
            b.app(b.global("List.Core", "length"), xs_expr.clone()),
        ),
    );
    let mark_done = b.app2(
        b.global("TxRef", "set"),
        b.tuple([b.int(2), b.loc("oc")]),
        b.loc("sref"),
    );
    b.let_(
        "_rst",
        b.block(
            [
                b.let_(
                    "rem",
                    b.app2(b.global("List.Core", "drop"), n_expr, xs_expr),
                ),
                Stmt::Expr(b.app2(b.global("TxRef", "set"), b.loc("rem"), b.loc("iref"))),
                Stmt::Expr(b.if_(drained_closing, mark_done, b.unit())),
            ],
            b.unit(),
        ),
    )
}

fn tx_queue_offer(b: &ExprBuilder) -> Expr {
    let push_ok = b.block(
        [Stmt::Expr(b.app2(
            b.global("TxRef", "set"),
            b.app2(
                b.global("List.Core", "append"),
                b.loc("xs"),
                b.list([b.loc("x")]),
            ),
            b.loc("iref"),
        ))],
        b.bool_(true),
    );
    let slide_list = b.app2(
        b.global("List.Core", "append"),
        b.app2(b.global("List.Core", "drop"), b.int(1), b.loc("xs")),
        b.list([b.loc("x")]),
    );
    let slide_ok = b.block(
        [Stmt::Expr(b.app2(
            b.global("TxRef", "set"),
            slide_list,
            b.loc("iref"),
        ))],
        b.bool_(true),
    );
    let on_full = b.if_(
        b.bin(BinOp::Eq, b.loc("strat"), b.int(1)),
        b.bool_(false),
        b.if_(
            b.bin(BinOp::Eq, b.loc("strat"), b.int(2)),
            slide_ok,
            b.app(b.global("Tx", "retry"), b.unit()),
        ),
    );
    let has_space = b.bin(
        BinOp::Or,
        b.bin(BinOp::Lt, b.loc("cap"), b.int(0)),
        b.bin(BinOp::Lt, b.loc("len"), b.loc("cap")),
    );
    let when_open = b.block(
        [
            b.let_("xs", b.app(b.global("TxRef", "get"), b.loc("iref"))),
            b.let_("len", b.app(b.global("List.Core", "length"), b.loc("xs"))),
        ],
        b.if_(has_space, push_ok, on_full),
    );
    let body = b.if_(
        b.not(b.bin(BinOp::Eq, b.loc("tag"), b.int(0))),
        b.bool_(false),
        when_open,
    );
    b.lam(&["x", "q"], in_tx(b, read_state(b, body)))
}

fn tx_queue_offer_all(b: &ExprBuilder) -> Expr {
    let push_rej = b.block(
        [b.assign(
            "rej",
            b.app2(
                b.global("List.Core", "append"),
                b.var("rej"),
                b.list([b.loc("x")]),
            ),
        )],
        b.unit(),
    );
    let step = b.if_(
        b.app2(b.global("TxQueue", "offer"), b.loc("x"), b.loc("q")),
        b.unit(),
        push_rej,
    );
    let body = b.block(
        [
            b.var_decl("rej", b.list(vec![])),
            b.let_("_oa", b.for_ctrl("txoa", "x", b.loc("xs"), step)),
        ],
        b.var("rej"),
    );
    b.lam(
        &["xs", "q"],
        b.app(b.global("Tx", "atomically"), b.task(body)),
    )
}

fn tx_queue_take(b: &ExprBuilder) -> Expr {
    let head_pat = Pat::List(vec![Pat::Bind(b.name("h"))], Some(Box::new(Pat::Wild)));
    let body = b.block(
        [
            fail_if_done(b),
            b.let_("xs", b.app(b.global("TxRef", "get"), b.loc("iref"))),
        ],
        b.match_(
            b.loc("xs"),
            [
                arm(
                    head_pat,
                    b.block([tq_rest_stmt(b, b.loc("xs"), b.int(1))], b.loc("h")),
                ),
                arm(Pat::Wild, b.app(b.global("Tx", "retry"), b.unit())),
            ],
        ),
    );
    b.lam(&["q"], in_tx(b, read_state(b, body)))
}

fn tx_queue_poll(b: &ExprBuilder) -> Expr {
    let head_pat = Pat::List(vec![Pat::Bind(b.name("h"))], Some(Box::new(Pat::Wild)));
    let pop = b.block(
        [b.let_("xs", b.app(b.global("TxRef", "get"), b.loc("iref")))],
        b.match_(
            b.loc("xs"),
            [
                arm(
                    head_pat,
                    b.block([tq_rest_stmt(b, b.loc("xs"), b.int(1))], b.some(b.loc("h"))),
                ),
                arm(Pat::Wild, b.none()),
            ],
        ),
    );
    let body = b.if_(b.bin(BinOp::Eq, b.loc("tag"), b.int(2)), b.none(), pop);
    b.lam(&["q"], in_tx(b, read_state(b, body)))
}

fn tx_queue_take_all(b: &ExprBuilder) -> Expr {
    let cons_pat = Pat::List(
        vec![Pat::Bind(b.name("h"))],
        Some(Box::new(Pat::Bind(b.name("tl")))),
    );
    let take_all = b.block(
        [tq_rest_stmt(
            b,
            b.loc("xs"),
            b.app(b.global("List.Core", "length"), b.loc("xs")),
        )],
        b.app2(b.global("NonEmpty", "cons"), b.loc("h"), b.loc("tl")),
    );
    let body = b.block(
        [
            fail_if_done(b),
            b.let_("xs", b.app(b.global("TxRef", "get"), b.loc("iref"))),
        ],
        b.match_(
            b.loc("xs"),
            [
                arm(cons_pat, take_all),
                arm(Pat::Wild, b.app(b.global("Tx", "retry"), b.unit())),
            ],
        ),
    );
    b.lam(&["q"], in_tx(b, read_state(b, body)))
}

pub(super) fn tx_queue_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("bounded", 1) => Some(b.lam(&["c"], tq_make(b, 0, b.loc("c")))),
        ("dropping", 1) => Some(b.lam(&["c"], tq_make(b, 1, b.loc("c")))),
        ("sliding", 1) => Some(b.lam(&["c"], tq_make(b, 2, b.loc("c")))),
        ("unbounded", 1) => Some(b.lam(&["_u"], tq_make(b, 0, b.int(-1)))),
        ("offer", 2) => Some(tx_queue_offer(b)),
        ("offerAll", 2) => Some(tx_queue_offer_all(b)),
        ("take", 1) => Some(tx_queue_take(b)),
        ("poll", 1) => Some(tx_queue_poll(b)),
        ("takeAll", 1) => Some(tx_queue_take_all(b)),
        _ => tx_queue_ops_template(b, bind, stages),
    }
}
