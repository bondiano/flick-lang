//! `Request.run` entry enqueueing, sorter fiber, and completion wait.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_observe_refs::{KEY_PARENT_SPAN, obs_read};
use super::super::{Expr, Pat, Stmt};
use super::add::add_entry_expr;
use super::batch::{make_settle_expr, run_batch_expr};
use super::cancel::on_cancel_expr;
use super::{fork_detached, res_pat};

fn sort_task_expr(b: &ExprBuilder) -> Expr {
    let clear_win = b.block(
        [b.rset(b.loc("r_win"), b.tuple([b.list(vec![]), b.bool_(false)]))],
        b.loc("wents"),
    );
    let drain_win = b.match_(
        b.rget(b.loc("r_win")),
        [arm(b.tup_pat(&["wents", "_wsrt"]), clear_win)],
    );
    let sort_loop = b.for_ctrl(
        "rsw",
        "we",
        b.loc("batch_win"),
        b.app(b.loc("add_entry"), b.loc("we")),
    );
    let atomic_sort = b.no_preempt(b.block([b.let_("batch_win", drain_win)], sort_loop));
    let stop_loop = b.for_ctrl(
        "rstp",
        "sf",
        b.rget(b.loc("to_stop")),
        b.app(b.global("Fiber", "interrupt"), b.loc("sf")),
    );
    b.task(b.block(
        [
            b.let_("to_stop", b.rmake(b.list(vec![]))),
            b.let_("run_batch", run_batch_expr(b)),
            b.let_("add_entry", add_entry_expr(b)),
            Stmt::Expr(atomic_sort),
        ],
        stop_loop,
    ))
}

fn setup_entry_expr(b: &ExprBuilder) -> Expr {
    let init_st = b.tuple([b.bool_(false), b.bool_(false), b.none()]);
    let ent_tup = b.tuple([
        b.loc("req"),
        b.loc("sp"),
        b.loc("cmp_ref"),
        b.loc("st_ref"),
        b.loc("dn"),
    ]);
    let appended = b.app2(
        b.global("List.Core", "append"),
        b.loc("wents"),
        b.list([b.loc("ent")]),
    );
    let enqueue = b.block(
        [b.rset(b.loc("r_win"), b.tuple([appended, b.bool_(true)]))],
        b.not(b.loc("wsrt")),
    );
    let need_sort = b.match_(
        b.rget(b.loc("r_win")),
        [arm(b.tup_pat(&["wents", "wsrt"]), enqueue)],
    );
    let stmts = [
        b.let_("dn", b.dmake()),
        b.let_("sp", obs_read(b, KEY_PARENT_SPAN, b.none())),
        b.let_("st_ref", b.rmake(init_st)),
        b.let_("cmp_ref", b.rmake(make_settle_expr(b))),
        b.let_("ent", ent_tup),
        b.let_("need_sort", need_sort),
    ];
    let result = b.tuple([
        b.loc("dn"),
        b.loc("st_ref"),
        b.loc("ent"),
        b.loc("need_sort"),
    ]);
    b.no_preempt(b.block(stmts, result))
}

fn unwrap_answer_exit(b: &ExprBuilder) -> Expr {
    let rethrow_fail = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("ans_ex")),
        [
            arm(
                b.some_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
            arm(
                Pat::Wild,
                b.app(b.global("Effect.Core", "die"), b.str_("request exit")),
            ),
        ],
    );
    b.match_(
        b.app(b.global("Exit", "value"), b.loc("ans_ex")),
        [
            arm(b.some_pat("v"), b.loc("v")),
            arm(Pat::Wild, rethrow_fail),
        ],
    )
}

fn handle_wait_cancel(b: &ExprBuilder) -> Expr {
    let rethrow_wait = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("wex")),
        [
            arm(
                b.some_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
            arm(
                Pat::Wild,
                b.app(b.global("Effect.Core", "die"), b.str_("request wait")),
            ),
        ],
    );
    b.block([Stmt::Expr(on_cancel_expr(b))], rethrow_wait)
}

pub(super) fn request_run_template(b: &ExprBuilder) -> Expr {
    let maybe_sort = b.if_(
        b.loc("need_sort"),
        b.block(
            [b.let_("_sf", fork_detached(b, sort_task_expr(b)))],
            b.unit(),
        ),
        b.unit(),
    );
    let wait_done = b.let_(
        "wex",
        b.app(
            b.global("Effect.Core", "exit"),
            b.task(b.dawait(b.loc("dn"))),
        ),
    );
    let resolve_wait = b.match_(
        b.app(b.global("Exit", "value"), b.loc("wex")),
        [
            arm(b.some_pat("ans_ex"), unwrap_answer_exit(b)),
            arm(Pat::Wild, handle_wait_cancel(b)),
        ],
    );
    let after_setup = b.block([Stmt::Expr(maybe_sort), wait_done], resolve_wait);
    let run_body = b.block(
        [b.let_("setup", setup_entry_expr(b))],
        b.match_(
            b.loc("setup"),
            [arm(
                b.tup_pat(&["dn", "st_ref", "ent", "need_sort"]),
                after_setup,
            )],
        ),
    );
    b.lam(
        &["res", "req"],
        b.match_(b.loc("res"), [arm(res_pat(b), run_body)]),
    )
}
