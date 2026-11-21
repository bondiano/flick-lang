//! Scalar shrink-tree and biased integer helpers for `Arbitrary`.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat, Stmt};
use super::{add_f, mul_f, st_pat, sub_f};

pub(super) fn number_sample_helper(b: &ExprBuilder) -> Expr {
    let next_num = b.some(b.app5(
        rt(b, "Helper.arb_number_sample"),
        b.loc("cv"),
        b.loc("min_o"),
        b.loc("max_o"),
        b.loc("is_int"),
        b.loc("nc"),
    ));
    let on_cand = b.lam(
        &["c"],
        b.match_(b.loc("c"), [arm(pair_pat(b, "cv", "nc"), next_num)]),
    );
    let pull = b.app2(rt(b, "Helper.arb_pull_each"), b.loc("cands"), on_cand);
    let cands = b.app5(
        rt(b, "Arbitrary.ShrinkNumber"),
        b.loc("val"),
        b.loc("min_o"),
        b.loc("max_o"),
        b.loc("is_int"),
        b.loc("ctx_o"),
    );
    let empty = b.bin(
        BinOp::Eq,
        b.app(b.global("List.Core", "length"), b.loc("cands")),
        b.int(0),
    );
    let body = b.if_(
        empty,
        b.tuple([b.loc("val"), b.none()]),
        b.tuple([b.loc("val"), b.some(pull)]),
    );
    b.lam(
        &["val", "min_o", "max_o", "is_int", "ctx_o"],
        b.block([b.let_("cands", cands)], body),
    )
}

pub(super) fn bigint_sample_helper(b: &ExprBuilder) -> Expr {
    let on_cand = b.lam(
        &["c"],
        b.block(
            [
                b.let_("prev", b.rget(b.loc("pr"))),
                b.rset(b.loc("pr"), b.some(b.loc("c"))),
            ],
            b.some(b.app3(
                rt(b, "Helper.arb_bigint_sample"),
                b.loc("c"),
                b.loc("tgt"),
                b.loc("prev"),
            )),
        ),
    );
    let pull = b.some(b.app2(rt(b, "Helper.arb_pull_each"), b.loc("cands"), on_cand));
    let non_empty = b.block([b.let_("pr", b.rmake(b.loc("ctx_o")))], pull);
    let cands = b.app3(
        rt(b, "Arbitrary.ShrinkBigInt"),
        b.loc("val"),
        b.loc("tgt"),
        b.loc("ctx_o"),
    );
    let empty = b.bin(
        BinOp::Eq,
        b.app(b.global("List.Core", "length"), b.loc("cands")),
        b.int(0),
    );
    let mk_pull = b.lam(
        &["_u"],
        b.block([b.let_("cands", cands)], b.if_(empty, b.none(), non_empty)),
    );
    b.lam(
        &["val", "tgt", "ctx_o"],
        b.app2(rt(b, "Helper.arb_lazy_shrinks"), b.loc("val"), mk_pull),
    )
}

fn decimal_next_cand(b: &ExprBuilder) -> Expr {
    let from_list = b.block(
        [b.rset(b.loc("ir"), b.add_i(b.loc("i"), b.int(1)))],
        b.some(b.app2(rt(b, "Helper.list_at"), b.loc("i"), b.loc("cands"))),
    );
    let mid = b.app3(
        rt(b, "Arbitrary.DecimalMidpoint"),
        b.loc("pd"),
        b.loc("val"),
        b.loc("max_sc"),
    );
    let from_prev = b.match_(
        b.loc("prev"),
        [
            arm(
                b.some_pat("pc"),
                b.match_(b.loc("pc"), [arm(pair_pat(b, "pd", "_"), mid)]),
            ),
            arm(Pat::Wild, b.none()),
        ],
    );
    b.if_(
        b.bin(BinOp::Lt, b.loc("i"), b.loc("clen")),
        from_list,
        from_prev,
    )
}

pub(super) fn decimal_sample_helper(b: &ExprBuilder) -> Expr {
    let rf = b.match_(
        b.loc("prev"),
        [
            arm(
                b.some_pat("pc"),
                b.match_(b.loc("pc"), [arm(pair_pat(b, "_", "r"), b.loc("r"))]),
            ),
            arm(Pat::Wild, b.float(0.0)),
        ],
    );
    let next_sample = b.app4(
        rt(b, "Helper.arb_decimal_sample"),
        b.loc("c"),
        b.loc("tgt"),
        b.loc("prev"),
        b.loc("max_sc"),
    );
    let on_cand = b.block(
        [
            b.let_("rf", rf),
            b.rset(
                b.loc("pr"),
                b.some(b.tuple([b.loc("c"), add_f(b, b.loc("rf"), b.float(1.0))])),
            ),
        ],
        b.some(b.some(next_sample)),
    );
    let step = b.lam(
        &["_u2"],
        b.block(
            [
                b.let_("i", b.rget(b.loc("ir"))),
                b.let_("prev", b.rget(b.loc("pr"))),
                b.let_("cand_o", decimal_next_cand(b)),
            ],
            b.match_(
                b.loc("cand_o"),
                [arm(b.some_pat("c"), on_cand), arm(Pat::Wild, b.none())],
            ),
        ),
    );
    let cands = b.app4(
        rt(b, "Arbitrary.ShrinkDecimalCands"),
        b.loc("val"),
        b.loc("tgt"),
        b.loc("ctx_o"),
        b.loc("max_sc"),
    );
    let mk_pull = b.lam(
        &["_u"],
        b.block(
            [
                b.let_("cands", cands),
                b.let_(
                    "clen",
                    b.app(b.global("List.Core", "length"), b.loc("cands")),
                ),
                b.let_("ir", b.rmake(b.int(0))),
                b.let_("pr", b.rmake(b.loc("ctx_o"))),
            ],
            b.some(step),
        ),
    );
    b.lam(
        &["val", "tgt", "ctx_o", "max_sc"],
        b.app2(rt(b, "Helper.arb_lazy_shrinks"), b.loc("val"), mk_pull),
    )
}

pub(super) fn string_sample_helper(b: &ExprBuilder) -> Expr {
    let on_cand = b.lam(
        &["c"],
        b.some(b.app2(
            rt(b, "Helper.arb_string_sample"),
            b.loc("c"),
            b.loc("min_len"),
        )),
    );
    let pull = b.app2(rt(b, "Helper.arb_pull_each"), b.loc("cands"), on_cand);
    let cands = b.app2(
        rt(b, "Arbitrary.ShrinkString"),
        b.loc("val"),
        b.loc("min_len"),
    );
    let empty = b.bin(
        BinOp::Eq,
        b.app(b.global("List.Core", "length"), b.loc("cands")),
        b.int(0),
    );
    let body = b.if_(
        empty,
        b.tuple([b.loc("val"), b.none()]),
        b.tuple([b.loc("val"), b.some(pull)]),
    );
    b.lam(&["val", "min_len"], b.block([b.let_("cands", cands)], body))
}

fn fl_log2(b: &ExprBuilder, x: Expr) -> Expr {
    let coarse = b.while_(
        b.bin(
            BinOp::Le,
            mul_f(b, b.var("p"), b.float(65536.0)),
            b.loc("xv"),
        ),
        b.block(
            [
                b.assign("p", mul_f(b, b.var("p"), b.float(65536.0))),
                b.assign("k", add_f(b, b.var("k"), b.float(16.0))),
            ],
            b.unit(),
        ),
    );
    let fine = b.while_(
        b.bin(BinOp::Le, mul_f(b, b.var("p"), b.float(2.0)), b.loc("xv")),
        b.block(
            [
                b.assign("p", mul_f(b, b.var("p"), b.float(2.0))),
                b.assign("k", add_f(b, b.var("k"), b.float(1.0))),
            ],
            b.unit(),
        ),
    );
    b.block(
        [
            b.let_("xv", x),
            b.var_decl("p", b.float(1.0)),
            b.var_decl("k", b.float(0.0)),
            Stmt::Expr(coarse),
            Stmt::Expr(fine),
        ],
        b.var("k"),
    )
}

fn biased_ranges_expr(b: &ExprBuilder) -> Expr {
    let crosses_zero = b.bin(
        BinOp::And,
        b.bin(BinOp::Lt, b.loc("lo"), b.float(0.0)),
        b.bin(BinOp::Gt, b.loc("hi"), b.float(0.0)),
    );
    let zero_ranges = b.block(
        [
            b.let_("l", fl_log2(b, sub_f(b, b.float(0.0), b.loc("lo")))),
            b.let_("h", fl_log2(b, b.loc("hi"))),
        ],
        b.list([
            b.tuple([sub_f(b, b.float(0.0), b.loc("l")), b.loc("h")]),
            b.tuple([sub_f(b, b.loc("hi"), b.loc("h")), b.loc("hi")]),
            b.tuple([b.loc("lo"), add_f(b, b.loc("lo"), b.loc("l"))]),
        ]),
    );
    let same_sign_ranges = b.block(
        [
            b.let_("gap", fl_log2(b, sub_f(b, b.loc("hi"), b.loc("lo")))),
            b.let_(
                "r0",
                b.tuple([b.loc("lo"), add_f(b, b.loc("lo"), b.loc("gap"))]),
            ),
            b.let_(
                "r1",
                b.tuple([sub_f(b, b.loc("hi"), b.loc("gap")), b.loc("hi")]),
            ),
        ],
        b.if_(
            b.bin(BinOp::Lt, b.loc("lo"), b.float(0.0)),
            b.list([b.loc("r1"), b.loc("r0")]),
            b.list([b.loc("r0"), b.loc("r1")]),
        ),
    );
    b.if_(
        b.bin(BinOp::Eq, b.loc("lo"), b.loc("hi")),
        b.list([b.tuple([b.loc("lo"), b.loc("hi")])]),
        b.if_(crosses_zero, zero_ranges, same_sign_ranges),
    )
}

pub(super) fn numeric_int_helper(b: &ExprBuilder) -> Expr {
    let ri_expr = b.app3(
        rt(b, "Helper.arb_st_int"),
        b.loc("st"),
        mul_f(b, b.float(-2.0), sub_f(b, b.loc("nf"), b.float(1.0))),
        sub_f(b, b.loc("nf"), b.float(2.0)),
    );
    let pick_multi = b.block(
        [
            b.let_("nf", b.app(b.global("Int", "to_float"), b.loc("rn"))),
            b.let_("ri", ri_expr),
        ],
        b.if_(
            b.bin(BinOp::Lt, b.loc("ri"), b.float(0.0)),
            b.app2(rt(b, "Helper.list_at"), b.int(0), b.loc("ranges")),
            b.app2(
                rt(b, "Helper.list_at"),
                b.add_i(b.app(b.global("Int", "truncate"), b.loc("ri")), b.int(1)),
                b.loc("ranges"),
            ),
        ),
    );
    let picked = b.if_(
        b.bin(BinOp::Eq, b.loc("rn"), b.int(1)),
        b.app2(rt(b, "Helper.list_at"), b.int(0), b.loc("ranges")),
        pick_multi,
    );
    let biased = b.block(
        [
            b.let_("ranges", biased_ranges_expr(b)),
            b.let_(
                "rn",
                b.app(b.global("List.Core", "length"), b.loc("ranges")),
            ),
            b.let_("picked", picked),
        ],
        b.match_(
            b.loc("picked"),
            [arm(
                pair_pat(b, "plo", "phi"),
                b.app3(
                    rt(b, "Helper.arb_st_int"),
                    b.loc("st"),
                    b.loc("plo"),
                    b.loc("phi"),
                ),
            )],
        ),
    );
    let do_bias = b.bin(
        BinOp::Eq,
        b.app3(
            rt(b, "Helper.arb_st_int"),
            b.loc("st"),
            b.float(1.0),
            b.loc("bias"),
        ),
        b.float(1.0),
    );
    let unbiased = b.app3(
        rt(b, "Helper.arb_st_int"),
        b.loc("st"),
        b.loc("lo"),
        b.loc("hi"),
    );
    b.lam(
        &["st", "lo", "hi"],
        b.match_(
            b.loc("st"),
            [arm(st_pat(b), b.if_(do_bias, biased, unbiased))],
        ),
    )
}
