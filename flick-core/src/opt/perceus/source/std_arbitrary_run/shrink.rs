//! Property evaluation, shrinking loop, and replay-path helpers for `Arbitrary`.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{cat, con_app, pair_pat, rt, str_lit};
use super::super::{Expr, Pat, Stmt};
use super::add_f;

pub(super) fn evaluate_helper(b: &ExprBuilder) -> Expr {
    let rf_con = con_app(b, "Arbitrary", "PropertyFailure", "ReturnedFalse", vec![]);
    let pe_con = |e: Expr| con_app(b, "Arbitrary", "PropertyFailure", "PropertyError", vec![e]);
    let find_err = b.match_(
        b.app(b.global("Cause", "find_error_option"), b.loc("c")),
        [
            arm(b.some_pat("err"), b.some(pe_con(b.loc("err")))),
            arm(Pat::Wild, b.app(b.global("Failure", "rethrow"), b.loc("c"))),
        ],
    );
    let fatal = b.bin(
        BinOp::Or,
        b.app(b.global("Cause", "has_dies"), b.loc("c")),
        b.app(b.global("Cause", "has_interrupts"), b.loc("c")),
    );
    let on_cause = b.if_(
        fatal,
        b.app(b.global("Failure", "rethrow"), b.loc("c")),
        find_err,
    );
    let on_fail = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("ex")),
        [arm(b.some_pat("c"), on_cause)],
    );
    let ex_let = b.let_(
        "ex",
        b.app(
            b.global("Effect.Core", "exit"),
            b.task(b.app(b.loc("prop"), b.loc("val"))),
        ),
    );
    let body = b.match_(
        b.app(b.global("Exit", "value"), b.loc("ex")),
        [
            arm(
                b.some_pat("ok"),
                b.if_(b.loc("ok"), b.none(), b.some(rf_con)),
            ),
            arm(Pat::Wild, on_fail),
        ],
    );
    b.lam(&["prop", "val"], b.block([ex_let], body))
}

pub(super) fn prop_tag_helper(b: &ExprBuilder) -> Expr {
    let pe_pat = Pat::Con(
        ExprBuilder::con("Arbitrary", "PropertyFailure", "PropertyError", 1),
        vec![Pat::Wild],
    );
    let on_f = b.match_(
        b.loc("f"),
        [
            arm(pe_pat, str_lit(b, "PropertyError")),
            arm(Pat::Wild, str_lit(b, "ReturnedFalse")),
        ],
    );
    let body = b.match_(
        b.loc("fail_opt"),
        [arm(b.some_pat("f"), on_f), arm(Pat::Wild, str_lit(b, ""))],
    );
    b.lam(&["fail_opt"], body)
}

fn shrink_on_cand(b: &ExprBuilder) -> Expr {
    let update_fail = b.match_(
        b.loc("out"),
        [
            arm(b.some_pat("nf"), b.assign_e("fail", b.loc("nf"))),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let append_path = b.app2(
        b.global("List.Core", "append"),
        b.var("path"),
        b.list([b.var("idx")]),
    );
    let on_same_tag = b.block(
        [
            b.assign("found", b.some(b.loc("cand"))),
            Stmt::Expr(update_fail),
            b.assign("path", append_path),
            b.assign("more", b.bool_(false)),
        ],
        b.unit(),
    );
    let same_tag = b.bin(
        BinOp::Eq,
        b.app(rt(b, "Helper.arb_prop_tag"), b.loc("out")),
        b.loc("tag"),
    );
    let eval_cand = b.block(
        [b.let_(
            "out",
            b.app2(rt(b, "Helper.arb_evaluate"), b.loc("prop"), b.loc("cv")),
        )],
        b.if_(
            same_tag,
            on_same_tag,
            b.assign_e("idx", b.add_i(b.var("idx"), b.int(1))),
        ),
    );
    b.match_(b.loc("cand"), [arm(pair_pat(b, "cv", "_"), eval_cand)])
}

fn shrink_inner_loop(b: &ExprBuilder) -> Expr {
    let on_opt_c = b.block(
        [b.assign("insp", add_f(b, b.var("insp"), b.float(1.0)))],
        b.match_(
            b.loc("opt_c"),
            [
                arm(b.some_pat("cand"), shrink_on_cand(b)),
                arm(Pat::Wild, b.unit()),
            ],
        ),
    );
    let pull_step = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.some_pat("opt_c"), on_opt_c),
            arm(Pat::Wild, b.assign_e("more", b.bool_(false))),
        ],
    );
    let cond = b.bin(
        BinOp::And,
        b.var("more"),
        b.bin(BinOp::Lt, b.var("insp"), b.loc("max_s")),
    );
    let after_loop = b.match_(
        b.var("found"),
        [
            arm(b.some_pat("fnd"), b.assign_e("cur", b.loc("fnd"))),
            arm(Pat::Wild, b.assign_e("keep_go", b.bool_(false))),
        ],
    );
    b.block(
        [
            b.var_decl("idx", b.int(0)),
            b.var_decl("found", b.none()),
            b.var_decl("more", b.bool_(true)),
            Stmt::Expr(b.while_(cond, pull_step)),
        ],
        after_loop,
    )
}

pub(super) fn shrink_failure_helper(b: &ExprBuilder) -> Expr {
    let on_sh = b.match_(
        b.loc("sh_opt"),
        [
            arm(b.some_pat("pull"), shrink_inner_loop(b)),
            arm(Pat::Wild, b.assign_e("keep_go", b.bool_(false))),
        ],
    );
    let outer_cond = b.bin(
        BinOp::And,
        b.var("keep_go"),
        b.bin(BinOp::Lt, b.var("insp"), b.loc("max_s")),
    );
    let outer_loop = b.while_(
        outer_cond,
        b.match_(b.var("cur"), [arm(pair_pat(b, "_v", "sh_opt"), on_sh)]),
    );
    let stmts = [
        b.let_(
            "tag",
            b.app(rt(b, "Helper.arb_prop_tag"), b.some(b.loc("fail0"))),
        ),
        b.var_decl("cur", b.loc("init")),
        b.var_decl("fail", b.loc("fail0")),
        b.var_decl("insp", b.float(0.0)),
        b.var_decl("path", b.list(vec![])),
        b.var_decl("keep_go", b.bool_(true)),
        Stmt::Expr(outer_loop),
    ];
    b.lam(
        &["init", "fail0", "prop", "max_s"],
        b.block(stmts, b.tuple([b.var("cur"), b.var("fail"), b.var("path")])),
    )
}

fn replay_eval_sel(b: &ExprBuilder) -> Expr {
    let mismatch_passed = b.block(
        [b.assign("mismatch", str_lit(b, "ShrinkPassed"))],
        b.break_("$frp", b.unit()),
    );
    let update_fail = b.match_(
        b.loc("out"),
        [
            arm(b.some_pat("nf"), b.assign_e("fail", b.loc("nf"))),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let accept_s = b.block(
        [b.assign("cur", b.loc("s")), Stmt::Expr(update_fail)],
        b.unit(),
    );
    let wrong_tag = b.bin(
        BinOp::NotEq,
        b.app(rt(b, "Helper.arb_prop_tag"), b.loc("out")),
        b.loc("tag"),
    );
    let eval_sv = b.block(
        [b.let_(
            "out",
            b.app2(rt(b, "Helper.arb_evaluate"), b.loc("prop"), b.loc("sv")),
        )],
        b.if_(wrong_tag, mismatch_passed, accept_s),
    );
    b.match_(b.loc("s"), [arm(pair_pat(b, "sv", "_"), eval_sv)])
}

fn replay_step_pull(b: &ExprBuilder) -> Expr {
    let on_cand = b.block(
        [
            Stmt::Expr(b.if_(
                b.bin(BinOp::Eq, b.var("idx"), b.loc("tgt")),
                b.assign_e("sel", b.some(b.loc("cand"))),
                b.unit(),
            )),
            b.assign("idx", b.add_i(b.var("idx"), b.int(1))),
        ],
        b.unit(),
    );
    let on_opt_c = b.match_(
        b.loc("opt_c"),
        [arm(b.some_pat("cand"), on_cand), arm(Pat::Wild, b.unit())],
    );
    let seek_loop = b.while_(
        b.bin(
            BinOp::And,
            b.var("ok"),
            b.bin(BinOp::Le, b.var("idx"), b.loc("tgt")),
        ),
        b.match_(
            b.app(b.loc("pull"), b.unit()),
            [
                arm(b.some_pat("opt_c"), on_opt_c),
                arm(Pat::Wild, b.assign_e("ok", b.bool_(false))),
            ],
        ),
    );
    let unavailable = b.block(
        [b.assign("mismatch", str_lit(b, "ShrinkPathUnavailable"))],
        b.break_("$frp", b.unit()),
    );
    b.block(
        [
            b.var_decl("idx", b.int(0)),
            b.var_decl("sel", b.none()),
            b.var_decl("ok", b.bool_(true)),
            Stmt::Expr(seek_loop),
        ],
        b.match_(
            b.var("sel"),
            [
                arm(b.some_pat("s"), replay_eval_sel(b)),
                arm(Pat::Wild, unavailable),
            ],
        ),
    )
}

pub(super) fn follow_replay_helper(b: &ExprBuilder) -> Expr {
    let unavailable = b.block(
        [b.assign("mismatch", str_lit(b, "ShrinkPathUnavailable"))],
        b.break_("$frp", b.unit()),
    );
    let on_sh = b.match_(
        b.loc("sh_opt"),
        [
            arm(b.some_pat("pull"), replay_step_pull(b)),
            arm(Pat::Wild, unavailable),
        ],
    );
    let step_body = b.block(
        [b.let_("tgt", b.app(b.global("Int", "truncate"), b.loc("at_f")))],
        b.match_(b.var("cur"), [arm(pair_pat(b, "_v", "sh_opt"), on_sh)]),
    );
    let stmts = [
        b.let_(
            "tag",
            b.app(rt(b, "Helper.arb_prop_tag"), b.some(b.loc("fail0"))),
        ),
        b.var_decl("cur", b.loc("init")),
        b.var_decl("fail", b.loc("fail0")),
        b.var_decl("mismatch", str_lit(b, "")),
        Stmt::Expr(b.for_ctrl("$frp", "at_f", b.loc("path"), step_body)),
    ];
    b.lam(
        &["init", "fail0", "path", "prop"],
        b.block(
            stmts,
            b.tuple([b.var("cur"), b.var("fail"), b.var("mismatch")]),
        ),
    )
}

fn cat_chain(b: &ExprBuilder, init: impl IntoIterator<Item = Expr>, last: Expr) -> Expr {
    let mut parts: Vec<Expr> = init.into_iter().collect();
    let mut acc = last;
    while let Some(prev) = parts.pop() {
        acc = cat(b, prev, acc);
    }
    acc
}

pub(super) fn make_replay_helper(b: &ExprBuilder) -> Expr {
    let steps = b.gen_strict(b.each(
        "p",
        b.loc("path"),
        b.yield_(b.app(b.global("Int", "to_string"), b.loc("p"))),
    ));
    let stmts = [
        b.let_("steps", steps),
        b.let_("s_str", b.app(rt(b, "Arbitrary.JsString"), b.loc("seed"))),
        b.let_("s_q", b.app(rt(b, "Arbitrary.QuoteJson"), b.loc("s_str"))),
        b.let_("a_str", b.app(rt(b, "Arbitrary.JsString"), b.loc("att"))),
        b.let_("z_str", b.app(rt(b, "Arbitrary.JsString"), b.loc("sz"))),
        b.let_("t_q", b.app(rt(b, "Arbitrary.QuoteJson"), b.loc("tag"))),
        b.let_(
            "p_str",
            b.app2(b.global("String", "join"), str_lit(b, ","), b.loc("steps")),
        ),
    ];
    let joined = cat_chain(
        b,
        [
            str_lit(b, "[0,"),
            b.loc("s_q"),
            str_lit(b, ","),
            b.loc("a_str"),
            str_lit(b, ","),
            b.loc("z_str"),
            str_lit(b, ",["),
            b.loc("p_str"),
            str_lit(b, "],"),
            b.loc("t_q"),
        ],
        str_lit(b, "]"),
    );
    b.lam(
        &["seed", "att", "sz", "path", "tag"],
        b.block(stmts, joined),
    )
}
