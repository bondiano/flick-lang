//! `Stream.Combine` neighbor-zipping (`zipWithNext`, `zipWithPrevious`, `zipWithPreviousAndNext`) helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::pair_pat;
use super::super::{Expr, Pat, Stmt};

pub(super) fn st_zip_with_next_helper(b: &ExprBuilder) -> Expr {
    let emit_pair = b.block(
        [
            b.assign(
                "out",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("out"),
                    b.list([b.tuple([b.loc("pv"), b.some(b.loc("a"))])]),
                ),
            ),
            b.assign("p", b.some(b.loc("a"))),
        ],
        b.unit(),
    );
    let step_el = b.match_(
        b.var("p"),
        [
            arm(b.some_pat("pv"), emit_pair),
            arm(Pat::Wild, b.assign_e("p", b.some(b.loc("a")))),
        ],
    );
    let has_out = b.bin(
        BinOp::Gt,
        b.app(b.global("List.Core", "length"), b.var("out")),
        b.int(0),
    );
    let on_chunk = b.block(
        [
            b.var_decl("p", b.rget(b.loc("prev"))),
            b.var_decl("out", b.list(vec![])),
            Stmt::Expr(b.each("a", b.loc("xs"), step_el)),
            b.rset(b.loc("prev"), b.var("p")),
        ],
        b.if_(has_out, b.break_("szn", b.ok(b.var("out"))), b.unit()),
    );
    let flush_last = b.match_(
        b.loc("lp"),
        [
            arm(
                b.some_pat("pv"),
                b.break_("szn", b.ok(b.list([b.tuple([b.loc("pv"), b.none()])]))),
            ),
            arm(Pat::Wild, b.break_("szn", b.err(b.loc("d")))),
        ],
    );
    let on_end = b.block(
        [
            b.rset(b.loc("done"), b.bool_(true)),
            b.let_("lp", b.rget(b.loc("prev"))),
            b.rset(b.loc("prev"), b.none()),
        ],
        flush_last,
    );
    let loop_body = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [arm(b.ok_pat("xs"), on_chunk), arm(b.err_pat("d"), on_end)],
    );
    let pull = b.lam(
        &["_u"],
        b.if_(
            b.rget(b.loc("done")),
            b.err(b.unit()),
            b.loop_ctrl("szn", loop_body),
        ),
    );
    let init = [
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("prev", b.rmake(b.none())),
        b.let_("done", b.rmake(b.bool_(false))),
    ];
    b.lam(&["self", "up", "own"], b.block(init, pull))
}

pub(super) fn st_zip_with_previous_helper(b: &ExprBuilder) -> Expr {
    let step = b.block(
        [b.let_("cp", b.var("p")), b.assign("p", b.some(b.loc("a")))],
        b.yield_(b.tuple([b.loc("cp"), b.loc("a")])),
    );
    let on_chunk = b.block(
        [
            b.var_decl("p", b.rget(b.loc("prev"))),
            b.let_("out", b.gen_strict(b.each("a", b.loc("xs"), step))),
            b.rset(b.loc("prev"), b.var("p")),
        ],
        b.ok(b.loc("out")),
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
        b.let_("prev", b.rmake(b.none())),
    ];
    b.lam(&["self", "up", "own"], b.block(init, pull))
}

pub(super) fn st_zip_with_prev_next_helper(b: &ExprBuilder) -> Expr {
    let emit_triple = b.block(
        [
            b.assign(
                "out",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("out"),
                    b.list([b.tuple([b.loc("pr"), b.loc("cv"), b.some(b.loc("a"))])]),
                ),
            ),
            b.assign("pair", b.tuple([b.some(b.loc("cv")), b.some(b.loc("a"))])),
        ],
        b.unit(),
    );
    let step_cur = b.match_(
        b.loc("cu"),
        [
            arm(b.some_pat("cv"), emit_triple),
            arm(
                Pat::Wild,
                b.assign_e("pair", b.tuple([b.loc("pr"), b.some(b.loc("a"))])),
            ),
        ],
    );
    let step_el = b.match_(b.var("pair"), [arm(pair_pat(b, "pr", "cu"), step_cur)]);
    let has_out = b.bin(
        BinOp::Gt,
        b.app(b.global("List.Core", "length"), b.var("out")),
        b.int(0),
    );
    let on_chunk = b.block(
        [
            b.var_decl("out", b.list(vec![])),
            b.var_decl("pair", b.rget(b.loc("st"))),
            Stmt::Expr(b.each("a", b.loc("xs"), step_el)),
            b.rset(b.loc("st"), b.var("pair")),
        ],
        b.if_(has_out, b.break_("szpn", b.ok(b.var("out"))), b.unit()),
    );
    let flush_cur = b.match_(
        b.loc("cu"),
        [
            arm(
                b.some_pat("cv"),
                b.break_(
                    "szpn",
                    b.ok(b.list([b.tuple([b.loc("pr"), b.loc("cv"), b.none()])])),
                ),
            ),
            arm(Pat::Wild, b.break_("szpn", b.err(b.loc("d")))),
        ],
    );
    let on_end = b.block(
        [
            b.rset(b.loc("done"), b.bool_(true)),
            b.let_("lp", b.rget(b.loc("st"))),
            b.rset(b.loc("st"), b.tuple([b.none(), b.none()])),
        ],
        b.match_(b.loc("lp"), [arm(pair_pat(b, "pr", "cu"), flush_cur)]),
    );
    let loop_body = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [arm(b.ok_pat("xs"), on_chunk), arm(b.err_pat("d"), on_end)],
    );
    let pull = b.lam(
        &["_u"],
        b.if_(
            b.rget(b.loc("done")),
            b.err(b.unit()),
            b.loop_ctrl("szpn", loop_body),
        ),
    );
    let init = [
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("st", b.rmake(b.tuple([b.none(), b.none()]))),
        b.let_("done", b.rmake(b.bool_(false))),
    ];
    b.lam(&["self", "up", "own"], b.block(init, pull))
}
