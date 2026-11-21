//! `TxSubscriptionRef` pure IR adapter templates.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};

pub(super) fn tx_subref_changes_stream(b: &ExprBuilder) -> Expr {
    let open_q = b.app2(
        b.global("$rt", "Layer.OwnerRun"),
        b.loc("own"),
        b.task(b.app(b.global("TxSubscriptionRef", "changes"), b.loc("s"))),
    );
    let init_q = b.block(
        [
            b.let_("q1", open_q),
            b.rset(b.loc("qr"), b.some(b.loc("q1"))),
        ],
        b.loc("q1"),
    );
    let get_q = b.match_(
        b.rget(b.loc("qr")),
        [arm(b.some_pat("q0"), b.loc("q0")), arm(Pat::Wild, init_q)],
    );
    let pull = b.lam(
        &["_u"],
        b.block(
            [b.let_("q", get_q)],
            b.ok(b.list([b.app(b.global("TxQueue", "take"), b.loc("q"))])),
        ),
    );
    b.lam(
        &["s", "_up", "own"],
        b.block([b.let_("qr", b.rmake(b.none()))], pull),
    )
}

fn subref_modify(b: &ExprBuilder, pair_expr: Expr) -> Expr {
    let commit = b.block(
        [
            Stmt::Expr(b.app2(b.global("TxRef", "set"), b.loc("nxt"), b.loc("r"))),
            b.let_(
                "_pb",
                b.app2(b.global("TxPubSub", "publish"), b.loc("nxt"), b.loc("ps")),
            ),
        ],
        b.loc("out"),
    );
    let step = b.block(
        [b.let_("cur", b.app(b.global("TxRef", "get"), b.loc("r")))],
        b.match_(pair_expr, [arm(b.tup_pat(&["out", "nxt"]), commit)]),
    );
    b.app(
        b.global("Tx", "atomically"),
        b.task(b.match_(b.loc("s"), [arm(b.tup_pat(&["r", "ps"]), step)])),
    )
}

pub(super) fn tx_subref_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("make", 1) => Some(b.lam(
            &["v"],
            b.tuple([
                b.app(b.global("TxRef", "make"), b.loc("v")),
                b.app(b.global("TxPubSub", "unbounded"), b.unit()),
            ]),
        )),
        ("get", 1) => Some(b.lam(
            &["s"],
            b.match_(
                b.loc("s"),
                [arm(
                    b.tup_pat(&["r", "_ps"]),
                    b.app(b.global("TxRef", "get"), b.loc("r")),
                )],
            ),
        )),
        ("modify", 2) => Some(b.lam(
            &["f", "s"],
            subref_modify(b, b.app(b.loc("f"), b.loc("cur"))),
        )),
        ("set", 2) => Some(b.lam(
            &["x", "s"],
            subref_modify(b, b.tuple([b.unit(), b.loc("x")])),
        )),
        ("update", 2) => Some(b.lam(
            &["f", "s"],
            subref_modify(b, b.tuple([b.unit(), b.app(b.loc("f"), b.loc("cur"))])),
        )),
        ("getAndSet", 2) => Some(b.lam(
            &["x", "s"],
            subref_modify(b, b.tuple([b.loc("cur"), b.loc("x")])),
        )),
        ("getAndUpdate", 2) => Some(b.lam(
            &["f", "s"],
            subref_modify(b, b.tuple([b.loc("cur"), b.app(b.loc("f"), b.loc("cur"))])),
        )),
        ("updateAndGet", 2) => {
            let pair = b.block(
                [b.let_("n", b.app(b.loc("f"), b.loc("cur")))],
                b.tuple([b.loc("n"), b.loc("n")]),
            );
            Some(b.lam(&["f", "s"], subref_modify(b, pair)))
        }
        ("changes", 1) => {
            let offer_cur = b.app(
                b.global("Tx", "atomically"),
                b.task(b.app2(
                    b.global("TxQueue", "offer"),
                    b.app(b.global("TxRef", "get"), b.loc("r")),
                    b.loc("sq"),
                )),
            );
            let body = b.block(
                [
                    b.let_("sq", b.app(b.global("TxPubSub", "subscribe"), b.loc("ps"))),
                    b.let_("_o", offer_cur),
                ],
                b.loc("sq"),
            );
            Some(b.lam(
                &["s"],
                b.match_(b.loc("s"), [arm(b.tup_pat(&["r", "ps"]), body)]),
            ))
        }
        _ => None,
    }
}
