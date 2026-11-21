//! Audited `TxReentrantLock` pure IR adapter templates.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};

// TxReentrantLock: `TxRef (readers : List (Float, Int), writer : Float, writes : Int)`
fn lk_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["rds", "wr", "wcnt"])
}

fn fiber_id_expr(b: &ExprBuilder) -> Expr {
    b.app(b.global("Effect.Concurrency", "fiber_id"), b.unit())
}

fn reader_count_expr(b: &ExprBuilder, rds: Expr, fid: Expr) -> Expr {
    b.match_(
        b.app2(
            b.global("List.Core", "find"),
            b.lam(
                &["p"],
                b.match_(
                    b.loc("p"),
                    vec![arm(
                        b.tup_pat(&["f", "_c"]),
                        b.bin(BinOp::Eq, b.loc("f"), fid),
                    )],
                ),
            ),
            rds,
        ),
        vec![
            arm(
                b.some_pat("p"),
                b.match_(b.loc("p"), vec![arm(b.tup_pat(&["_f", "c"]), b.loc("c"))]),
            ),
            arm(Pat::Wild, b.int(0)),
        ],
    )
}

fn set_reader_count_expr(b: &ExprBuilder, rds: Expr, fid: Expr, cnt: Expr) -> Expr {
    let without = b.app2(
        b.global("List.Core", "filter"),
        b.lam(
            &["p"],
            b.match_(
                b.loc("p"),
                vec![arm(
                    b.tup_pat(&["f", "_c"]),
                    b.not(b.bin(BinOp::Eq, b.loc("f"), fid.clone())),
                )],
            ),
        ),
        rds,
    );
    b.if_(
        b.bin(BinOp::Le, cnt.clone(), b.int(0)),
        without.clone(),
        b.app2(
            b.global("List.Core", "append"),
            without,
            b.list(vec![b.tuple(vec![fid, cnt])]),
        ),
    )
}

fn acquire_read_for(b: &ExprBuilder, fid: Expr) -> Expr {
    b.app(
        b.global("Tx", "atomically"),
        b.task(b.match_(
            b.app(b.global("TxRef", "get"), b.loc("l")),
            vec![arm(
                lk_pat(b),
                b.if_(
                    b.bin(
                        BinOp::And,
                        b.not(b.bin(BinOp::Eq, b.loc("wr"), b.float(0.0))),
                        b.not(b.bin(BinOp::Eq, b.loc("wr"), fid.clone())),
                    ),
                    b.app(b.global("Tx", "retry"), b.unit()),
                    b.block(
                        vec![
                            b.let_(
                                "n",
                                b.add_i(reader_count_expr(b, b.loc("rds"), fid.clone()), b.int(1)),
                            ),
                            Stmt::Expr(b.app2(
                                b.global("TxRef", "set"),
                                b.tuple(vec![
                                    set_reader_count_expr(b, b.loc("rds"), fid, b.loc("n")),
                                    b.loc("wr"),
                                    b.loc("wcnt"),
                                ]),
                                b.loc("l"),
                            )),
                        ],
                        b.loc("n"),
                    ),
                ),
            )],
        )),
    )
}

fn release_read_for(b: &ExprBuilder, fid: Expr) -> Expr {
    b.app(
        b.global("Tx", "atomically"),
        b.task(b.match_(
            b.app(b.global("TxRef", "get"), b.loc("l")),
            vec![arm(
                lk_pat(b),
                b.block(
                    vec![b.let_("cur", reader_count_expr(b, b.loc("rds"), fid.clone()))],
                    b.if_(
                        b.bin(BinOp::Le, b.loc("cur"), b.int(0)),
                        b.int(0),
                        b.block(
                            vec![
                                b.let_("n", b.sub_i(b.loc("cur"), b.int(1))),
                                Stmt::Expr(b.app2(
                                    b.global("TxRef", "set"),
                                    b.tuple(vec![
                                        set_reader_count_expr(b, b.loc("rds"), fid, b.loc("n")),
                                        b.loc("wr"),
                                        b.loc("wcnt"),
                                    ]),
                                    b.loc("l"),
                                )),
                            ],
                            b.loc("n"),
                        ),
                    ),
                ),
            )],
        )),
    )
}

fn acquire_write_for(b: &ExprBuilder, fid: Expr) -> Expr {
    let other_reader = b.app2(
        b.global("List.Core", "any"),
        b.lam(
            &["p"],
            b.match_(
                b.loc("p"),
                vec![arm(
                    b.tup_pat(&["f", "c"]),
                    b.bin(
                        BinOp::And,
                        b.not(b.bin(BinOp::Eq, b.loc("f"), fid.clone())),
                        b.bin(BinOp::Gt, b.loc("c"), b.int(0)),
                    ),
                )],
            ),
        ),
        b.loc("rds"),
    );
    b.app(
        b.global("Tx", "atomically"),
        b.task(b.match_(
            b.app(b.global("TxRef", "get"), b.loc("l")),
            vec![arm(
                lk_pat(b),
                b.if_(
                    b.bin(
                        BinOp::Or,
                        b.bin(
                            BinOp::And,
                            b.not(b.bin(BinOp::Eq, b.loc("wr"), b.float(0.0))),
                            b.not(b.bin(BinOp::Eq, b.loc("wr"), fid.clone())),
                        ),
                        other_reader,
                    ),
                    b.app(b.global("Tx", "retry"), b.unit()),
                    b.block(
                        vec![
                            b.let_(
                                "n",
                                b.if_(
                                    b.bin(BinOp::Eq, b.loc("wr"), b.float(0.0)),
                                    b.int(1),
                                    b.add_i(b.loc("wcnt"), b.int(1)),
                                ),
                            ),
                            Stmt::Expr(b.app2(
                                b.global("TxRef", "set"),
                                b.tuple(vec![b.loc("rds"), fid, b.loc("n")]),
                                b.loc("l"),
                            )),
                        ],
                        b.loc("n"),
                    ),
                ),
            )],
        )),
    )
}

fn release_write_for(b: &ExprBuilder, fid: Expr) -> Expr {
    b.app(
        b.global("Tx", "atomically"),
        b.task(b.match_(
            b.app(b.global("TxRef", "get"), b.loc("l")),
            vec![arm(
                lk_pat(b),
                b.if_(
                    b.bin(
                        BinOp::Or,
                        b.bin(BinOp::Eq, b.loc("wr"), b.float(0.0)),
                        b.not(b.bin(BinOp::Eq, b.loc("wr"), fid.clone())),
                    ),
                    b.int(0),
                    b.block(
                        vec![
                            b.let_("n", b.sub_i(b.loc("wcnt"), b.int(1))),
                            Stmt::Expr(b.app2(
                                b.global("TxRef", "set"),
                                b.if_(
                                    b.bin(BinOp::Le, b.loc("n"), b.int(0)),
                                    b.tuple(vec![b.loc("rds"), b.float(0.0), b.int(0)]),
                                    b.tuple(vec![b.loc("rds"), fid, b.loc("n")]),
                                ),
                                b.loc("l"),
                            )),
                        ],
                        b.loc("n"),
                    ),
                ),
            )],
        )),
    )
}

fn scoped_lock_expr(b: &ExprBuilder, acquire: Expr, release: Expr) -> Expr {
    b.lam(
        &["l"],
        b.block(
            vec![
                b.let_("id", fiber_id_expr(b)),
                b.let_("n", acquire),
                Stmt::Expr(b.app(
                    b.global("Scope", "add_finalizer"),
                    b.task(b.block(vec![b.let_("_r", release)], b.unit())),
                )),
            ],
            b.loc("n"),
        ),
    )
}

fn with_lock_expr(b: &ExprBuilder, acquire: Expr, release: Expr) -> Expr {
    b.lam(
        &["body", "l"],
        b.block(
            vec![b.let_("id", fiber_id_expr(b)), b.let_("_n", acquire)],
            b.app2(
                b.global("Effect.Errors", "ensuring"),
                b.task(b.block(vec![b.let_("_r", release)], b.unit())),
                b.loc("body"),
            ),
        ),
    )
}

fn tx_lock_query_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("readLocks", 1) => Some(b.lam(
            &["l"],
            b.match_(
                b.app(b.global("TxRef", "get"), b.loc("l")),
                vec![arm(
                    lk_pat(b),
                    b.app3(
                        b.global("List.Core", "fold_left"),
                        b.lam(
                            &["acc", "p"],
                            b.match_(
                                b.loc("p"),
                                vec![arm(
                                    b.tup_pat(&["_f", "c"]),
                                    b.add_i(b.loc("acc"), b.loc("c")),
                                )],
                            ),
                        ),
                        b.int(0),
                        b.loc("rds"),
                    ),
                )],
            ),
        )),
        ("writeLocks", 1) => Some(b.lam(
            &["l"],
            b.match_(
                b.app(b.global("TxRef", "get"), b.loc("l")),
                vec![arm(lk_pat(b), b.loc("wcnt"))],
            ),
        )),
        ("readLocked", 1) => Some(b.lam(
            &["l"],
            b.match_(
                b.app(b.global("TxRef", "get"), b.loc("l")),
                vec![arm(
                    lk_pat(b),
                    b.bin(
                        BinOp::Gt,
                        b.app(b.global("List.Core", "length"), b.loc("rds")),
                        b.int(0),
                    ),
                )],
            ),
        )),
        ("writeLocked", 1) => Some(b.lam(
            &["l"],
            b.match_(
                b.app(b.global("TxRef", "get"), b.loc("l")),
                vec![arm(
                    lk_pat(b),
                    b.not(b.bin(BinOp::Eq, b.loc("wr"), b.float(0.0))),
                )],
            ),
        )),
        ("locked", 1) => Some(b.lam(
            &["l"],
            b.match_(
                b.app(b.global("TxRef", "get"), b.loc("l")),
                vec![arm(
                    lk_pat(b),
                    b.bin(
                        BinOp::Or,
                        b.bin(
                            BinOp::Gt,
                            b.app(b.global("List.Core", "length"), b.loc("rds")),
                            b.int(0),
                        ),
                        b.not(b.bin(BinOp::Eq, b.loc("wr"), b.float(0.0))),
                    ),
                )],
            ),
        )),
        _ => None,
    }
}

pub(super) fn tx_lock_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("make", 1) => Some(b.lam(
            &["_u"],
            b.app(
                b.global("TxRef", "make"),
                b.tuple(vec![b.list(vec![]), b.float(0.0), b.int(0)]),
            ),
        )),
        ("acquireRead", 1) => Some(b.lam(
            &["l"],
            b.block(
                vec![b.let_("id", fiber_id_expr(b))],
                acquire_read_for(b, b.loc("id")),
            ),
        )),
        ("acquireWrite", 1) => Some(b.lam(
            &["l"],
            b.block(
                vec![b.let_("id", fiber_id_expr(b))],
                acquire_write_for(b, b.loc("id")),
            ),
        )),
        ("releaseRead", 1) => Some(b.lam(
            &["l"],
            b.block(
                vec![b.let_("id", fiber_id_expr(b))],
                release_read_for(b, b.loc("id")),
            ),
        )),
        ("releaseWrite", 1) => Some(b.lam(
            &["l"],
            b.block(
                vec![b.let_("id", fiber_id_expr(b))],
                release_write_for(b, b.loc("id")),
            ),
        )),
        ("readLock", 1) => Some(scoped_lock_expr(
            b,
            acquire_read_for(b, b.loc("id")),
            release_read_for(b, b.loc("id")),
        )),
        ("writeLock", 1) => Some(scoped_lock_expr(
            b,
            acquire_write_for(b, b.loc("id")),
            release_write_for(b, b.loc("id")),
        )),
        ("withReadLock", 2) => Some(with_lock_expr(
            b,
            acquire_read_for(b, b.loc("id")),
            release_read_for(b, b.loc("id")),
        )),
        ("withWriteLock" | "withLock", 2) => Some(with_lock_expr(
            b,
            acquire_write_for(b, b.loc("id")),
            release_write_for(b, b.loc("id")),
        )),
        _ => tx_lock_query_template(b, bind, stages),
    }
}
