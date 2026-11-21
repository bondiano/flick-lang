//! `TxHashSet` pure IR adapter templates.

use flick_ir::BinOp;

use super::super::expr_builder::ExprBuilder;
use super::super::{Expr, Stmt};

fn get_s(b: &ExprBuilder) -> Expr {
    b.app(b.global("TxRef", "get"), b.loc("s"))
}

fn tx_hashset_remove(b: &ExprBuilder) -> Expr {
    let do_remove = b.app2(
        b.global("TxRef", "set"),
        b.app2(b.global("HashSet", "remove"), b.loc("x"), b.loc("cur")),
        b.loc("s"),
    );
    let body = b.block(
        [
            b.let_("cur", get_s(b)),
            b.let_(
                "ex",
                b.app2(b.global("HashSet", "member"), b.loc("x"), b.loc("cur")),
            ),
            Stmt::Expr(b.if_(b.loc("ex"), do_remove, b.unit())),
        ],
        b.loc("ex"),
    );
    b.lam(
        &["x", "s"],
        b.app(b.global("Tx", "atomically"), b.task(body)),
    )
}

fn tx_hashset_set_ops(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickUnion", 2) => Some(b.lam(
            &["that", "s"],
            b.app(
                b.global("Tx", "atomically"),
                b.task(b.app(
                    b.global("TxRef", "make"),
                    b.app2(
                        b.global("HashSet", "union"),
                        get_s(b),
                        b.app(b.global("TxRef", "get"), b.loc("that")),
                    ),
                )),
            ),
        )),
        ("flickIntersection", 2) => Some(b.lam(
            &["that", "s"],
            b.app(
                b.global("Tx", "atomically"),
                b.task(b.app(
                    b.global("TxRef", "make"),
                    b.app2(
                        b.global("HashSet", "intersection"),
                        b.app(b.global("TxRef", "get"), b.loc("that")),
                        get_s(b),
                    ),
                )),
            ),
        )),
        ("flickDifference", 2) => Some(b.lam(
            &["that", "s"],
            b.app(
                b.global("Tx", "atomically"),
                b.task(b.app(
                    b.global("TxRef", "make"),
                    b.app2(
                        b.global("HashSet", "difference"),
                        b.app(b.global("TxRef", "get"), b.loc("that")),
                        get_s(b),
                    ),
                )),
            ),
        )),
        ("flickIsSubset", 2) => Some(b.lam(
            &["that", "s"],
            b.app(
                b.global("Tx", "atomically"),
                b.task(b.app2(
                    b.global("HashSet", "is_subset"),
                    b.app(b.global("TxRef", "get"), b.loc("that")),
                    get_s(b),
                )),
            ),
        )),
        ("flickSome", 2) => Some(b.lam(
            &["f", "s"],
            b.app2(b.global("HashSet", "some"), b.loc("f"), get_s(b)),
        )),
        ("flickEvery", 2) => Some(b.lam(
            &["f", "s"],
            b.app2(b.global("HashSet", "every"), b.loc("f"), get_s(b)),
        )),
        ("flickMap", 3) => Some(b.lam(
            &["w", "f", "s"],
            b.app(
                b.global("TxRef", "make"),
                b.app3(b.global("HashSet", "map"), b.loc("w"), b.loc("f"), get_s(b)),
            ),
        )),
        ("flickFilter", 2) => Some(b.lam(
            &["f", "s"],
            b.app(
                b.global("TxRef", "make"),
                b.app2(b.global("HashSet", "filter"), b.loc("f"), get_s(b)),
            ),
        )),
        ("flickReduce", 3) => Some(b.lam(
            &["z", "f", "s"],
            b.app3(
                b.global("HashSet", "reduce"),
                b.loc("z"),
                b.loc("f"),
                get_s(b),
            ),
        )),
        ("flickToHashSet", 1) => Some(b.lam(&["s"], get_s(b))),
        _ => None,
    }
}

pub(super) fn tx_hashset_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickEmpty", 1) => Some(b.lam(
            &["w"],
            b.app(
                b.global("TxRef", "make"),
                b.app(b.global("HashSet", "with"), b.loc("w")),
            ),
        )),
        ("flickMake" | "flickFromIterable", 2) => Some(b.lam(
            &["w", "xs"],
            b.app(
                b.global("TxRef", "make"),
                b.app2(b.global("HashSet", "from_list"), b.loc("w"), b.loc("xs")),
            ),
        )),
        ("flickFromHashSet", 1) => {
            Some(b.lam(&["hs"], b.app(b.global("TxRef", "make"), b.loc("hs"))))
        }
        ("flickAdd", 2) => Some(b.lam(
            &["x", "s"],
            b.app2(
                b.global("TxRef", "update"),
                b.lam(
                    &["hs"],
                    b.app2(b.global("HashSet", "insert"), b.loc("x"), b.loc("hs")),
                ),
                b.loc("s"),
            ),
        )),
        ("flickRemove", 2) => Some(tx_hashset_remove(b)),
        ("flickHas", 2) => Some(b.lam(
            &["x", "s"],
            b.app2(b.global("HashSet", "member"), b.loc("x"), get_s(b)),
        )),
        ("flickSize", 1) => Some(b.lam(&["s"], b.app(b.global("HashSet", "size"), get_s(b)))),
        ("flickIsEmpty", 1) => {
            Some(b.lam(&["s"], b.app(b.global("HashSet", "is_empty"), get_s(b))))
        }
        ("flickIsNonEmpty", 1) => Some(b.lam(
            &["s"],
            b.bin(
                BinOp::Gt,
                b.app(b.global("HashSet", "size"), get_s(b)),
                b.int(0),
            ),
        )),
        ("flickClear", 1) => Some(b.lam(
            &["s"],
            b.app2(
                b.global("TxRef", "update"),
                b.lam(
                    &["hs"],
                    b.app2(
                        b.global("HashSet", "filter"),
                        b.lam(&["_x"], b.bool_(false)),
                        b.loc("hs"),
                    ),
                ),
                b.loc("s"),
            ),
        )),
        _ => tx_hashset_set_ops(b, bind, stages),
    }
}
