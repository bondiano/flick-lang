//! `Stream.Combine` `groupBy` / `groupByKey` helper.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_channel_core::DEFAULT_CHUNK_SIZE;
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, ExprKind, Pat, Stmt};

fn group_by_get_or_add(b: &ExprBuilder) -> Expr {
    let check_entry = b.match_(
        b.loc("entry"),
        [arm(
            pair_pat(b, "ek", "eq"),
            b.if_(
                b.bin(BinOp::Eq, b.loc("ek"), b.loc("k")),
                b.block(
                    [b.assign("found", b.some(b.loc("eq")))],
                    b.break_("sgf", b.unit()),
                ),
                b.unit(),
            ),
        )],
    );
    let q_opts = b.at(ExprKind::Record(vec![
        ("capacity".into(), b.some(b.int(DEFAULT_CHUNK_SIZE))),
        ("strategy".into(), b.none()),
    ]));
    let append_group = b.app2(
        b.global("List.Core", "append"),
        b.rget(b.loc("grps")),
        b.list([b.tuple([b.loc("k"), b.loc("q")])]),
    );
    let group_entry = b.tuple([
        b.loc("k"),
        b.app(rt(b, "Helper.st_queue_shutdown_stream"), b.loc("q")),
    ]);
    let create_group = b.block(
        [
            b.let_("q", b.app(b.global("Queue", "make"), q_opts)),
            b.rset(b.loc("grps"), append_group),
            b.let_(
                "_o",
                b.app2(b.global("Queue", "offer"), group_entry, b.loc("out_q")),
            ),
        ],
        b.loc("q"),
    );
    let lookup = b.match_(
        b.var("found"),
        [
            arm(b.some_pat("q"), b.loc("q")),
            arm(Pat::Wild, create_group),
        ],
    );
    let search = b.for_ctrl("sgf", "entry", b.rget(b.loc("grps")), check_entry);
    b.lam(
        &["k"],
        b.block([b.var_decl("found", b.none()), Stmt::Expr(search)], lookup),
    )
}

fn group_by_producer(b: &ExprBuilder) -> Expr {
    let by_key_body = b.block(
        [
            b.let_("q", b.app(b.loc("get_q"), b.app(b.loc("f"), b.loc("a")))),
            b.let_(
                "_o",
                b.app2(b.global("Queue", "offer"), b.loc("a"), b.loc("q")),
            ),
        ],
        b.unit(),
    );
    let by_pair_body = b.match_(
        b.app(b.loc("f"), b.loc("a")),
        [arm(
            pair_pat(b, "k", "v"),
            b.block(
                [
                    b.let_("q", b.app(b.loc("get_q"), b.loc("k"))),
                    b.let_(
                        "_o",
                        b.app2(b.global("Queue", "offer"), b.loc("v"), b.loc("q")),
                    ),
                ],
                b.unit(),
            ),
        )],
    );
    let on_chunk = b.each(
        "a",
        b.loc("xs"),
        b.if_(b.loc("by_key"), by_key_body, by_pair_body),
    );
    let on_end = b.block(
        [b.let_("_e", b.app(b.global("Queue", "end"), b.loc("out_q")))],
        b.break_("sgb", b.unit()),
    );
    let loop_expr = b.loop_ctrl(
        "sgb",
        b.match_(
            b.app(b.loc("pull"), b.unit()),
            [arm(b.ok_pat("xs"), on_chunk), arm(b.err_pat("_d"), on_end)],
        ),
    );
    let on_cause = b.lam(
        &["c"],
        b.block(
            [b.let_(
                "_fc",
                b.app2(b.global("Queue", "fail_cause"), b.loc("c"), b.loc("out_q")),
            )],
            b.unit(),
        ),
    );
    let caught = b.app2(
        b.global("Effect.Errors", "catch_cause"),
        on_cause,
        b.task(loop_expr),
    );
    b.app3(
        b.global("Fiber", "spawn"),
        b.loc("own"),
        b.lam(&["_c"], b.unit()),
        b.task(caught),
    )
}

pub(super) fn st_group_by_helper(b: &ExprBuilder) -> Expr {
    // (by_key : Bool, f, self) -> Stream (k, Stream v)
    let shutdown_out = b.block(
        [b.let_("_sh", b.app(b.global("Queue", "shutdown"), b.loc("out_q")))],
        b.unit(),
    );
    let reg_out = b.app2(
        rt(b, "Layer.OwnerRun"),
        b.loc("own"),
        b.task(b.app(b.global("Scope", "add_finalizer"), b.task(shutdown_out))),
    );
    let end_each = b.each(
        "entry",
        b.rget(b.loc("grps")),
        b.match_(
            b.loc("entry"),
            [arm(
                pair_pat(b, "_", "eq"),
                b.block(
                    [b.let_("_e", b.app(b.global("Queue", "end"), b.loc("eq")))],
                    b.unit(),
                ),
            )],
        ),
    );
    let reg_grps = b.app2(
        rt(b, "Layer.OwnerRun"),
        b.loc("forked"),
        b.task(b.app(b.global("Scope", "add_finalizer"), b.task(end_each))),
    );
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.app(b.global("Queue", "take_all"), b.loc("out_q")),
            [
                arm(b.some_pat("xs"), b.ok(b.loc("xs"))),
                arm(Pat::Wild, b.err(b.unit())),
            ],
        ),
    );
    let stmts = [
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("out_q", b.app(b.global("Queue", "unbounded"), b.unit())),
        b.let_("grps", b.rmake(b.list(vec![]))),
        b.let_("get_q", group_by_get_or_add(b)),
        Stmt::Expr(reg_out),
        Stmt::Expr(reg_grps),
        b.let_("_pf", group_by_producer(b)),
    ];
    let build = b.lam(&["up", "own", "forked"], b.block(stmts, pull));
    b.lam(
        &["by_key", "f", "self"],
        b.app(rt(b, "Helper.ch_bracket"), build),
    )
}
