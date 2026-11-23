//! `TxPriorityQueue` pure IR adapter templates.

use flick_ir::{BinOp, Dict};

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, ExprKind, Pat, Stmt};

pub(super) fn tx_pq_dict_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    let cmp_method = b.at(ExprKind::Method {
        dict: Dict::Field(b.name("ord")),
        name: "compare".into(),
        type_args: Vec::new(),
    });
    match (bind, stages) {
        ("flickEmpty", 1) => Some(b.lam(
            &["ord", "_u"],
            b.tuple([cmp_method, b.app(b.global("TxRef", "make"), b.list(vec![]))]),
        )),
        ("flickFromIterable" | "flickMake", 1) => Some(b.lam(
            &["ord", "xs"],
            b.block(
                [b.let_("cmp", cmp_method)],
                b.tuple([
                    b.loc("cmp"),
                    b.app(
                        b.global("TxRef", "make"),
                        b.app2(b.global("List.Core", "sort_by"), b.loc("cmp"), b.loc("xs")),
                    ),
                ]),
            ),
        )),
        _ => None,
    }
}

fn split_pq(b: &ExprBuilder, body: Expr) -> Expr {
    b.match_(b.loc("q"), [arm(b.tup_pat(&["cmp", "r"]), body)])
}

fn tx_pq_take(b: &ExprBuilder) -> Expr {
    let cons_pat = Pat::List(
        vec![Pat::Bind(b.name("h"))],
        Some(Box::new(Pat::Bind(b.name("tl")))),
    );
    let pop = b.match_(
        b.app(b.global("TxRef", "get"), b.loc("r")),
        [
            arm(
                cons_pat,
                b.block(
                    [Stmt::Expr(b.app2(
                        b.global("TxRef", "set"),
                        b.loc("tl"),
                        b.loc("r"),
                    ))],
                    b.loc("h"),
                ),
            ),
            arm(Pat::Wild, b.app(b.global("Tx", "retry"), b.unit())),
        ],
    );
    b.lam(
        &["q"],
        b.app(b.global("Tx", "atomically"), b.task(split_pq(b, pop))),
    )
}

fn tx_pq_take_option(b: &ExprBuilder) -> Expr {
    let cons_pat = Pat::List(
        vec![Pat::Bind(b.name("h"))],
        Some(Box::new(Pat::Bind(b.name("tl")))),
    );
    let step = b.lam(
        &["xs"],
        b.match_(
            b.loc("xs"),
            [
                arm(cons_pat, b.tuple([b.some(b.loc("h")), b.loc("tl")])),
                arm(Pat::Wild, b.tuple([b.none(), b.list(vec![])])),
            ],
        ),
    );
    b.lam(
        &["q"],
        split_pq(b, b.app2(b.global("TxRef", "modify"), step, b.loc("r"))),
    )
}

fn tx_pq_mutators(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("take", 1) => Some(tx_pq_take(b)),
        ("takeAll", 1) => Some(b.lam(
            &["q"],
            split_pq(
                b,
                b.app2(
                    b.global("TxRef", "modify"),
                    b.lam(&["xs"], b.tuple([b.loc("xs"), b.list(vec![])])),
                    b.loc("r"),
                ),
            ),
        )),
        ("takeOption", 1) => Some(tx_pq_take_option(b)),
        ("takeUpTo", 2) => {
            let step = b.lam(
                &["xs"],
                b.tuple([
                    b.app2(b.global("List.Core", "take"), b.loc("n"), b.loc("xs")),
                    b.app2(b.global("List.Core", "drop"), b.loc("n"), b.loc("xs")),
                ]),
            );
            Some(b.lam(
                &["n", "q"],
                split_pq(b, b.app2(b.global("TxRef", "modify"), step, b.loc("r"))),
            ))
        }
        ("removeIf", 2) => {
            let keep = b.lam(&["x"], b.not(b.app(b.loc("f"), b.loc("x"))));
            let step = b.lam(
                &["xs"],
                b.app2(b.global("List.Core", "filter"), keep, b.loc("xs")),
            );
            Some(b.lam(
                &["f", "q"],
                split_pq(b, b.app2(b.global("TxRef", "update"), step, b.loc("r"))),
            ))
        }
        ("retainIf", 2) => {
            let step = b.lam(
                &["xs"],
                b.app2(b.global("List.Core", "filter"), b.loc("f"), b.loc("xs")),
            );
            Some(b.lam(
                &["f", "q"],
                split_pq(b, b.app2(b.global("TxRef", "update"), step, b.loc("r"))),
            ))
        }
        ("toArray", 1) => Some(b.lam(
            &["q"],
            split_pq(b, b.app(b.global("TxRef", "get"), b.loc("r"))),
        )),
        _ => None,
    }
}

pub(super) fn tx_pq_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("size", 1) => Some(b.lam(
            &["q"],
            split_pq(
                b,
                b.app(
                    b.global("List.Core", "length"),
                    b.app(b.global("TxRef", "get"), b.loc("r")),
                ),
            ),
        )),
        ("isEmpty", 1) => Some(b.lam(
            &["q"],
            b.bin(
                BinOp::Eq,
                b.app(b.global("TxPriorityQueue", "size"), b.loc("q")),
                b.int(0),
            ),
        )),
        ("isNonEmpty", 1) => Some(b.lam(
            &["q"],
            b.bin(
                BinOp::Gt,
                b.app(b.global("TxPriorityQueue", "size"), b.loc("q")),
                b.int(0),
            ),
        )),
        ("peekOption", 1) => {
            let head_pat = Pat::List(vec![Pat::Bind(b.name("h"))], Some(Box::new(Pat::Wild)));
            let body = b.match_(
                b.app(b.global("TxRef", "get"), b.loc("r")),
                [arm(head_pat, b.some(b.loc("h"))), arm(Pat::Wild, b.none())],
            );
            Some(b.lam(&["q"], split_pq(b, body)))
        }
        ("peek", 1) => {
            let wait = b.match_(
                b.app(b.global("TxPriorityQueue", "peek_option"), b.loc("q")),
                [
                    arm(b.some_pat("h"), b.loc("h")),
                    arm(Pat::Wild, b.app(b.global("Tx", "retry"), b.unit())),
                ],
            );
            Some(b.lam(&["q"], b.app(b.global("Tx", "atomically"), b.task(wait))))
        }
        ("offer", 2) => {
            let step = b.lam(
                &["xs"],
                b.app2(
                    b.global("List.Core", "sort_by"),
                    b.loc("cmp"),
                    b.app2(
                        b.global("List.Core", "append"),
                        b.loc("xs"),
                        b.list([b.loc("x")]),
                    ),
                ),
            );
            Some(b.lam(
                &["x", "q"],
                split_pq(b, b.app2(b.global("TxRef", "update"), step, b.loc("r"))),
            ))
        }
        ("offerAll", 2) => {
            let step = b.lam(
                &["xs"],
                b.app2(
                    b.global("List.Core", "sort_by"),
                    b.loc("cmp"),
                    b.app2(b.global("List.Core", "append"), b.loc("xs"), b.loc("ys")),
                ),
            );
            Some(b.lam(
                &["ys", "q"],
                split_pq(b, b.app2(b.global("TxRef", "update"), step, b.loc("r"))),
            ))
        }
        _ => tx_pq_mutators(b, bind, stages),
    }
}
