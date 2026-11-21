//! `Tx`, `TxRef`, and `TxDeferred` pure IR operation templates.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, ExprKind, Pat, Stmt};
use super::{meta_pat, tref_pat, with_active_tx};

pub(super) fn tx_ops_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickRetry", 1) => {
            let mark_retry = b.rset(
                b.loc("j"),
                b.tuple([
                    b.loc("ents"),
                    b.bool_(true),
                    b.loc("conf"),
                    b.loc("done"),
                    b.loc("meta"),
                ]),
            );
            let interrupt = b.app(
                b.global("Failure", "rethrow"),
                b.app(b.global("Cause", "interrupt"), b.none()),
            );
            Some(b.lam(&["_u"], with_active_tx(b, b.block([mark_retry], interrupt))))
        }
        ("flickCurrent", 1) => {
            let rec = b.at(ExprKind::Record(vec![
                ("attempt".into(), b.loc("att")),
                ("retryReason".into(), b.loc("rsn")),
                ("start".into(), b.loc("st")),
                ("now".into(), b.loc("nw")),
                ("elapsed".into(), b.loc("el")),
                ("elapsedSincePrevious".into(), b.loc("sp")),
            ]));
            Some(b.lam(
                &["_u"],
                with_active_tx(b, b.match_(b.loc("meta"), [arm(meta_pat(b), rec)])),
            ))
        }
        _ => None,
    }
}

fn find_journal_entry(b: &ExprBuilder, ents: Expr, tr: Expr) -> Expr {
    let pred = b.lam(
        &["e"],
        b.match_(
            b.loc("e"),
            [arm(
                b.tup_pat(&["er", "_ev", "_ever", "_ew"]),
                b.app2(b.global("Ref", "same"), b.loc("er"), tr),
            )],
        ),
    );
    b.app2(b.global("List.Core", "find"), pred, ents)
}

fn tx_ref_read_in_j(b: &ExprBuilder) -> Expr {
    let on_found = b.match_(
        b.loc("found"),
        [arm(b.tup_pat(&["_er", "ev", "_ever", "_ew"]), b.loc("ev"))],
    );
    let new_entry = b.tuple([b.loc("r"), b.loc("rv"), b.loc("rver"), b.bool_(false)]);
    let appended = b.app2(
        b.global("List.Core", "append"),
        b.loc("ents"),
        b.list([new_entry]),
    );
    let record_read = b.block(
        [b.rset(
            b.loc("j"),
            b.tuple([
                appended,
                b.loc("rtry"),
                b.loc("conf"),
                b.loc("done"),
                b.loc("meta"),
            ]),
        )],
        b.loc("rv"),
    );
    let on_miss = b.match_(b.rget(b.loc("r")), [arm(tref_pat(b), record_read)]);
    b.match_(
        find_journal_entry(b, b.loc("ents"), b.loc("r")),
        [arm(b.some_pat("found"), on_found), arm(Pat::Wild, on_miss)],
    )
}

fn tx_ref_write_in_j(b: &ExprBuilder) -> Expr {
    let replace_one = b.if_(
        b.app2(b.global("Ref", "same"), b.loc("er"), b.loc("r")),
        b.tuple([b.loc("er"), b.loc("v"), b.loc("ever"), b.bool_(true)]),
        b.tuple([b.loc("er"), b.loc("ev"), b.loc("ever"), b.loc("ew")]),
    );
    let map_fn = b.lam(
        &["e"],
        b.match_(
            b.loc("e"),
            [arm(b.tup_pat(&["er", "ev", "ever", "ew"]), replace_one)],
        ),
    );
    let on_found = b.block(
        [
            b.let_(
                "ents2",
                b.app2(b.global("List", "map"), map_fn, b.loc("ents")),
            ),
            b.rset(
                b.loc("j"),
                b.tuple([
                    b.loc("ents2"),
                    b.loc("rtry"),
                    b.loc("conf"),
                    b.loc("done"),
                    b.loc("meta"),
                ]),
            ),
        ],
        b.unit(),
    );
    let new_entry = b.tuple([b.loc("r"), b.loc("v"), b.loc("rver"), b.bool_(true)]);
    let appended = b.app2(
        b.global("List.Core", "append"),
        b.loc("ents"),
        b.list([new_entry]),
    );
    let record_write = b.block(
        [b.rset(
            b.loc("j"),
            b.tuple([
                appended,
                b.loc("rtry"),
                b.loc("conf"),
                b.loc("done"),
                b.loc("meta"),
            ]),
        )],
        b.unit(),
    );
    let on_miss = b.match_(b.rget(b.loc("r")), [arm(tref_pat(b), record_write)]);
    b.match_(
        find_journal_entry(b, b.loc("ents"), b.loc("r")),
        [arm(b.some_pat("_found"), on_found), arm(Pat::Wild, on_miss)],
    )
}

pub(super) fn tx_ref_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("make", 1) => Some(b.lam(
            &["v"],
            b.rmake(b.tuple([b.loc("v"), b.int(0), b.list(vec![])])),
        )),
        ("get", 1) => Some(b.lam(
            &["r"],
            b.app(
                b.global("Tx", "atomically"),
                b.task(with_active_tx(b, tx_ref_read_in_j(b))),
            ),
        )),
        ("set", 2) => Some(b.lam(
            &["v", "r"],
            b.app(
                b.global("Tx", "atomically"),
                b.task(with_active_tx(b, tx_ref_write_in_j(b))),
            ),
        )),
        ("update", 2) => Some(b.lam(
            &["f", "r"],
            b.app(
                b.global("Tx", "atomically"),
                b.task(b.app2(
                    b.global("TxRef", "set"),
                    b.app(b.loc("f"), b.app(b.global("TxRef", "get"), b.loc("r"))),
                    b.loc("r"),
                )),
            ),
        )),
        ("modify", 2) => {
            let apply = b.match_(
                b.app(b.loc("f"), b.app(b.global("TxRef", "get"), b.loc("r"))),
                [arm(
                    b.tup_pat(&["out", "nxt"]),
                    b.block(
                        [Stmt::Expr(b.app2(
                            b.global("TxRef", "set"),
                            b.loc("nxt"),
                            b.loc("r"),
                        ))],
                        b.loc("out"),
                    ),
                )],
            );
            Some(b.lam(
                &["f", "r"],
                b.app(b.global("Tx", "atomically"), b.task(apply)),
            ))
        }
        _ => None,
    }
}

pub(super) fn tx_deferred_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("make", 1) => Some(b.lam(&["_u"], b.app(b.global("TxRef", "make"), b.none()))),
        ("poll", 1) => Some(b.lam(&["d"], b.app(b.global("TxRef", "get"), b.loc("d")))),
        ("await", 1) => {
            let unwrap = b.match_(
                b.loc("res"),
                [
                    arm(b.ok_pat("v"), b.loc("v")),
                    arm(b.err_pat("e"), b.app(b.global("Fail", "fail"), b.loc("e"))),
                ],
            );
            let wait = b.match_(
                b.app(b.global("TxRef", "get"), b.loc("d")),
                [
                    arm(b.some_pat("res"), unwrap),
                    arm(Pat::Wild, b.app(b.global("Tx", "retry"), b.unit())),
                ],
            );
            Some(b.lam(&["d"], b.app(b.global("Tx", "atomically"), b.task(wait))))
        }
        ("done", 2) => {
            let step = b.lam(
                &["cur"],
                b.match_(
                    b.loc("cur"),
                    [
                        arm(b.some_pat("_v"), b.tuple([b.bool_(false), b.loc("cur")])),
                        arm(Pat::Wild, b.tuple([b.bool_(true), b.some(b.loc("res"))])),
                    ],
                ),
            );
            Some(b.lam(
                &["res", "d"],
                b.app2(b.global("TxRef", "modify"), step, b.loc("d")),
            ))
        }
        ("succeed", 2) => Some(b.lam(
            &["v", "d"],
            b.app2(b.global("TxDeferred", "done"), b.ok(b.loc("v")), b.loc("d")),
        )),
        ("fail", 2) => Some(b.lam(
            &["e", "d"],
            b.app2(
                b.global("TxDeferred", "done"),
                b.err(b.loc("e")),
                b.loc("d"),
            ),
        )),
        _ => None,
    }
}
