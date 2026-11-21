//! Synthetic `$rt.Helper.cfg_*` scalar and schema decoder helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{cat, rt, str_lit};
use super::super::{Expr, Pat, Span};
use super::decode::{cfg_decode_array_helper, cfg_decode_at_helper};

pub(in super::super) fn config_helper_expr(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$cfgh");
    match name {
        "cfg_dec_number" => Some(b.lam(
            &["t"],
            b.match_(
                b.app(rt(&b, "Config.DecodeNumber"), b.loc("t")),
                [
                    arm(b.some_pat("x"), b.ok(b.loc("x"))),
                    arm(
                        Pat::Wild,
                        b.err(str_lit(
                            &b,
                            "Expected a string representing a finite number",
                        )),
                    ),
                ],
            ),
        )),
        "cfg_dec_finite" => {
            let check_finite = b.if_(
                b.app(b.global("Float", "is_finite"), b.loc("x")),
                b.ok(b.loc("x")),
                b.err(str_lit(
                    &b,
                    "Expected a string representing a finite number",
                )),
            );
            Some(b.lam(
                &["t"],
                b.match_(
                    b.app(rt(&b, "Helper.cfg_dec_number"), b.loc("t")),
                    [
                        arm(b.ok_pat("x"), check_finite),
                        arm(b.err_pat("e"), b.err(b.loc("e"))),
                    ],
                ),
            ))
        }
        "cfg_dec_int" => Some(cfg_between_helper(
            &b,
            -2_147_483_648.0,
            2_147_483_647.0,
            true,
        )),
        "cfg_dec_port" => Some(cfg_between_helper(&b, 1.0, 65535.0, false)),
        "cfg_dec_bool" => Some(cfg_dec_bool_helper(&b)),
        "cfg_dec_literals" => Some(cfg_dec_literals_helper(&b)),
        "cfg_schema_dec" => Some(cfg_schema_dec_helper(&b)),
        "cfg_decode_at" => Some(cfg_decode_at_helper(&b)),
        "cfg_decode_array" => Some(cfg_decode_array_helper(&b)),
        _ => None,
    }
}

fn cfg_between_helper(b: &ExprBuilder, lo: f64, hi: f64, to_int: bool) -> Expr {
    let (lo_s, hi_s) = if to_int {
        ("-2147483648", "2147483647")
    } else {
        ("1", "65535")
    };
    let msg = format!("Expected a value between {lo_s} and {hi_s}");
    let ret = if to_int {
        b.ok(b.app(b.global("Int", "truncate"), b.loc("x")))
    } else {
        b.ok(b.loc("x"))
    };
    let out_of_bounds = b.bin(
        BinOp::Or,
        b.bin(BinOp::Lt, b.loc("x"), b.float(lo)),
        b.bin(BinOp::Gt, b.loc("x"), b.float(hi)),
    );
    let check_range = b.if_(out_of_bounds, b.err(str_lit(b, &msg)), ret);
    let not_int = b.bin(
        BinOp::NotEq,
        b.loc("x"),
        b.app(b.global("Float", "floor"), b.loc("x")),
    );
    let on_num = b.if_(
        not_int,
        b.err(str_lit(b, "Expected an integer")),
        check_range,
    );
    b.lam(
        &["t"],
        b.match_(
            b.app(rt(b, "Helper.cfg_dec_finite"), b.loc("t")),
            [
                arm(b.ok_pat("x"), on_num),
                arm(b.err_pat("e"), b.err(b.loc("e"))),
            ],
        ),
    )
}

fn cfg_dec_bool_helper(b: &ExprBuilder) -> Expr {
    let all = b.list([
        str_lit(b, "true"),
        str_lit(b, "yes"),
        str_lit(b, "on"),
        str_lit(b, "1"),
        str_lit(b, "y"),
        str_lit(b, "false"),
        str_lit(b, "no"),
        str_lit(b, "off"),
        str_lit(b, "0"),
        str_lit(b, "n"),
    ]);
    let trues = b.list([
        str_lit(b, "true"),
        str_lit(b, "yes"),
        str_lit(b, "on"),
        str_lit(b, "1"),
        str_lit(b, "y"),
    ]);
    let is_true = b.app2(
        b.global("List.Core", "any"),
        b.lam(&["x"], b.bin(BinOp::Eq, b.loc("x"), b.loc("t"))),
        trues,
    );
    b.lam(
        &["t"],
        b.match_(
            b.app2(rt(b, "Helper.cfg_dec_literals"), all, b.loc("t")),
            [
                arm(b.ok_pat("_"), b.ok(is_true)),
                arm(b.err_pat("e"), b.err(b.loc("e"))),
            ],
        ),
    )
}

fn cfg_dec_literals_helper(b: &ExprBuilder) -> Expr {
    let found = b.app2(
        b.global("List.Core", "any"),
        b.lam(&["o"], b.bin(BinOp::Eq, b.loc("o"), b.loc("t"))),
        b.loc("opts"),
    );
    let quoted = b.gen_strict(b.each(
        "o",
        b.loc("opts"),
        b.yield_(b.app(rt(b, "Arbitrary.QuoteJson"), b.loc("o"))),
    ));
    let miss_msg = cat(
        b,
        str_lit(b, "Expected "),
        b.app2(b.global("String", "join"), str_lit(b, " | "), b.loc("qs")),
    );
    let on_miss = b.block([b.let_("qs", quoted)], b.err(miss_msg));
    b.lam(&["opts", "t"], b.if_(found, b.ok(b.loc("t")), on_miss))
}

fn cfg_schema_dec_helper(b: &ExprBuilder) -> Expr {
    let kc = |c: &str, n: usize| ExprBuilder::con("Schema", "$Kind", c, n);
    let bool_arm = b.if_(
        b.bin(BinOp::Eq, b.loc("t"), str_lit(b, "true")),
        b.ok(b.bool_(true)),
        b.if_(
            b.bin(BinOp::Eq, b.loc("t"), str_lit(b, "false")),
            b.ok(b.bool_(false)),
            b.err(str_lit(b, "Expected \"true\" | \"false\"")),
        ),
    );
    let bigint_arm = b.match_(
        b.app(rt(b, "BigInt.FromString"), b.loc("t")),
        [
            arm(b.some_pat("n"), b.ok(b.loc("n"))),
            arm(
                Pat::Wild,
                b.err(str_lit(b, "Expected a string representing a bigint")),
            ),
        ],
    );
    let lazy_arm = b.app2(
        rt(b, "Helper.cfg_schema_dec"),
        b.app(b.loc("th"), b.unit()),
        b.loc("t"),
    );
    b.lam(
        &["sch", "t"],
        b.match_(
            b.app(rt(b, "Helper.schema_kind"), b.loc("sch")),
            [
                arm(
                    Pat::Con(kc("$Lazy", 2), vec![Pat::Wild, Pat::Bind(b.name("th"))]),
                    lazy_arm,
                ),
                arm(
                    Pat::Con(kc("$Number", 0), vec![]),
                    b.app(rt(b, "Helper.cfg_dec_number"), b.loc("t")),
                ),
                arm(
                    Pat::Con(kc("$Int", 0), vec![]),
                    b.app(rt(b, "Helper.cfg_dec_int"), b.loc("t")),
                ),
                arm(Pat::Con(kc("$Bool", 0), vec![]), bool_arm),
                arm(Pat::Con(kc("$BigInt", 0), vec![]), bigint_arm),
                arm(Pat::Wild, b.ok(b.loc("t"))),
            ],
        ),
    )
}
