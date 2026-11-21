//! Shared runner option helpers and `arb_sample` for `Arbitrary`.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{con_app, pair_pat, rt, str_lit};
use super::super::{Expr, ExprKind, Pat};
use super::{add_f, mul_f};

pub(super) struct FalsifiedRecord {
    pub(super) init_v: Expr,
    pub(super) shrunk_v: Expr,
    pub(super) fail_v: Expr,
    pub(super) runs: Expr,
    pub(super) disc: Expr,
    pub(super) shrinks: Expr,
    pub(super) replay: Expr,
}

pub(super) fn falsified_con(b: &ExprBuilder, fields: FalsifiedRecord) -> Expr {
    let rec = b.at(ExprKind::Record(vec![
        ("discards".into(), fields.disc),
        ("failure".into(), fields.fail_v),
        ("initialInput".into(), fields.init_v),
        ("replay".into(), fields.replay),
        ("runs".into(), fields.runs),
        ("shrinks".into(), fields.shrinks),
        ("shrunkInput".into(), fields.shrunk_v),
    ]));
    con_app(b, "Arbitrary", "CheckResult", "Falsified", vec![rec])
}

pub(super) fn arb_count(b: &ExprBuilder, opt: Expr, def: f64, label: &str, positive: bool) -> Expr {
    let neg_msg = format!("Error: {label} must be a non-negative safe integer");
    let pos_msg = format!("Error: {label} must be greater than zero");
    let v_expr = b.match_(
        opt,
        [
            arm(
                b.some_pat("n"),
                b.app(b.global("Int", "to_float"), b.loc("n")),
            ),
            arm(Pat::Wild, b.float(def)),
        ],
    );
    let non_neg = if positive {
        b.if_(
            b.bin(BinOp::Eq, b.loc("v"), b.float(0.0)),
            b.app(rt(b, "Schema.Defect"), str_lit(b, &pos_msg)),
            b.loc("v"),
        )
    } else {
        b.loc("v")
    };
    let check = b.if_(
        b.bin(BinOp::Lt, b.loc("v"), b.float(0.0)),
        b.app(rt(b, "Schema.Defect"), str_lit(b, &neg_msg)),
        non_neg,
    );
    b.block([b.let_("v", v_expr)], check)
}

pub(super) fn arb_seed_of(b: &ExprBuilder, opt: Expr) -> Expr {
    let rnd_seed = b.app(
        b.global("Float", "floor"),
        mul_f(
            b,
            b.app(rt(b, "Host.RandomNext"), b.unit()),
            b.float(2_147_483_647.0),
        ),
    );
    b.match_(
        opt,
        [arm(b.some_pat("s"), b.loc("s")), arm(Pat::Wild, rnd_seed)],
    )
}

fn sample_loop_step(b: &ExprBuilder) -> Expr {
    let attempt = b.app5(
        rt(b, "Helper.arb_gen_attempt"),
        b.loc("arb"),
        b.loc("hashed"),
        b.var("att"),
        b.loc("sz"),
        b.bool_(false),
    );
    let yield_v = b.block(
        [b.assign("gen_n", add_f(b, b.var("gen_n"), b.float(1.0)))],
        b.yield_(b.loc("v")),
    );
    let on_sm = b.match_(b.loc("sm"), [arm(pair_pat(b, "v", "_"), yield_v)]);
    let err_rec = b.at(ExprKind::Record(vec![
        (
            "discards".into(),
            b.app(b.global("Int", "truncate"), b.var("disc")),
        ),
        (
            "generated".into(),
            b.app(b.global("Int", "truncate"), b.var("gen_n")),
        ),
        (
            "seed".into(),
            b.app(rt(b, "Arbitrary.JsString"), b.loc("seed")),
        ),
    ]));
    let sample_err = con_app(b, "Arbitrary", "SampleError", "SampleError", vec![err_rec]);
    let on_discard = b.block(
        [b.assign("disc", add_f(b, b.var("disc"), b.float(1.0)))],
        b.if_(
            b.bin(BinOp::Gt, b.var("disc"), b.loc("max_d")),
            b.app(b.global("Fail", "fail"), sample_err),
            b.unit(),
        ),
    );
    b.block(
        [
            b.let_("s", attempt),
            b.assign("att", add_f(b, b.var("att"), b.float(1.0))),
        ],
        b.match_(
            b.loc("s"),
            [arm(b.some_pat("sm"), on_sm), arm(Pat::Wild, on_discard)],
        ),
    )
}

pub(super) fn arb_sample_helper(b: &ExprBuilder) -> Expr {
    let def_d = b.if_(
        b.bin(
            BinOp::Gt,
            mul_f(b, b.loc("cnt"), b.float(10.0)),
            b.float(100.0),
        ),
        mul_f(b, b.loc("cnt"), b.float(10.0)),
        b.float(100.0),
    );
    let check_d = b.block(
        [b.let_("d", b.app(b.global("Int", "to_float"), b.loc("n")))],
        b.if_(
            b.bin(BinOp::Lt, b.loc("d"), b.float(0.0)),
            b.app(
                rt(b, "Schema.Defect"),
                str_lit(b, "Error: maxDiscards must be a non-negative safe integer"),
            ),
            b.loc("d"),
        ),
    );
    let max_d = b.match_(
        b.field(b.loc("opts"), "maxDiscards"),
        [
            arm(b.some_pat("n"), check_d),
            arm(Pat::Wild, b.loc("def_d")),
        ],
    );
    let stmts = [
        b.let_(
            "cnt",
            arb_count(b, b.field(b.loc("opts"), "count"), 10.0, "count", false),
        ),
        b.let_(
            "sz",
            arb_count(b, b.field(b.loc("opts"), "size"), 10.0, "size", false),
        ),
        b.let_("def_d", def_d),
        b.let_("max_d", max_d),
        b.let_("seed", arb_seed_of(b, b.field(b.loc("opts"), "seed"))),
        b.let_("hashed", b.app(rt(b, "Arbitrary.HashSeed"), b.loc("seed"))),
        b.var_decl("gen_n", b.float(0.0)),
        b.var_decl("disc", b.float(0.0)),
        b.var_decl("att", b.float(0.0)),
    ];
    let body = b.gen_strict(b.while_(
        b.bin(BinOp::Lt, b.var("gen_n"), b.loc("cnt")),
        sample_loop_step(b),
    ));
    b.lam(&["opts", "arb"], b.block(stmts, body))
}
