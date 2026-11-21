//! `Effect.tx` (`Tx.atomically`) journal commit, retry-wait, and conflict loop templates.

use flick_ir::{Arith, BinOp, Num};

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, ExprKind, Pat, Stmt};
use super::{j_pat, meta_pat, now_ms, tref_pat, tx_ctx_ref};

fn retry_reason_con(b: &ExprBuilder, name: &str) -> Expr {
    b.at(ExprKind::Con(ExprBuilder::con(
        "Tx",
        "RetryReason",
        name,
        0,
    )))
}

fn is_consistent_expr(b: &ExprBuilder, ents: Expr) -> Expr {
    let check_ref = b.match_(
        b.rget(b.loc("tr")),
        [arm(
            tref_pat(b),
            b.bin(BinOp::Eq, b.loc("rver"), b.loc("ever")),
        )],
    );
    let pred = b.lam(
        &["e"],
        b.match_(
            b.loc("e"),
            [arm(b.tup_pat(&["tr", "_ev", "ever", "_ew"]), check_ref)],
        ),
    );
    b.app2(b.global("List.Core", "all"), pred, ents)
}

fn bump_meta_expr(b: &ExprBuilder, reason_expr: Expr) -> Expr {
    let next_meta = b.tuple([
        b.add_i(b.loc("att"), b.int(1)),
        b.some(reason_expr),
        b.loc("st"),
        b.loc("t_now"),
        b.bin(
            BinOp::Arith(Arith::Sub, Num::Float),
            b.loc("t_now"),
            b.loc("st"),
        ),
        b.bin(
            BinOp::Arith(Arith::Sub, Num::Float),
            b.loc("t_now"),
            b.loc("nw"),
        ),
    ]);
    b.match_(
        b.loc("meta"),
        [arm(
            meta_pat(b),
            b.block([b.let_("t_now", now_ms(b))], next_meta),
        )],
    )
}

fn await_retry_expr(b: &ExprBuilder) -> Expr {
    let new_waiters = b.app2(
        b.global("List.Core", "append"),
        b.loc("rws"),
        b.list([b.loc("wd")]),
    );
    let add_waiter = b.match_(
        b.rget(b.loc("tr")),
        [arm(
            tref_pat(b),
            b.block(
                [b.rset(
                    b.loc("tr"),
                    b.tuple([b.loc("rv"), b.loc("rver"), new_waiters]),
                )],
                b.unit(),
            ),
        )],
    );
    let register = b.for_ctrl(
        "txreg",
        "e",
        b.loc("ents"),
        b.match_(
            b.loc("e"),
            [arm(b.tup_pat(&["tr", "_ev", "_ever", "_ew"]), add_waiter)],
        ),
    );
    let reg_opt = b.no_preempt(b.if_(
        is_consistent_expr(b, b.loc("ents")),
        b.block(
            [b.let_("wd", b.dmake()), b.let_("_rg", register)],
            b.some(b.loc("wd")),
        ),
        b.none(),
    ));
    b.match_(
        reg_opt,
        [
            arm(b.some_pat("wd"), b.dawait(b.loc("wd"))),
            arm(Pat::Wild, b.unit()),
        ],
    )
}

fn commit_entries_expr(b: &ExprBuilder) -> Expr {
    let updated_ref = b.tuple([
        b.loc("ev"),
        b.add_i(b.loc("rver"), b.int(1)),
        b.list(vec![]),
    ]);
    let append_wakes = b.app2(
        b.global("List.Core", "append"),
        b.var("to_wake"),
        b.loc("rws"),
    );
    let write_ref = b.match_(
        b.rget(b.loc("tr")),
        [arm(
            tref_pat(b),
            b.block(
                [
                    b.rset(b.loc("tr"), updated_ref),
                    b.assign("to_wake", append_wakes),
                ],
                b.unit(),
            ),
        )],
    );
    let apply_one = b.for_ctrl(
        "txcm",
        "e",
        b.loc("ents"),
        b.match_(
            b.loc("e"),
            [arm(
                b.tup_pat(&["tr", "ev", "_ever", "ew"]),
                b.if_(b.loc("ew"), write_ref, b.unit()),
            )],
        ),
    );
    let wake_all = b.for_ctrl(
        "txwk",
        "wd",
        b.var("to_wake"),
        b.block([b.dsucceed(b.unit(), b.loc("wd"))], b.unit()),
    );
    b.block(
        [
            b.var_decl("to_wake", b.list(vec![])),
            b.let_("_cm", apply_one),
            b.let_("_wk", wake_all),
        ],
        b.unit(),
    )
}

fn reset_for_next_expr(b: &ExprBuilder, reason: Expr) -> Expr {
    b.block(
        [b.rset(
            b.loc("j"),
            b.tuple([
                b.list(vec![]),
                b.bool_(false),
                b.bool_(false),
                b.bool_(false),
                bump_meta_expr(b, reason),
            ]),
        )],
        b.unit(),
    )
}

fn done_journal_stmt(b: &ExprBuilder) -> Stmt {
    b.rset(
        b.loc("j"),
        b.tuple([
            b.list(vec![]),
            b.bool_(false),
            b.bool_(false),
            b.bool_(true),
            b.loc("meta"),
        ]),
    )
}

fn tx_step_expr(b: &ExprBuilder) -> Expr {
    let is_retry = b.bin(
        BinOp::And,
        b.loc("rtry"),
        b.bin(
            BinOp::And,
            b.not(b.loc("conf")),
            b.app(b.global("Cause", "has_interrupts_only"), b.loc("c")),
        ),
    );
    let is_conflict = b.bin(
        BinOp::Or,
        b.loc("conf"),
        b.not(is_consistent_expr(b, b.loc("ents"))),
    );
    let rethrow_fail = b.block(
        [done_journal_stmt(b)],
        b.app(b.global("Failure", "rethrow"), b.loc("c")),
    );
    let on_fail = b.if_(
        is_retry,
        b.block(
            [Stmt::Expr(await_retry_expr(b))],
            reset_for_next_expr(b, retry_reason_con(b, "Retry")),
        ),
        b.if_(
            is_conflict,
            reset_for_next_expr(b, retry_reason_con(b, "Conflict")),
            rethrow_fail,
        ),
    );
    let can_commit = b.bin(
        BinOp::And,
        b.not(b.loc("rtry")),
        b.bin(
            BinOp::And,
            b.not(b.loc("conf")),
            is_consistent_expr(b, b.loc("ents")),
        ),
    );
    let atomic_commit = b.no_preempt(b.if_(
        can_commit,
        b.block(
            [Stmt::Expr(commit_entries_expr(b)), done_journal_stmt(b)],
            b.bool_(true),
        ),
        b.bool_(false),
    ));
    let on_ok = b.if_(
        atomic_commit,
        b.break_("txl", b.loc("out")),
        reset_for_next_expr(b, retry_reason_con(b, "Conflict")),
    );
    let on_exit_fail = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("ex")),
        [
            arm(b.some_pat("c"), on_fail),
            arm(
                Pat::Wild,
                b.app(b.global("Effect.Core", "die"), b.str_("tx exit")),
            ),
        ],
    );
    let handle_exit = b.match_(
        b.app(b.global("Exit", "value"), b.loc("ex")),
        [arm(b.some_pat("out"), on_ok), arm(Pat::Wild, on_exit_fail)],
    );
    let run_body = b.app3(
        b.global("Context", "with_reference"),
        b.loc("cref"),
        b.some(b.loc("j")),
        b.loc("body"),
    );
    b.block(
        [b.let_(
            "ex",
            b.app(b.global("Effect.Core", "exit"), b.task(run_body)),
        )],
        b.match_(b.rget(b.loc("j")), [arm(j_pat(b), handle_exit)]),
    )
}

pub(super) fn tx_atomically_template(b: &ExprBuilder) -> Expr {
    let mark_done = b.match_(
        b.rget(b.loc("j")),
        [arm(j_pat(b), b.block([done_journal_stmt(b)], b.unit()))],
    );
    let init_meta = b.tuple([
        b.int(1),
        b.none(),
        b.loc("t0"),
        b.loc("t0"),
        b.float(0.0),
        b.float(0.0),
    ]);
    let init_journal = b.rmake(b.tuple([
        b.list(vec![]),
        b.bool_(false),
        b.bool_(false),
        b.bool_(false),
        init_meta,
    ]));
    let run_fresh = b.block(
        [b.let_("t0", now_ms(b)), b.let_("j", init_journal)],
        b.app2(
            b.global("Effect.Errors", "ensuring"),
            b.task(mark_done),
            b.task(b.loop_ctrl("txl", tx_step_expr(b))),
        ),
    );
    let on_existing = b.match_(
        b.rget(b.loc("existing")),
        [arm(
            j_pat(b),
            b.if_(b.loc("done"), run_fresh.clone(), b.await_(b.loc("body"))),
        )],
    );
    b.lam(
        &["body"],
        b.block(
            [b.let_("cref", tx_ctx_ref(b))],
            b.match_(
                b.app(b.global("Context", "read"), b.loc("cref")),
                [
                    arm(b.some_pat("existing"), on_existing),
                    arm(Pat::Wild, run_fresh),
                ],
            ),
        ),
    )
}
