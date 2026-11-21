//! `Stream.Transform` stateful accumulation (`mapAccum`, `scan`, `accumulate`) helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::pair_pat;
use super::super::{Expr, Stmt};

pub(super) fn st_map_accum_helper(b: &ExprBuilder) -> Expr {
    let step_el = b.match_(
        b.app2(b.loc("f"), b.var("s"), b.loc("a")),
        [arm(
            pair_pat(b, "ns", "vs"),
            b.block(
                [
                    b.assign("s", b.loc("ns")),
                    b.assign(
                        "out",
                        b.app2(b.global("List.Core", "append"), b.var("out"), b.loc("vs")),
                    ),
                ],
                b.unit(),
            ),
        )],
    );
    let has_out = b.bin(
        BinOp::Gt,
        b.app(b.global("List.Core", "length"), b.var("out")),
        b.int(0),
    );
    let on_chunk = b.block(
        [
            b.var_decl("s", b.rget(b.loc("st"))),
            b.var_decl("out", b.list(vec![])),
            Stmt::Expr(b.each("a", b.loc("xs"), step_el)),
            b.rset(b.loc("st"), b.var("s")),
        ],
        b.if_(has_out, b.break_("sma", b.ok(b.var("out"))), b.unit()),
    );
    let loop_body = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("xs"), on_chunk),
            arm(b.err_pat("d"), b.break_("sma", b.err(b.loc("d")))),
        ],
    );
    let init = [
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("st", b.rmake(b.loc("init"))),
    ];
    let pull = b.lam(&["_u"], b.loop_ctrl("sma", loop_body));
    b.lam(&["init", "f", "self", "up", "own"], b.block(init, pull))
}

pub(super) fn st_map_accum_array_helper(b: &ExprBuilder) -> Expr {
    let has_vs = b.bin(
        BinOp::Gt,
        b.app(b.global("List.Core", "length"), b.loc("vs")),
        b.int(0),
    );
    let on_pair = b.block(
        [b.rset(b.loc("st"), b.loc("ns"))],
        b.if_(has_vs, b.break_("smaa", b.ok(b.loc("vs"))), b.unit()),
    );
    let on_chunk = b.match_(
        b.app2(b.loc("f"), b.rget(b.loc("st")), b.loc("xs")),
        [arm(pair_pat(b, "ns", "vs"), on_pair)],
    );
    let loop_body = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("xs"), on_chunk),
            arm(b.err_pat("d"), b.break_("smaa", b.err(b.loc("d")))),
        ],
    );
    let init = [
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("st", b.rmake(b.loc("init"))),
    ];
    let pull = b.lam(&["_u"], b.loop_ctrl("smaa", loop_body));
    b.lam(&["init", "f", "self", "up", "own"], b.block(init, pull))
}

pub(super) fn st_scan_helper(b: &ExprBuilder) -> Expr {
    let step_el = b.block(
        [b.assign("s", b.app2(b.loc("f"), b.var("s"), b.loc("a")))],
        b.yield_(b.var("s")),
    );
    let on_chunk = b.block(
        [
            b.var_decl("s", b.rget(b.loc("st"))),
            b.let_("out", b.gen_strict(b.each("a", b.loc("xs"), step_el))),
            b.rset(b.loc("st"), b.var("s")),
        ],
        b.ok(b.loc("out")),
    );
    let pull_next = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("xs"), on_chunk),
            arm(b.err_pat("d"), b.err(b.loc("d"))),
        ],
    );
    let emit_init = b.block(
        [b.rset(b.loc("first"), b.bool_(false))],
        b.ok(b.list([b.loc("init")])),
    );
    let pull = b.lam(&["_u"], b.if_(b.rget(b.loc("first")), emit_init, pull_next));
    let init = [
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("st", b.rmake(b.loc("init"))),
        b.let_("first", b.rmake(b.bool_(true))),
    ];
    b.lam(&["init", "f", "self", "up", "own"], b.block(init, pull))
}

pub(super) fn st_accumulate_helper(b: &ExprBuilder) -> Expr {
    let on_chunk = b.block(
        [
            b.let_(
                "comb",
                b.app2(
                    b.global("List.Core", "append"),
                    b.rget(b.loc("acc")),
                    b.loc("xs"),
                ),
            ),
            b.rset(b.loc("acc"), b.loc("comb")),
        ],
        b.ok(b.list([b.loc("comb")])),
    );
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.app(b.loc("pull"), b.unit()),
            [
                arm(b.ok_pat("xs"), on_chunk),
                arm(b.err_pat("d"), b.err(b.loc("d"))),
            ],
        ),
    );
    let init = [
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("acc", b.rmake(b.list(vec![]))),
    ];
    b.lam(&["self", "up", "own"], b.block(init, pull))
}
