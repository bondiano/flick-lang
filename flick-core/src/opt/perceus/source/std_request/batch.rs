//! Batch settlement, execution, and delayed fiber spawning for `Request`.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};
use super::{
    batch_pat, complete_entry_expr, filter_out_batch, filter_out_entry_by_st, fork_detached,
    has_entry_by_st,
};

fn settle_in_batch(b: &ExprBuilder) -> Expr {
    let mark_done = b.rset(
        b.loc("st_ref"),
        b.tuple([b.bool_(true), b.loc("un"), b.none()]),
    );
    let already_removed = b.block([mark_done.clone()], b.bool_(false));
    let filtered = filter_out_entry_by_st(b, b.loc("bpnd"), b.loc("st_ref"));
    let updated_batch = b.tuple([
        b.loc("bk"),
        b.loc("bents"),
        filtered,
        b.loc("brid"),
        b.loc("bst"),
        b.loc("bfib"),
    ]);
    let remove_and_mark = b.block(
        [b.rset(b.loc("bref"), updated_batch), mark_done],
        b.bool_(true),
    );
    let not_pending = b.not(has_entry_by_st(b, b.loc("bpnd"), b.loc("st_ref")));
    let body = b.if_(not_pending, already_removed, remove_and_mark);
    b.match_(b.rget(b.loc("bref")), [arm(batch_pat(b), body)])
}

pub(super) fn make_settle_expr(b: &ExprBuilder) -> Expr {
    let no_batch = b.block(
        [b.rset(
            b.loc("st_ref"),
            b.tuple([b.bool_(true), b.loc("un"), b.none()]),
        )],
        b.bool_(true),
    );
    let bopt_match = b.match_(
        b.loc("bopt"),
        [
            arm(b.some_pat("bref"), settle_in_batch(b)),
            arm(Pat::Wild, no_batch),
        ],
    );
    let check_state = b.if_(b.loc("cmp"), b.bool_(false), bopt_match);
    let atomic = b.no_preempt(b.match_(
        b.rget(b.loc("st_ref")),
        [arm(b.tup_pat(&["cmp", "un", "bopt"]), check_state)],
    ));
    let complete_dn = b.block([b.dsucceed(b.loc("ex"), b.loc("dn"))], b.unit());
    let finish = b.if_(b.loc("should_complete"), complete_dn, b.unit());
    b.lam(
        &["ex"],
        b.block([b.let_("should_complete", atomic)], finish),
    )
}

fn claim_batch_expr(b: &ExprBuilder) -> Expr {
    let skip = b.bin(
        BinOp::Or,
        b.bin(BinOp::NotEq, b.loc("brid"), b.loc("target_rid")),
        b.loc("bst"),
    );
    let started_batch = b.tuple([
        b.loc("bk"),
        b.loc("bents"),
        b.loc("bpnd"),
        b.loc("brid"),
        b.bool_(true),
        b.none(),
    ]);
    let filtered = filter_out_batch(b, b.rget(b.loc("r_pnd")), b.loc("bref"));
    let claim = b.block(
        [
            b.rset(b.loc("bref"), started_batch),
            b.rset(b.loc("r_pnd"), filtered),
        ],
        b.some(b.tuple([b.loc("bk"), b.loc("bents")])),
    );
    let body = b.if_(skip, b.none(), claim);
    b.no_preempt(b.match_(b.rget(b.loc("bref")), [arm(batch_pat(b), body)]))
}

fn execute_claimed_batch(b: &ExprBuilder) -> Expr {
    let run_call = b.app2(b.loc("r_run"), b.loc("cents"), b.loc("ck"));
    let rex = b.let_(
        "rex",
        b.app(b.global("Effect.Core", "exit"), b.task(run_call)),
    );
    let die_msg = "Error: Effect.request: RequestResolver did not complete request";
    let fallback_ex = b.let_(
        "fallback_ex",
        b.match_(
            b.app(b.global("Exit", "failure"), b.loc("rex")),
            [
                arm(
                    b.some_pat("fc"),
                    b.app(b.global("Exit", "fail_cause"), b.loc("fc")),
                ),
                arm(Pat::Wild, b.app(b.global("Exit", "die"), b.str_(die_msg))),
            ],
        ),
    );
    let cleared_batch = b.tuple([
        b.loc("bk"),
        b.list(vec![]),
        b.loc("bpnd"),
        b.loc("brid"),
        b.bool_(true),
        b.none(),
    ]);
    let take_rest = b.block([b.rset(b.loc("bref"), cleared_batch)], b.loc("bpnd"));
    let rest = b.let_(
        "rest",
        b.no_preempt(b.match_(b.rget(b.loc("bref")), [arm(batch_pat(b), take_rest)])),
    );
    let complete_rest = b.for_ctrl(
        "rrem",
        "pe",
        b.loc("rest"),
        complete_entry_expr(b, b.loc("fallback_ex"), b.loc("pe")),
    );
    let unmask_tail = b.app(
        b.global("Effect.Concurrency", "uninterruptible_mask"),
        b.lam(&["_rst"], complete_rest),
    );
    b.match_(
        b.loc("pair"),
        [arm(
            b.tup_pat(&["ck", "cents"]),
            b.block([rex, fallback_ex, rest], unmask_tail),
        )],
    )
}

pub(super) fn run_batch_expr(b: &ExprBuilder) -> Expr {
    // Runs batch `bref` if `brid == target_rid && !bst`.
    let dispatch = b.match_(
        b.loc("claimed"),
        [
            arm(b.some_pat("pair"), execute_claimed_batch(b)),
            arm(Pat::Wild, b.unit()),
        ],
    );
    b.lam(
        &["bref", "target_rid"],
        b.block([b.let_("claimed", claim_batch_expr(b))], dispatch),
    )
}

pub(super) fn fork_batch_expr(b: &ExprBuilder, bref: Expr, delay_fn: Expr) -> Expr {
    let bumped_batch = b.tuple([
        b.loc("bk"),
        b.loc("bents"),
        b.loc("bpnd"),
        b.loc("nr"),
        b.loc("bst"),
        b.loc("bfib"),
    ]);
    let bump_rid = b.block(
        [
            b.let_("nr", b.add_i(b.loc("brid"), b.int(1))),
            b.rset(b.loc("br"), bumped_batch),
        ],
        b.loc("nr"),
    );
    let nrid = b.no_preempt(b.match_(b.rget(b.loc("br")), [arm(batch_pat(b), bump_rid)]));
    let delayed_run = b.task(b.block(
        [Stmt::Expr(b.app(b.loc("df"), b.unit()))],
        b.app2(b.loc("run_batch"), b.loc("br"), b.loc("nrid")),
    ));
    let fib_batch = b.tuple([
        b.loc("bk"),
        b.loc("bents"),
        b.loc("bpnd"),
        b.loc("brid"),
        b.loc("bst"),
        b.some(b.loc("bf")),
    ]);
    let store_fib = b.if_(
        b.bin(BinOp::Eq, b.loc("brid"), b.loc("nrid")),
        b.block([b.rset(b.loc("br"), fib_batch)], b.unit()),
        b.unit(),
    );
    let record_fib = b.no_preempt(b.match_(b.rget(b.loc("br")), [arm(batch_pat(b), store_fib)]));
    b.block(
        [
            b.let_("br", bref),
            b.let_("df", delay_fn),
            b.let_("nrid", nrid),
            b.let_("bf", fork_detached(b, delayed_run)),
            Stmt::Expr(record_fib),
        ],
        b.unit(),
    )
}
