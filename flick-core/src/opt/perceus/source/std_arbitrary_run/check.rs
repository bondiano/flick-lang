//! Property-testing check runner (`arb_check`) for `Arbitrary`.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{con_app, pair_pat, rt};
use super::super::{Expr, ExprKind, Pat, Stmt};
use super::sample::{FalsifiedRecord, arb_count, arb_seed_of, falsified_con};
use super::{add_f, div_f, mul_f, sub_f};

fn check_on_fail0(b: &ExprBuilder) -> Expr {
    let sh_attempt = b.app5(
        rt(b, "Helper.arb_gen_attempt"),
        b.loc("arb"),
        b.loc("hashed"),
        b.loc("cur_att"),
        b.loc("csz"),
        b.bool_(true),
    );
    let sm_sh = b.match_(
        sh_attempt,
        [
            arm(b.some_pat("sh0"), b.loc("sh0")),
            arm(Pat::Wild, b.loc("sm")),
        ],
    );
    let rep_expr = b.app5(
        rt(b, "Helper.arb_make_replay"),
        b.loc("seed"),
        b.loc("cur_att"),
        b.loc("csz"),
        b.loc("path"),
        b.app(rt(b, "Helper.arb_prop_tag"), b.some(b.loc("fail0"))),
    );
    let falsified = falsified_con(
        b,
        FalsifiedRecord {
            init_v: b.loc("iv"),
            shrunk_v: b.loc("shv"),
            fail_v: b.loc("last_f"),
            runs: b.app(
                b.global("Int", "truncate"),
                add_f(b, b.var("runs"), b.float(1.0)),
            ),
            disc: b.app(b.global("Int", "truncate"), b.var("disc")),
            shrinks: b.app(b.global("List.Core", "length"), b.loc("path")),
            replay: b.loc("rep"),
        },
    );
    let on_shv = b.block(
        [
            b.let_("rep", rep_expr),
            b.assign("result", b.some(falsified)),
        ],
        b.unit(),
    );
    let on_shrunk = b.match_(b.loc("sh_s"), [arm(pair_pat(b, "shv", "_"), on_shv)]);
    let shrink_call = b.app4(
        rt(b, "Helper.arb_shrink_failure"),
        b.loc("sm_sh"),
        b.loc("fail0"),
        b.loc("prop"),
        b.loc("max_s"),
    );
    b.block(
        [b.let_("sm_sh", sm_sh)],
        b.match_(
            shrink_call,
            [arm(b.tup_pat(&["sh_s", "last_f", "path"]), on_shrunk)],
        ),
    )
}

fn check_loop_step(b: &ExprBuilder) -> Expr {
    let scaled_sz = b.app(
        b.global("Float", "floor"),
        add_f(
            b,
            div_f(
                b,
                mul_f(b, b.var("runs"), b.loc("sz")),
                sub_f(b, b.loc("target"), b.float(1.0)),
            ),
            b.float(0.5),
        ),
    );
    let csz = b.if_(
        b.bin(BinOp::Eq, b.loc("target"), b.float(1.0)),
        b.loc("sz"),
        scaled_sz,
    );
    let attempt = b.app5(
        rt(b, "Helper.arb_gen_attempt"),
        b.loc("arb"),
        b.loc("hashed"),
        b.loc("cur_att"),
        b.loc("csz"),
        b.bool_(false),
    );
    let eval_iv = b.match_(
        b.app2(rt(b, "Helper.arb_evaluate"), b.loc("prop"), b.loc("iv")),
        [
            arm(b.some_pat("fail0"), check_on_fail0(b)),
            arm(
                Pat::Wild,
                b.assign_e("runs", add_f(b, b.var("runs"), b.float(1.0))),
            ),
        ],
    );
    let on_sm = b.match_(b.loc("sm"), [arm(pair_pat(b, "iv", "_"), eval_iv)]);
    let exhausted_rec = b.at(ExprKind::Record(vec![
        (
            "discards".into(),
            b.app(b.global("Int", "truncate"), b.var("disc")),
        ),
        (
            "runs".into(),
            b.app(b.global("Int", "truncate"), b.var("runs")),
        ),
        (
            "seed".into(),
            b.app(rt(b, "Arbitrary.JsString"), b.loc("seed")),
        ),
    ]));
    let exhausted = con_app(
        b,
        "Arbitrary",
        "CheckResult",
        "Exhausted",
        vec![exhausted_rec],
    );
    let on_discard = b.block(
        [b.assign("disc", add_f(b, b.var("disc"), b.float(1.0)))],
        b.if_(
            b.bin(BinOp::Gt, b.var("disc"), b.loc("max_d")),
            b.assign_e("result", b.some(exhausted)),
            b.unit(),
        ),
    );
    let stmts = [
        b.let_("csz", csz),
        b.let_("cur_att", b.var("att")),
        b.assign("att", add_f(b, b.var("att"), b.float(1.0))),
        b.let_("s", attempt),
    ];
    b.block(
        stmts,
        b.match_(
            b.loc("s"),
            [arm(b.some_pat("sm"), on_sm), arm(Pat::Wild, on_discard)],
        ),
    )
}

fn check_fresh_body(b: &ExprBuilder) -> Expr {
    let def_d = b.if_(
        b.bin(
            BinOp::Gt,
            mul_f(b, b.loc("target"), b.float(10.0)),
            b.float(100.0),
        ),
        mul_f(b, b.loc("target"), b.float(10.0)),
        b.float(100.0),
    );
    let max_d = b.match_(
        b.field(b.loc("opts"), "maxDiscards"),
        [
            arm(
                b.some_pat("n"),
                b.app(b.global("Int", "to_float"), b.loc("n")),
            ),
            arm(Pat::Wild, b.loc("def_d")),
        ],
    );
    let no_res = b.match_(
        b.var("result"),
        [
            arm(b.some_pat("_"), b.bool_(false)),
            arm(Pat::Wild, b.bool_(true)),
        ],
    );
    let loop_cond = b.bin(
        BinOp::And,
        no_res,
        b.bin(BinOp::Lt, b.var("runs"), b.loc("target")),
    );
    let passed_rec = b.at(ExprKind::Record(vec![
        (
            "discards".into(),
            b.app(b.global("Int", "truncate"), b.var("disc")),
        ),
        (
            "runs".into(),
            b.app(b.global("Int", "truncate"), b.var("runs")),
        ),
    ]));
    let passed = con_app(b, "Arbitrary", "CheckResult", "Passed", vec![passed_rec]);
    let stmts = [
        b.let_(
            "target",
            arb_count(b, b.field(b.loc("opts"), "runs"), 100.0, "runs", true),
        ),
        b.let_(
            "sz",
            arb_count(b, b.field(b.loc("opts"), "size"), 10.0, "size", false),
        ),
        b.let_("def_d", def_d),
        b.let_("max_d", max_d),
        b.let_(
            "max_s",
            arb_count(
                b,
                b.field(b.loc("opts"), "maxShrinks"),
                100.0,
                "maxShrinks",
                false,
            ),
        ),
        b.let_("seed", arb_seed_of(b, b.field(b.loc("opts"), "seed"))),
        b.let_("hashed", b.app(rt(b, "Arbitrary.HashSeed"), b.loc("seed"))),
        b.var_decl("runs", b.float(0.0)),
        b.var_decl("disc", b.float(0.0)),
        b.var_decl("att", b.float(0.0)),
        b.var_decl("result", b.none()),
        Stmt::Expr(b.while_(loop_cond, check_loop_step(b))),
    ];
    b.block(
        stmts,
        b.match_(
            b.var("result"),
            [arm(b.some_pat("r"), b.loc("r")), arm(Pat::Wild, passed)],
        ),
    )
}

pub(super) fn arb_check_helper(b: &ExprBuilder) -> Expr {
    let on_rep = b.app3(
        rt(b, "Helper.arb_replay_check"),
        b.loc("arb"),
        b.loc("rep"),
        b.loc("prop"),
    );
    b.lam(
        &["opts", "prop", "arb"],
        b.match_(
            b.field(b.loc("opts"), "replay"),
            [
                arm(b.some_pat("rep"), on_rep),
                arm(Pat::Wild, check_fresh_body(b)),
            ],
        ),
    )
}
