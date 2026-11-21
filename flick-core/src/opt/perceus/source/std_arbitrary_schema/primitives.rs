//! Primitive constraint-to-node builders (`number`, `string`, `bigint`).

use flick_ir::{Arith, BinOp, Num};

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt, str_lit};
use super::super::{Expr, Pat};
use super::{arb_err, c_pat, float_inf, node_con};

pub(super) fn build_number_helper(b: &ExprBuilder) -> Expr {
    let bounds = b.app5(
        rt(b, "Arbitrary.NumberBounds"),
        b.loc("lo"),
        b.loc("hi"),
        b.loc("exl"),
        b.loc("exh"),
        b.loc("cnum"),
    );
    let on_bp = b.match_(
        b.loc("bp"),
        [arm(
            pair_pat(b, "bl", "bh"),
            node_con(b, "$Number", vec![b.loc("bl"), b.loc("bh"), b.loc("cnum")]),
        )],
    );
    let stmts = [
        b.let_(
            "is_num",
            b.bin(BinOp::Eq, b.loc("cord"), str_lit(b, "number")),
        ),
        b.let_("lo", b.if_(b.loc("is_num"), b.loc("cmin"), b.none())),
        b.let_("hi", b.if_(b.loc("is_num"), b.loc("cmax"), b.none())),
        b.let_("exl", b.bin(BinOp::And, b.loc("is_num"), b.loc("cexmin"))),
        b.let_("exh", b.bin(BinOp::And, b.loc("is_num"), b.loc("cexmax"))),
    ];
    let body = b.block(
        stmts,
        b.match_(
            bounds,
            [
                arm(b.ok_pat("bp"), on_bp),
                arm(b.err_pat("what"), arb_err(b, b.loc("what"))),
            ],
        ),
    );
    let on_c = b.match_(b.loc("c"), [arm(c_pat(b), body)]);
    let default_node = node_con(b, "$Number", vec![b.none(), b.none(), str_lit(b, "")]);
    b.lam(
        &["c_opt"],
        b.match_(
            b.loc("c_opt"),
            [arm(b.some_pat("c"), on_c), arm(Pat::Wild, default_node)],
        ),
    )
}

fn opt_or(b: &ExprBuilder, opt: Expr, fallback: Expr) -> Expr {
    b.match_(
        opt,
        [arm(b.some_pat("x"), b.loc("x")), arm(Pat::Wild, fallback)],
    )
}

fn is_some_expr(b: &ExprBuilder, opt: Expr) -> Expr {
    b.match_(
        opt,
        [
            arm(b.some_pat("_"), b.bool_(true)),
            arm(Pat::Wild, b.bool_(false)),
        ],
    )
}

pub(super) fn build_string_helper(b: &ExprBuilder) -> Expr {
    let has_cp = b.bin(
        BinOp::Or,
        is_some_expr(b, b.loc("cmincp")),
        is_some_expr(b, b.loc("cmaxcp")),
    );
    let stmts = [
        b.let_("mlo", opt_or(b, b.loc("cminl"), b.float(0.0))),
        b.let_("mhi", opt_or(b, b.loc("cmaxl"), float_inf(b))),
        b.let_("has_cp", has_cp),
        b.let_("cplo", opt_or(b, b.loc("cmincp"), b.float(0.0))),
        b.let_("cphi", opt_or(b, b.loc("cmaxcp"), float_inf(b))),
    ];
    let str_node = node_con(
        b,
        "$String",
        vec![
            b.loc("mlo"),
            b.loc("mhi"),
            b.loc("has_cp"),
            b.loc("cplo"),
            b.loc("cphi"),
        ],
    );
    let check_bounds = b.if_(
        b.bin(BinOp::Gt, b.loc("mlo"), b.loc("mhi")),
        arb_err(b, str_lit(b, "string constraints")),
        str_node,
    );
    let on_c = b.match_(b.loc("c"), [arm(c_pat(b), b.block(stmts, check_bounds))]);
    let default_node = node_con(
        b,
        "$String",
        vec![
            b.float(0.0),
            float_inf(b),
            b.bool_(false),
            b.float(0.0),
            float_inf(b),
        ],
    );
    b.lam(
        &["c_opt"],
        b.match_(
            b.loc("c_opt"),
            [arm(b.some_pat("c"), on_c), arm(Pat::Wild, default_node)],
        ),
    )
}

fn bigint_constrained_body(b: &ExprBuilder) -> Expr {
    let lo_val = b.if_(
        b.loc("cexmin"),
        b.bin(
            BinOp::Arith(Arith::Add, Num::BigInt),
            b.loc("v"),
            b.bigint("1"),
        ),
        b.loc("v"),
    );
    let lo = b.match_(
        b.loc("cmin"),
        [
            arm(b.some_pat("v"), b.some(lo_val)),
            arm(Pat::Wild, b.none()),
        ],
    );
    let hi_val = b.if_(
        b.loc("cexmax"),
        b.bin(
            BinOp::Arith(Arith::Sub, Num::BigInt),
            b.loc("v"),
            b.bigint("1"),
        ),
        b.loc("v"),
    );
    let hi = b.match_(
        b.loc("cmax"),
        [
            arm(b.some_pat("v"), b.some(hi_val)),
            arm(Pat::Wild, b.none()),
        ],
    );
    let hi_tgt = b.match_(
        b.loc("hi"),
        [
            arm(
                b.some_pat("hv"),
                b.if_(
                    b.bin(BinOp::Lt, b.loc("hv"), b.loc("z")),
                    b.loc("hv"),
                    b.loc("z"),
                ),
            ),
            arm(Pat::Wild, b.loc("z")),
        ],
    );
    let tgt = b.match_(
        b.loc("lo"),
        [
            arm(
                b.some_pat("lv"),
                b.if_(
                    b.bin(BinOp::Gt, b.loc("lv"), b.loc("z")),
                    b.loc("lv"),
                    b.loc("z"),
                ),
            ),
            arm(Pat::Wild, hi_tgt),
        ],
    );
    b.block(
        [b.let_("lo", lo), b.let_("hi", hi), b.let_("tgt", tgt)],
        node_con(b, "$BigInt", vec![b.loc("lo"), b.loc("hi"), b.loc("tgt")]),
    )
}

pub(super) fn build_bigint_helper(b: &ExprBuilder) -> Expr {
    let default_bi = node_con(b, "$BigInt", vec![b.none(), b.none(), b.loc("z")]);
    let on_c = b.match_(
        b.loc("c"),
        [arm(
            c_pat(b),
            b.if_(
                b.bin(BinOp::NotEq, b.loc("cord"), str_lit(b, "bigint")),
                default_bi.clone(),
                bigint_constrained_body(b),
            ),
        )],
    );
    let body = b.match_(
        b.loc("c_opt"),
        [arm(b.some_pat("c"), on_c), arm(Pat::Wild, default_bi)],
    );
    b.lam(&["c_opt"], b.block([b.let_("z", b.bigint("0"))], body))
}
