//! Filter, `mapAccum`, and scan `Channel.Transform` IR helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::pair_pat;
use super::super::{Expr, Pat};

pub(super) fn ch_filter_helper(b: &ExprBuilder) -> Expr {
    let on_ok = b.if_(
        b.app(b.loc("pred"), b.loc("v")),
        b.break_("flt", b.ok(b.loc("v"))),
        b.unit(),
    );
    let step = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("v"), on_ok),
            arm(b.err_pat("d"), b.break_("flt", b.err(b.loc("d")))),
        ],
    );
    b.lam(
        &["pred", "self", "up", "own"],
        b.block(
            [b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own")))],
            b.lam(&["_u"], b.loop_ctrl("flt", step)),
        ),
    )
}

pub(super) fn ch_filter_map_helper(b: &ExprBuilder) -> Expr {
    let on_ok = b.match_(
        b.app(b.loc("f"), b.loc("v")),
        [
            arm(b.ok_pat("y"), b.break_("flm", b.ok(b.loc("y")))),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let step = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("v"), on_ok),
            arm(b.err_pat("d"), b.break_("flm", b.err(b.loc("d")))),
        ],
    );
    b.lam(
        &["f", "self", "up", "own"],
        b.block(
            [b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own")))],
            b.lam(&["_u"], b.loop_ctrl("flm", step)),
        ),
    )
}

pub(super) fn ch_filter_array_helper(b: &ExprBuilder) -> Expr {
    let keep = b.gen_strict(b.each(
        "x",
        b.loc("xs"),
        b.if_(
            b.app(b.loc("pred"), b.loc("x")),
            b.yield_(b.loc("x")),
            b.unit(),
        ),
    ));
    let non_empty = b.bin(
        BinOp::Gt,
        b.app(b.global("List.Core", "length"), b.loc("out")),
        b.int(0),
    );
    let on_ok = b.block(
        [b.let_("out", keep)],
        b.if_(non_empty, b.break_("flta", b.ok(b.loc("out"))), b.unit()),
    );
    let step = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("xs"), on_ok),
            arm(b.err_pat("d"), b.break_("flta", b.err(b.loc("d")))),
        ],
    );
    b.lam(
        &["pred", "self", "up", "own"],
        b.block(
            [b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own")))],
            b.lam(&["_u"], b.loop_ctrl("flta", step)),
        ),
    )
}

pub(super) fn ch_filter_map_array_helper(b: &ExprBuilder) -> Expr {
    let keep = b.gen_strict(b.each(
        "x",
        b.loc("xs"),
        b.match_(
            b.app(b.loc("f"), b.loc("x")),
            [
                arm(b.ok_pat("y"), b.yield_(b.loc("y"))),
                arm(Pat::Wild, b.unit()),
            ],
        ),
    ));
    let non_empty = b.bin(
        BinOp::Gt,
        b.app(b.global("List.Core", "length"), b.loc("out")),
        b.int(0),
    );
    let on_ok = b.block(
        [b.let_("out", keep)],
        b.if_(non_empty, b.break_("flma", b.ok(b.loc("out"))), b.unit()),
    );
    let step = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("xs"), on_ok),
            arm(b.err_pat("d"), b.break_("flma", b.err(b.loc("d")))),
        ],
    );
    b.lam(
        &["f", "self", "up", "own"],
        b.block(
            [b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own")))],
            b.lam(&["_u"], b.loop_ctrl("flma", step)),
        ),
    )
}

pub(super) fn ch_map_accum_helper(b: &ExprBuilder) -> Expr {
    let cons_pat = Pat::List(
        vec![Pat::Bind(b.name("h"))],
        Some(Box::new(Pat::Bind(b.name("tl")))),
    );
    let emit_head = b.block(
        [b.rset(b.loc("buf"), b.loc("tl"))],
        b.break_("ma", b.ok(b.loc("h"))),
    );
    let update_st = b.block(
        [
            b.rset(b.loc("st"), b.loc("ns")),
            b.rset(b.loc("buf"), b.loc("out")),
        ],
        b.unit(),
    );
    let on_ok = b.match_(
        b.app2(b.loc("f"), b.rget(b.loc("st")), b.loc("o")),
        [arm(pair_pat(b, "ns", "out"), update_st)],
    );
    let refill = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("o"), on_ok),
            arm(b.err_pat("d"), b.break_("ma", b.err(b.loc("d")))),
        ],
    );
    let step = b.match_(
        b.rget(b.loc("buf")),
        [arm(cons_pat, emit_head), arm(Pat::Wild, refill)],
    );
    b.lam(
        &["init", "f", "self", "up", "own"],
        b.block(
            [
                b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
                b.let_("st", b.rmake(b.app(b.loc("init"), b.unit()))),
                b.let_("buf", b.rmake(b.list(vec![]))),
            ],
            b.lam(&["_u"], b.loop_ctrl("ma", step)),
        ),
    )
}

pub(super) fn ch_scan_helper(b: &ExprBuilder) -> Expr {
    let emit_init = b.block(
        [b.rset(b.loc("first"), b.bool_(false))],
        b.ok(b.rget(b.loc("st"))),
    );
    let step_ok = b.block(
        [
            b.let_("ns", b.app2(b.loc("f"), b.rget(b.loc("st")), b.loc("o"))),
            b.rset(b.loc("st"), b.loc("ns")),
        ],
        b.ok(b.loc("ns")),
    );
    let next_pull = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("o"), step_ok),
            arm(b.err_pat("d"), b.err(b.loc("d"))),
        ],
    );
    let pull = b.lam(&["_u"], b.if_(b.rget(b.loc("first")), emit_init, next_pull));
    b.lam(
        &["init", "f", "self", "up", "own"],
        b.block(
            [
                b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
                b.let_("st", b.rmake(b.loc("init"))),
                b.let_("first", b.rmake(b.bool_(true))),
            ],
            pull,
        ),
    )
}
