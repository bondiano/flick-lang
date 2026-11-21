//! `Stream.Core` unfold, pagination, iteration, range, and byte-limit helpers.

use flick_ir::{Arith, BinOp, Num};

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_channel_core::DEFAULT_CHUNK_SIZE;
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat, Stmt};

pub(super) fn st_unfold_helper(b: &ExprBuilder) -> Expr {
    let emit_pair = b.match_(
        b.loc("pair"),
        [arm(
            pair_pat(b, "a", "ns"),
            b.block(
                [b.rset(b.loc("st"), b.some(b.loc("ns")))],
                b.ok(b.list([b.loc("a")])),
            ),
        )],
    );
    let clear = b.block([b.rset(b.loc("st"), b.none())], b.err(b.unit()));
    let step = b.match_(
        b.app(b.loc("f"), b.loc("s")),
        [arm(b.some_pat("pair"), emit_pair), arm(Pat::Wild, clear)],
    );
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.rget(b.loc("st")),
            [arm(b.some_pat("s"), step), arm(Pat::Wild, b.err(b.unit()))],
        ),
    );
    let init = [b.let_("st", b.rmake(b.some(b.loc("seed"))))];
    b.lam(&["seed", "f", "_up", "_own"], b.block(init, pull))
}

pub(super) fn st_paginate_helper(b: &ExprBuilder) -> Expr {
    let non_empty = b.bin(
        BinOp::Gt,
        b.app(b.global("List.Core", "length"), b.loc("xs")),
        b.int(0),
    );
    let page_body = b.block(
        [b.rset(b.loc("st"), b.loc("next_o"))],
        b.if_(non_empty, b.break_("spg", b.ok(b.loc("xs"))), b.unit()),
    );
    let step = b.match_(
        b.app(b.loc("f"), b.loc("s")),
        [arm(pair_pat(b, "xs", "next_o"), page_body)],
    );
    let loop_body = b.match_(
        b.rget(b.loc("st")),
        [
            arm(b.some_pat("s"), step),
            arm(Pat::Wild, b.break_("spg", b.err(b.unit()))),
        ],
    );
    let pull = b.lam(&["_u"], b.loop_ctrl("spg", loop_body));
    let init = [b.let_("st", b.rmake(b.some(b.loc("seed"))))];
    b.lam(&["seed", "f", "_up", "_own"], b.block(init, pull))
}

pub(super) fn st_iterate_helper(b: &ExprBuilder) -> Expr {
    let step = b.block(
        [
            b.let_("v", b.rget(b.loc("cur"))),
            b.rset(b.loc("cur"), b.app(b.loc("f"), b.loc("v"))),
        ],
        b.ok(b.list([b.loc("v")])),
    );
    let init = [b.let_("cur", b.rmake(b.loc("init")))];
    b.lam(
        &["init", "f", "_up", "_own"],
        b.block(init, b.lam(&["_u"], step)),
    )
}

pub(super) fn st_range_helper(b: &ExprBuilder) -> Expr {
    let step_body = b.block(
        [
            b.let_("v", b.var("k")),
            b.assign("k", b.add_i(b.var("k"), b.int(1))),
        ],
        b.yield_(b.loc("v")),
    );
    let chunk_gen = b.gen_strict(b.while_(b.bin(BinOp::Lt, b.var("k"), b.loc("end")), step_body));
    let chunk_size = b.if_(
        b.bin(BinOp::Gt, b.loc("rem"), b.int(DEFAULT_CHUNK_SIZE)),
        b.int(DEFAULT_CHUNK_SIZE),
        b.loc("rem"),
    );
    let emit_chunk = b.block(
        [
            b.let_("rem", b.add_i(b.sub_i(b.loc("hi"), b.loc("s")), b.int(1))),
            b.let_("n", chunk_size),
            b.let_("end", b.add_i(b.loc("s"), b.loc("n"))),
            b.rset(b.loc("cur"), b.loc("end")),
            b.var_decl("k", b.loc("s")),
            b.let_("chunk", chunk_gen),
        ],
        b.ok(b.loc("chunk")),
    );
    let pull_body = b.block(
        [b.let_("s", b.rget(b.loc("cur")))],
        b.if_(
            b.bin(BinOp::Gt, b.loc("s"), b.loc("hi")),
            b.err(b.unit()),
            emit_chunk,
        ),
    );
    let init = [b.let_("cur", b.rmake(b.loc("lo")))];
    b.lam(
        &["lo", "hi", "_up", "_own"],
        b.block(init, b.lam(&["_u"], pull_body)),
    )
}

fn limit_bytes_chunk_body(b: &ExprBuilder) -> Expr {
    let bl = b.app(
        rt(b, "Int.ToBigInt"),
        b.app(b.global("List.Core", "length"), b.loc("bs")),
    );
    let keep = b.block(
        [
            b.assign(
                "r",
                b.bin(
                    BinOp::Arith(Arith::Sub, Num::BigInt),
                    b.var("r"),
                    b.loc("bl"),
                ),
            ),
            b.assign(
                "out",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("out"),
                    b.list([b.loc("bs")]),
                ),
            ),
        ],
        b.unit(),
    );
    let stop = b.block(
        [b.assign("hit", b.bool_(true)), b.assign("r", b.bigint("0"))],
        b.break_("slbc", b.unit()),
    );
    let check = b.if_(b.bin(BinOp::Le, b.loc("bl"), b.var("r")), keep, stop);
    let scan = b.for_ctrl(
        "slbc",
        "bs",
        b.loc("chunks"),
        b.block([b.let_("bl", bl)], check),
    );
    let has_out = b.bin(
        BinOp::Gt,
        b.app(b.global("List.Core", "length"), b.var("out")),
        b.int(0),
    );
    let on_hit = b.block(
        [
            b.let_("fp", b.app2(b.loc("on_lim"), b.loc("up"), b.loc("own"))),
            b.rset(b.loc("fb"), b.some(b.loc("fp"))),
        ],
        b.if_(
            has_out.clone(),
            b.break_("slb", b.ok(b.var("out"))),
            b.break_("slb", b.app(b.loc("fp"), b.unit())),
        ),
    );
    let on_ok = b.if_(has_out, b.break_("slb", b.ok(b.var("out"))), b.unit());
    let stmts = [
        b.var_decl("out", b.list(vec![])),
        b.var_decl("hit", b.bool_(false)),
        b.var_decl("r", b.rget(b.loc("rem"))),
        Stmt::Expr(scan),
        b.rset(b.loc("rem"), b.var("r")),
    ];
    b.block(stmts, b.if_(b.var("hit"), on_hit, on_ok))
}

pub(super) fn st_limit_bytes_helper(b: &ExprBuilder) -> Expr {
    // (limit: ByteSize (BigInt), on_limit: Stream, self: Stream (List Int)) -> Stream (List Int)
    let pull_loop = b.loop_ctrl(
        "slb",
        b.match_(
            b.app(b.loc("pull"), b.unit()),
            [
                arm(b.ok_pat("chunks"), limit_bytes_chunk_body(b)),
                arm(b.err_pat("d"), b.break_("slb", b.err(b.loc("d")))),
            ],
        ),
    );
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.rget(b.loc("fb")),
            [
                arm(b.some_pat("fp"), b.app(b.loc("fp"), b.unit())),
                arm(Pat::Wild, pull_loop),
            ],
        ),
    );
    let init = [
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("rem", b.rmake(b.loc("lim"))),
        b.let_("fb", b.rmake(b.none())),
    ];
    b.lam(&["lim", "on_lim", "self", "up", "own"], b.block(init, pull))
}
