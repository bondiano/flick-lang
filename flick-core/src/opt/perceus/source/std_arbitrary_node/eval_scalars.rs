//! Scalar `$Node` evaluation arms (`$Number`, `$BigInt`, `$Decimal`, `$String`).

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt, str_lit};
use super::super::{Expr, Pat};
use super::{add_f, div_f, mul_f, sub_f};

fn num_int_branch(b: &ExprBuilder) -> Expr {
    let neg_ctr = b.match_(
        b.loc("max_o"),
        [
            arm(
                b.some_pat("hi"),
                b.if_(
                    b.bin(BinOp::Lt, b.loc("hi"), b.float(0.0)),
                    b.loc("hi"),
                    b.float(0.0),
                ),
            ),
            arm(Pat::Wild, b.float(0.0)),
        ],
    );
    let ctr = b.match_(
        b.loc("min_o"),
        [
            arm(
                b.some_pat("lo"),
                b.if_(
                    b.bin(BinOp::Gt, b.loc("lo"), b.float(0.0)),
                    b.loc("lo"),
                    b.float(0.0),
                ),
            ),
            arm(Pat::Wild, neg_ctr),
        ],
    );
    let from = b.match_(
        b.loc("min_o"),
        [
            arm(b.some_pat("lo"), b.loc("lo")),
            arm(Pat::Wild, sub_f(b, b.loc("ctr"), b.loc("mag"))),
        ],
    );
    let to = b.match_(
        b.loc("max_o"),
        [
            arm(b.some_pat("hi"), b.loc("hi")),
            arm(Pat::Wild, add_f(b, b.loc("ctr"), b.loc("mag"))),
        ],
    );
    let stmts = [
        b.let_("sq", mul_f(b, b.loc("sz"), b.loc("sz"))),
        b.let_(
            "mag",
            b.if_(
                b.bin(BinOp::Gt, b.loc("sq"), b.float(1.0)),
                b.loc("sq"),
                b.float(1.0),
            ),
        ),
        b.let_("ctr", ctr),
        b.let_("from", from),
        b.let_("to", to),
    ];
    b.block(
        stmts,
        b.app3(
            rt(b, "Helper.arb_numeric_int"),
            b.loc("st"),
            b.loc("from"),
            b.loc("to"),
        ),
    )
}

fn num_float_branch(b: &ExprBuilder) -> Expr {
    let inf = div_f(b, b.float(1.0), b.float(0.0));
    let neg_inf = div_f(b, b.float(-1.0), b.float(0.0));
    let def_lo = b.if_(
        b.bin(BinOp::Eq, b.loc("nmode"), str_lit(b, "")),
        neg_inf,
        b.float(-f64::MAX),
    );
    let def_hi = b.if_(
        b.bin(BinOp::Eq, b.loc("nmode"), str_lit(b, "")),
        inf,
        b.float(f64::MAX),
    );
    let lo = b.match_(
        b.loc("min_o"),
        [
            arm(b.some_pat("x"), b.loc("x")),
            arm(Pat::Wild, b.loc("def_lo")),
        ],
    );
    let hi = b.match_(
        b.loc("max_o"),
        [
            arm(b.some_pat("x"), b.loc("x")),
            arm(Pat::Wild, b.loc("def_hi")),
        ],
    );
    let no_min = b.match_(
        b.loc("min_o"),
        [
            arm(b.some_pat("_"), b.bool_(false)),
            arm(Pat::Wild, b.bool_(true)),
        ],
    );
    let allow_nan = b.bin(
        BinOp::And,
        b.bin(BinOp::Eq, b.loc("nmode"), str_lit(b, "")),
        no_min,
    );
    let rnd = b.app5(
        rt(b, "Arbitrary.RandomNumber"),
        b.rget(b.loc("rr")),
        b.loc("lo"),
        b.loc("hi"),
        b.loc("allow_nan"),
        b.loc("bias"),
    );
    let stmts = [
        b.let_("def_lo", def_lo),
        b.let_("def_hi", def_hi),
        b.let_("lo", lo),
        b.let_("hi", hi),
        b.let_("allow_nan", allow_nan),
    ];
    b.block(
        stmts,
        b.match_(
            rnd,
            [arm(
                pair_pat(b, "nr", "nv"),
                b.block([b.rset(b.loc("rr"), b.loc("nr"))], b.loc("nv")),
            )],
        ),
    )
}

pub(super) fn num_arm(b: &ExprBuilder) -> Expr {
    let val_expr = b.if_(b.loc("is_int"), num_int_branch(b), num_float_branch(b));
    let sample = b.if_(
        b.loc("shk"),
        b.app5(
            rt(b, "Helper.arb_number_sample"),
            b.loc("v"),
            b.loc("min_o"),
            b.loc("max_o"),
            b.loc("is_int"),
            b.none(),
        ),
        b.tuple([b.loc("v"), b.none()]),
    );
    b.block(
        [
            b.let_(
                "is_int",
                b.bin(BinOp::Eq, b.loc("nmode"), str_lit(b, "integer")),
            ),
            b.let_("v", val_expr),
        ],
        b.some(sample),
    )
}

pub(super) fn bigint_arm(b: &ExprBuilder) -> Expr {
    let rnd = b.app4(
        rt(b, "Arbitrary.RandomBigIntBiased"),
        b.rget(b.loc("rr")),
        b.loc("bmin"),
        b.loc("bmax"),
        b.loc("bias"),
    );
    let sample = b.if_(
        b.loc("shk"),
        b.app3(
            rt(b, "Helper.arb_bigint_sample"),
            b.loc("bv"),
            b.loc("btgt"),
            b.none(),
        ),
        b.tuple([b.loc("bv"), b.none()]),
    );
    b.match_(
        rnd,
        [arm(
            pair_pat(b, "nr", "bv"),
            b.block([b.rset(b.loc("rr"), b.loc("nr"))], b.some(sample)),
        )],
    )
}

pub(super) fn decimal_arm(b: &ExprBuilder) -> Expr {
    let base_sc = b.if_(
        b.bin(BinOp::Gt, b.loc("sc"), b.float(0.0)),
        b.loc("sc"),
        b.float(0.0),
    );
    let plus_128 = add_f(b, b.loc("base_sc"), b.float(128.0));
    let max_sc = b.if_(
        b.bin(
            BinOp::Lt,
            plus_128.clone(),
            b.float(9_007_199_254_740_991.0),
        ),
        plus_128,
        b.float(9_007_199_254_740_991.0),
    );
    let sample = b.if_(
        b.loc("shk"),
        b.app4(
            rt(b, "Helper.arb_decimal_sample"),
            b.loc("dv"),
            b.loc("zero_d"),
            b.none(),
            b.loc("max_sc"),
        ),
        b.tuple([b.loc("dv"), b.none()]),
    );
    let body = b.block(
        [
            b.rset(b.loc("rr"), b.loc("nr")),
            b.let_(
                "zero_d",
                b.app(rt(b, "BigDecimal.FromInteger"), b.bigint("0")),
            ),
            b.let_("sc", b.app(rt(b, "BigDecimal.Scale"), b.loc("dv"))),
            b.let_("base_sc", base_sc),
            b.let_("max_sc", max_sc),
        ],
        b.some(sample),
    );
    b.match_(
        b.app(rt(b, "Arbitrary.RandomDecimal"), b.rget(b.loc("rr"))),
        [arm(pair_pat(b, "nr", "dv"), body)],
    )
}

pub(super) fn string_arm(b: &ExprBuilder) -> Expr {
    let cp_hi = b.if_(
        b.bin(BinOp::Lt, b.loc("cpmax"), b.loc("up")),
        b.loc("cpmax"),
        b.loc("up"),
    );
    let cp_str = b.match_(
        b.app5(
            rt(b, "Arbitrary.RandomCodePointString"),
            b.rget(b.loc("rr")),
            b.loc("smin"),
            b.loc("up"),
            b.loc("cpmin"),
            cp_hi,
        ),
        [arm(
            pair_pat(b, "nr", "s"),
            b.block([b.rset(b.loc("rr"), b.loc("nr"))], b.loc("s")),
        )],
    );
    let plain_str = b.match_(
        b.app4(
            rt(b, "Arbitrary.RandomString"),
            b.rget(b.loc("rr")),
            b.loc("smin"),
            b.loc("up"),
            b.loc("bias"),
        ),
        [arm(
            pair_pat(b, "nr", "s"),
            b.block([b.rset(b.loc("rr"), b.loc("nr"))], b.loc("s")),
        )],
    );
    let sample = b.if_(
        b.loc("shk"),
        b.app2(
            rt(b, "Helper.arb_string_sample"),
            b.loc("sv"),
            b.app(b.global("Int", "truncate"), b.loc("smin")),
        ),
        b.tuple([b.loc("sv"), b.none()]),
    );
    let stmts = [
        b.let_(
            "cur",
            b.if_(
                b.bin(BinOp::Gt, b.loc("smin"), b.loc("sz")),
                b.loc("smin"),
                b.loc("sz"),
            ),
        ),
        b.let_(
            "up",
            b.if_(
                b.bin(BinOp::Lt, b.loc("smax"), b.loc("cur")),
                b.loc("smax"),
                b.loc("cur"),
            ),
        ),
        b.let_("sv", b.if_(b.loc("has_cp"), cp_str, plain_str)),
    ];
    b.block(stmts, b.some(sample))
}
