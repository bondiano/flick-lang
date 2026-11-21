//! `TxHashMap` pure IR adapter templates.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};

fn get_m(b: &ExprBuilder) -> Expr {
    b.app(b.global("TxRef", "get"), b.loc("m"))
}

fn tx_hashmap_remove(b: &ExprBuilder) -> Expr {
    let do_remove = b.app2(
        b.global("TxRef", "set"),
        b.app2(b.global("HashMap", "remove"), b.loc("k"), b.loc("cur")),
        b.loc("m"),
    );
    let body = b.block(
        [
            b.let_("cur", get_m(b)),
            b.let_(
                "ex",
                b.app2(b.global("HashMap", "has"), b.loc("k"), b.loc("cur")),
            ),
            Stmt::Expr(b.if_(b.loc("ex"), do_remove, b.unit())),
        ],
        b.loc("ex"),
    );
    b.lam(
        &["k", "m"],
        b.app(b.global("Tx", "atomically"), b.task(body)),
    )
}

fn tx_hashmap_modify(b: &ExprBuilder) -> Expr {
    let set_new = b.app2(
        b.global("TxRef", "set"),
        b.app3(
            b.global("HashMap", "insert"),
            b.loc("k"),
            b.app(b.loc("f"), b.loc("v")),
            b.loc("cur"),
        ),
        b.loc("m"),
    );
    let body = b.block(
        [
            b.let_("cur", get_m(b)),
            b.let_(
                "old",
                b.app2(b.global("HashMap", "get"), b.loc("k"), b.loc("cur")),
            ),
            Stmt::Expr(b.match_(
                b.loc("old"),
                [arm(b.some_pat("v"), set_new), arm(Pat::Wild, b.unit())],
            )),
        ],
        b.loc("old"),
    );
    b.lam(
        &["k", "f", "m"],
        b.app(b.global("Tx", "atomically"), b.task(body)),
    )
}

fn tx_hashmap_flat_map(b: &ExprBuilder) -> Expr {
    let empty_acc = b.app2(
        b.global("HashMap", "filter"),
        b.lam(&["_v", "_k"], b.bool_(false)),
        b.loc("cur"),
    );
    let merge_entry = b.block(
        [
            b.let_("sub", b.app2(b.loc("f"), b.loc("ev"), b.loc("ek"))),
            b.let_(
                "ses",
                b.app(
                    b.global("HashMap", "to_list"),
                    b.app(b.global("TxRef", "get"), b.loc("sub")),
                ),
            ),
            b.assign(
                "acc",
                b.app2(
                    b.global("HashMap", "insert_many"),
                    b.loc("ses"),
                    b.var("acc"),
                ),
            ),
        ],
        b.unit(),
    );
    let loop_entries = b.for_ctrl(
        "txfm",
        "p",
        b.app(b.global("HashMap", "to_list"), b.loc("cur")),
        b.match_(b.loc("p"), [arm(b.tup_pat(&["ek", "ev"]), merge_entry)]),
    );
    let body = b.block(
        [
            b.let_("cur", get_m(b)),
            b.var_decl("acc", empty_acc),
            b.let_("_fm", loop_entries),
        ],
        b.app(b.global("TxRef", "make"), b.var("acc")),
    );
    b.lam(
        &["f", "m"],
        b.app(b.global("Tx", "atomically"), b.task(body)),
    )
}

fn make_from_hm_op(b: &ExprBuilder, op: &str) -> Expr {
    b.lam(
        &["f", "m"],
        b.app(
            b.global("TxRef", "make"),
            b.app2(b.global("HashMap", op), b.loc("f"), get_m(b)),
        ),
    )
}

fn tx_hashmap_transforms(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickUnion", 2) => Some(b.lam(
            &["other", "m"],
            b.app2(
                b.global("TxRef", "update"),
                b.lam(
                    &["hm"],
                    b.app2(b.global("HashMap", "union"), b.loc("hm"), b.loc("other")),
                ),
                b.loc("m"),
            ),
        )),
        ("flickRemoveMany", 2) => Some(b.lam(
            &["ks", "m"],
            b.app2(
                b.global("TxRef", "update"),
                b.lam(
                    &["hm"],
                    b.app2(b.global("HashMap", "remove_many"), b.loc("ks"), b.loc("hm")),
                ),
                b.loc("m"),
            ),
        )),
        ("flickSetMany", 2) => Some(b.lam(
            &["es", "m"],
            b.app2(
                b.global("TxRef", "update"),
                b.lam(
                    &["hm"],
                    b.app2(b.global("HashMap", "insert_many"), b.loc("es"), b.loc("hm")),
                ),
                b.loc("m"),
            ),
        )),
        ("flickGetHash", 3) => Some(b.lam(
            &["k", "h", "m"],
            b.app3(
                b.global("HashMap", "get_hash"),
                b.loc("k"),
                b.loc("h"),
                get_m(b),
            ),
        )),
        ("flickHasHash", 3) => Some(b.lam(
            &["k", "h", "m"],
            b.app3(
                b.global("HashMap", "has_hash"),
                b.loc("k"),
                b.loc("h"),
                get_m(b),
            ),
        )),
        ("flickMap", 2) => Some(make_from_hm_op(b, "map")),
        ("flickFilter", 2) => Some(make_from_hm_op(b, "filter")),
        ("flickFilterMap", 2) => Some(make_from_hm_op(b, "filter_map")),
        ("flickCompact", 1) => Some(b.lam(
            &["m"],
            b.app(
                b.global("TxRef", "make"),
                b.app(b.global("HashMap", "compact"), get_m(b)),
            ),
        )),
        ("flickFlatMap", 2) => Some(tx_hashmap_flat_map(b)),
        ("flickReduce", 3) => Some(b.lam(
            &["z", "f", "m"],
            b.app3(
                b.global("HashMap", "reduce"),
                b.loc("z"),
                b.loc("f"),
                get_m(b),
            ),
        )),
        ("flickHasBy" | "flickSome", 2) => Some(b.lam(
            &["f", "m"],
            b.app2(b.global("HashMap", "some"), b.loc("f"), get_m(b)),
        )),
        ("flickEvery", 2) => Some(b.lam(
            &["f", "m"],
            b.app2(b.global("HashMap", "every"), b.loc("f"), get_m(b)),
        )),
        ("flickFindFirst", 2) => Some(b.lam(
            &["f", "m"],
            b.app2(b.global("HashMap", "find_first"), b.loc("f"), get_m(b)),
        )),
        ("flickForEach", 2) => Some(b.lam(
            &["f", "m"],
            b.app(
                b.global("Tx", "atomically"),
                b.task(b.app2(b.global("HashMap", "for_each"), b.loc("f"), get_m(b))),
            ),
        )),
        _ => None,
    }
}

pub(super) fn tx_hashmap_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickEmpty", 1) => Some(b.lam(
            &["w"],
            b.app(
                b.global("TxRef", "make"),
                b.app(b.global("HashMap", "with"), b.loc("w")),
            ),
        )),
        ("flickMake" | "flickFromIterable", 2) => Some(b.lam(
            &["w", "es"],
            b.app(
                b.global("TxRef", "make"),
                b.app2(b.global("HashMap", "from_list"), b.loc("w"), b.loc("es")),
            ),
        )),
        ("flickGet", 2) => Some(b.lam(
            &["k", "m"],
            b.app2(b.global("HashMap", "get"), b.loc("k"), get_m(b)),
        )),
        ("flickSet", 3) => Some(b.lam(
            &["k", "v", "m"],
            b.app2(
                b.global("TxRef", "update"),
                b.lam(
                    &["hm"],
                    b.app3(
                        b.global("HashMap", "insert"),
                        b.loc("k"),
                        b.loc("v"),
                        b.loc("hm"),
                    ),
                ),
                b.loc("m"),
            ),
        )),
        ("flickHas", 2) => Some(b.lam(
            &["k", "m"],
            b.app2(b.global("HashMap", "has"), b.loc("k"), get_m(b)),
        )),
        ("flickRemove", 2) => Some(tx_hashmap_remove(b)),
        ("flickClear", 1) => Some(b.lam(
            &["m"],
            b.app2(
                b.global("TxRef", "update"),
                b.lam(
                    &["hm"],
                    b.app2(
                        b.global("HashMap", "filter"),
                        b.lam(&["_v", "_k"], b.bool_(false)),
                        b.loc("hm"),
                    ),
                ),
                b.loc("m"),
            ),
        )),
        ("flickSize", 1) => Some(b.lam(&["m"], b.app(b.global("HashMap", "size"), get_m(b)))),
        ("flickIsEmpty", 1) => {
            Some(b.lam(&["m"], b.app(b.global("HashMap", "is_empty"), get_m(b))))
        }
        ("flickIsNonEmpty", 1) => Some(b.lam(
            &["m"],
            b.bin(
                BinOp::Gt,
                b.app(b.global("HashMap", "size"), get_m(b)),
                b.int(0),
            ),
        )),
        ("flickModify", 3) => Some(tx_hashmap_modify(b)),
        ("flickModifyAt", 3) => Some(b.lam(
            &["k", "f", "m"],
            b.app2(
                b.global("TxRef", "update"),
                b.lam(
                    &["hm"],
                    b.app3(
                        b.global("HashMap", "modify_at"),
                        b.loc("k"),
                        b.loc("f"),
                        b.loc("hm"),
                    ),
                ),
                b.loc("m"),
            ),
        )),
        ("flickKeys", 1) => Some(b.lam(&["m"], b.app(b.global("HashMap", "keys"), get_m(b)))),
        ("flickValues" | "flickToValues", 1) => {
            Some(b.lam(&["m"], b.app(b.global("HashMap", "values"), get_m(b))))
        }
        ("flickEntries" | "flickToEntries", 1) => {
            Some(b.lam(&["m"], b.app(b.global("HashMap", "to_list"), get_m(b))))
        }
        ("flickSnapshot", 1) => Some(b.lam(&["m"], get_m(b))),
        _ => tx_hashmap_transforms(b, bind, stages),
    }
}
