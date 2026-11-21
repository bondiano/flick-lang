//! `Stream.Combine` queue-backed partition helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat, Stmt};

pub(super) fn st_queue_shutdown_stream_helper(b: &ExprBuilder) -> Expr {
    let shutdown = b.block(
        [b.let_("_sh", b.app(b.global("Queue", "shutdown"), b.loc("q")))],
        b.unit(),
    );
    let reg = b.app2(
        rt(b, "Layer.OwnerRun"),
        b.loc("own"),
        b.task(b.app(b.global("Scope", "add_finalizer"), b.task(shutdown))),
    );
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.app(b.global("Queue", "take_all"), b.loc("q")),
            [
                arm(b.some_pat("xs"), b.ok(b.loc("xs"))),
                arm(Pat::Wild, b.err(b.unit())),
            ],
        ),
    );
    b.lam(&["q", "_up", "own"], b.block([Stmt::Expr(reg)], pull))
}

fn offer_if_non_empty(b: &ExprBuilder, list_var: &str, queue_loc: &str) -> Stmt {
    let offer = b.block(
        [b.let_(
            "_o",
            b.app2(
                b.global("Queue", "offer_all"),
                b.var(list_var),
                b.loc(queue_loc),
            ),
        )],
        b.unit(),
    );
    let has_items = b.bin(
        BinOp::Gt,
        b.app(b.global("List.Core", "length"), b.var(list_var)),
        b.int(0),
    );
    Stmt::Expr(b.if_(has_items, offer, b.unit()))
}

fn partition_chunk_handler(b: &ExprBuilder) -> Expr {
    let append_p = b.assign_e(
        "ps",
        b.app2(
            b.global("List.Core", "append"),
            b.var("ps"),
            b.list([b.loc("p")]),
        ),
    );
    let append_f = b.assign_e(
        "fs",
        b.app2(
            b.global("List.Core", "append"),
            b.var("fs"),
            b.list([b.loc("f")]),
        ),
    );
    let split_elem = b.match_(
        b.app(b.loc("flt"), b.loc("a")),
        [arm(b.ok_pat("p"), append_p), arm(b.err_pat("f"), append_f)],
    );
    let stmts = [
        b.var_decl("ps", b.list(vec![])),
        b.var_decl("fs", b.list(vec![])),
        Stmt::Expr(b.each("a", b.loc("xs"), split_elem)),
        offer_if_non_empty(b, "ps", "pq"),
        offer_if_non_empty(b, "fs", "fq"),
    ];
    b.lam(&["xs"], b.block(stmts, b.unit()))
}

pub(super) fn st_partition_queue_helper(b: &ExprBuilder) -> Expr {
    let shutdown_both = b.block(
        [
            b.let_("_s1", b.app(b.global("Queue", "shutdown"), b.loc("pq"))),
            b.let_("_s2", b.app(b.global("Queue", "shutdown"), b.loc("fq"))),
        ],
        b.unit(),
    );
    let fail_both = b.lam(
        &["c"],
        b.block(
            [
                b.let_(
                    "_f1",
                    b.app2(b.global("Queue", "fail_cause"), b.loc("c"), b.loc("pq")),
                ),
                b.let_(
                    "_f2",
                    b.app2(b.global("Queue", "fail_cause"), b.loc("c"), b.loc("fq")),
                ),
            ],
            b.unit(),
        ),
    );
    let run_body = b.block(
        [
            b.let_(
                "_d",
                b.app2(
                    rt(b, "Helper.ch_run_for_each"),
                    partition_chunk_handler(b),
                    b.loc("self"),
                ),
            ),
            b.let_("_e1", b.app(b.global("Queue", "end"), b.loc("pq"))),
            b.let_("_e2", b.app(b.global("Queue", "end"), b.loc("fq"))),
        ],
        b.unit(),
    );
    let fiber = b.app3(
        b.global("Fiber", "spawn"),
        b.loc("ow"),
        b.lam(&["_c"], b.unit()),
        b.task(b.app2(
            b.global("Effect.Errors", "catch_cause"),
            fail_both,
            b.task(run_body),
        )),
    );
    let stmts = [
        b.let_("ow", b.app(b.global("Scope", "current"), b.unit())),
        b.let_("pq", b.app(b.global("Queue", "unbounded"), b.unit())),
        b.let_("fq", b.app(b.global("Queue", "unbounded"), b.unit())),
        Stmt::Expr(b.app(b.global("Scope", "add_finalizer"), b.task(shutdown_both))),
        b.let_("_fib", fiber),
    ];
    b.lam(
        &["flt", "self"],
        b.block(stmts, b.tuple([b.loc("pq"), b.loc("fq")])),
    )
}

pub(super) fn st_partition_helper(b: &ExprBuilder) -> Expr {
    let wrap = b.tuple([
        b.app(rt(b, "Helper.st_queue_shutdown_stream"), b.loc("pq")),
        b.app(rt(b, "Helper.st_queue_shutdown_stream"), b.loc("fq")),
    ]);
    let body = b.match_(
        b.app2(
            rt(b, "Helper.st_partition_queue"),
            b.loc("flt"),
            b.loc("self"),
        ),
        [arm(pair_pat(b, "pq", "fq"), wrap)],
    );
    b.lam(&["_eff", "flt", "self"], body)
}
