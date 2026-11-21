//! `GenState` RNG, budget-reservation, shuffle, and attempt helpers for `Arbitrary`.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat, Stmt};
use super::{add_f, div_f, mul_f, st_pat, sub_f};

pub(super) fn st_below_helper(b: &ExprBuilder) -> Expr {
    let draw = b.match_(
        b.app(rt(b, "Arbitrary.XoshiroNext"), b.rget(b.loc("rr"))),
        [arm(
            pair_pat(b, "nr", "nv"),
            b.block(
                [b.rset(b.loc("rr"), b.loc("nr")), b.assign("v", b.loc("nv"))],
                b.unit(),
            ),
        )],
    );
    let bkt = b.app(
        b.global("Float", "floor"),
        div_f(b, b.float(4_294_967_296.0), b.loc("n")),
    );
    let body = b.block(
        [
            b.let_("bkt", bkt),
            b.let_("acc", mul_f(b, b.loc("bkt"), b.loc("n"))),
            b.var_decl("v", b.loc("acc")),
            Stmt::Expr(b.while_(b.bin(BinOp::Ge, b.var("v"), b.loc("acc")), draw)),
        ],
        b.app(
            b.global("Float", "floor"),
            div_f(b, b.var("v"), b.loc("bkt")),
        ),
    );
    b.lam(&["st", "n"], b.match_(b.loc("st"), [arm(st_pat(b), body)]))
}

pub(super) fn st_index_helper(b: &ExprBuilder) -> Expr {
    let below = b.app2(
        rt(b, "Helper.arb_st_below"),
        b.loc("st"),
        b.app(b.global("Int", "to_float"), b.loc("n")),
    );
    let body = b.if_(
        b.bin(BinOp::Le, b.loc("n"), b.int(1)),
        b.int(0),
        b.app(b.global("Int", "truncate"), below),
    );
    b.lam(&["st", "n"], body)
}

pub(super) fn st_int_helper(b: &ExprBuilder) -> Expr {
    let draw = b.app3(
        rt(b, "Arbitrary.RandomInt"),
        b.rget(b.loc("rr")),
        b.loc("lo"),
        b.loc("hi"),
    );
    let body = b.match_(
        draw,
        [arm(
            pair_pat(b, "nr", "v"),
            b.block([b.rset(b.loc("rr"), b.loc("nr"))], b.loc("v")),
        )],
    );
    b.lam(
        &["st", "lo", "hi"],
        b.match_(b.loc("st"), [arm(st_pat(b), body)]),
    )
}

pub(super) fn st_bool_helper(b: &ExprBuilder) -> Expr {
    let half = b.app(
        b.global("Float", "floor"),
        div_f(b, b.loc("v"), b.float(2.0)),
    );
    let on_draw = b.block(
        [b.rset(b.loc("rr"), b.loc("nr")), b.let_("half", half)],
        b.bin(
            BinOp::NotEq,
            mul_f(b, b.loc("half"), b.float(2.0)),
            b.loc("v"),
        ),
    );
    let body = b.match_(
        b.app(rt(b, "Arbitrary.XoshiroNext"), b.rget(b.loc("rr"))),
        [arm(pair_pat(b, "nr", "v"), on_draw)],
    );
    b.lam(&["st"], b.match_(b.loc("st"), [arm(st_pat(b), body)]))
}

pub(super) fn st_len_helper(b: &ExprBuilder) -> Expr {
    let bias_hit = b.bin(
        BinOp::Eq,
        b.app3(
            rt(b, "Helper.arb_st_int"),
            b.loc("st"),
            b.float(1.0),
            b.loc("bias"),
        ),
        b.float(1.0),
    );
    let extreme = b.if_(
        b.app(rt(b, "Helper.arb_st_bool"), b.loc("st")),
        b.loc("lo"),
        b.loc("hi"),
    );
    let uniform = b.app3(
        rt(b, "Helper.arb_st_int"),
        b.loc("st"),
        b.loc("lo"),
        b.loc("hi"),
    );
    let pick = b.match_(
        b.loc("st"),
        [arm(st_pat(b), b.if_(bias_hit, extreme, uniform))],
    );
    b.lam(
        &["st", "lo", "hi"],
        b.if_(
            b.bin(BinOp::Eq, b.loc("lo"), b.loc("hi")),
            b.loc("lo"),
            pick,
        ),
    )
}

pub(super) fn shuffle_helper(b: &ExprBuilder) -> Expr {
    let j_expr = b.app(
        b.global("Int", "truncate"),
        b.app2(
            rt(b, "Helper.arb_st_below"),
            b.loc("st"),
            b.app(b.global("Int", "to_float"), b.add_i(b.var("i"), b.int(1))),
        ),
    );
    let swap_i = b.app3(
        rt(b, "Helper.arb_replace_at"),
        b.var("out"),
        b.var("i"),
        b.loc("vj"),
    );
    let swap_j = b.app3(
        rt(b, "Helper.arb_replace_at"),
        swap_i,
        b.loc("j"),
        b.loc("vi"),
    );
    let step = b.block(
        [
            b.let_("j", j_expr),
            b.let_(
                "vi",
                b.app2(rt(b, "Helper.list_at"), b.var("i"), b.var("out")),
            ),
            b.let_(
                "vj",
                b.app2(rt(b, "Helper.list_at"), b.loc("j"), b.var("out")),
            ),
            b.assign("out", swap_j),
            b.assign("i", b.sub_i(b.var("i"), b.int(1))),
        ],
        b.unit(),
    );
    let init_i = b.sub_i(
        b.app(b.global("List.Core", "length"), b.loc("xs")),
        b.int(1),
    );
    let stmts = [
        b.var_decl("out", b.loc("xs")),
        b.var_decl("i", init_i),
        Stmt::Expr(b.while_(b.bin(BinOp::Ge, b.var("i"), b.int(1)), step)),
    ];
    b.lam(&["st", "xs"], b.block(stmts, b.var("out")))
}

pub(super) fn reserving_helper(b: &ExprBuilder) -> Expr {
    let run_res = b.block(
        [
            b.rset(b.loc("br"), sub_f(b, b.loc("bd"), b.loc("res"))),
            b.let_("s", b.app(b.loc("gen"), b.loc("st"))),
            b.rset(b.loc("br"), add_f(b, b.rget(b.loc("br")), b.loc("res"))),
        ],
        b.loc("s"),
    );
    let with_budget = b.if_(
        b.bin(BinOp::Eq, b.loc("res"), b.float(0.0)),
        b.app(b.loc("gen"), b.loc("st")),
        run_res,
    );
    let check_budget = b.block(
        [b.let_("bd", b.rget(b.loc("br")))],
        b.if_(
            b.bin(BinOp::Gt, add_f(b, b.loc("mc"), b.loc("res")), b.loc("bd")),
            b.none(),
            with_budget,
        ),
    );
    let body = b.match_(b.loc("st"), [arm(st_pat(b), check_budget)]);
    b.lam(
        &["child", "st", "res"],
        b.match_(b.loc("child"), [arm(pair_pat(b, "mc", "gen"), body)]),
    )
}

fn product_step(b: &ExprBuilder) -> Expr {
    let at_expr = b.match_(
        b.loc("order_opt"),
        [
            arm(
                b.some_pat("ord"),
                b.app2(rt(b, "Helper.list_at"), b.var("i"), b.loc("ord")),
            ),
            arm(Pat::Wild, b.var("i")),
        ],
    );
    let cmc = b.match_(b.loc("ch"), [arm(pair_pat(b, "m", "_"), b.loc("m"))]);
    let on_s = b.block(
        [
            b.assign(
                "out",
                b.app3(
                    rt(b, "Helper.arb_replace_at"),
                    b.var("out"),
                    b.loc("at"),
                    b.loc("s"),
                ),
            ),
            b.assign("i", b.add_i(b.var("i"), b.int(1))),
        ],
        b.unit(),
    );
    let resv = b.app3(
        rt(b, "Helper.arb_reserving"),
        b.loc("ch"),
        b.loc("st"),
        b.var("res"),
    );
    let stmts = [
        b.let_("at", at_expr),
        b.let_(
            "ch",
            b.app2(rt(b, "Helper.list_at"), b.loc("at"), b.loc("children")),
        ),
        b.let_("cmc", cmc),
        b.assign("res", sub_f(b, b.var("res"), b.loc("cmc"))),
    ];
    b.block(
        stmts,
        b.match_(
            resv,
            [
                arm(b.some_pat("s"), on_s),
                arm(Pat::Wild, b.assign_e("ok", b.bool_(false))),
            ],
        ),
    )
}

pub(super) fn gen_product_helper(b: &ExprBuilder) -> Expr {
    let init_out = b.gen_strict(b.each("_c", b.loc("children"), b.yield_(b.unit())));
    let cond = b.bin(
        BinOp::And,
        b.var("ok"),
        b.bin(BinOp::Lt, b.var("i"), b.loc("n")),
    );
    let run = b.block(
        [
            b.let_(
                "n",
                b.app(b.global("List.Core", "length"), b.loc("children")),
            ),
            b.var_decl("res", b.loc("res_init")),
            b.var_decl("out", init_out),
            b.var_decl("ok", b.bool_(true)),
            b.var_decl("i", b.int(0)),
            Stmt::Expr(b.while_(cond, product_step(b))),
        ],
        b.if_(b.var("ok"), b.some(b.var("out")), b.none()),
    );
    let body = b.if_(
        b.bin(BinOp::Gt, b.loc("res_init"), b.rget(b.loc("br"))),
        b.none(),
        run,
    );
    b.lam(
        &["children", "st", "order_opt", "res_init"],
        b.match_(b.loc("st"), [arm(st_pat(b), body)]),
    )
}

pub(super) fn gen_attempt_helper(b: &ExprBuilder) -> Expr {
    let log_step = b.block(
        [
            b.assign(
                "x",
                b.app(
                    b.global("Float", "floor"),
                    div_f(b, b.var("x"), b.float(10.0)),
                ),
            ),
            b.assign("d", add_f(b, b.var("d"), b.float(1.0))),
        ],
        b.unit(),
    );
    let floor_log10 = b.block(
        [
            b.var_decl("x", add_f(b, b.loc("att"), b.float(1.0))),
            b.var_decl("d", b.float(0.0)),
            Stmt::Expr(b.while_(b.bin(BinOp::Ge, b.var("x"), b.float(10.0)), log_step)),
        ],
        b.var("d"),
    );
    let rng = b.app3(
        rt(b, "Arbitrary.AttemptRandom"),
        b.loc("h1"),
        b.loc("h2"),
        b.loc("att"),
    );
    let st = b.tuple([
        b.loc("size"),
        b.loc("shrinks"),
        b.loc("bias"),
        b.rmake(b.loc("rng")),
        b.rmake(add_f(b, b.loc("mc"), b.loc("size"))),
    ]);
    let run = b.block(
        [
            b.let_("bias", add_f(b, b.float(2.0), floor_log10)),
            b.let_("rng", rng),
            b.let_("st", st),
        ],
        b.app(b.loc("gen"), b.loc("st")),
    );
    let on_hashed = b.match_(b.loc("hashed"), [arm(pair_pat(b, "h1", "h2"), run)]);
    b.lam(
        &["arb", "hashed", "att", "size", "shrinks"],
        b.match_(b.loc("arb"), [arm(pair_pat(b, "mc", "gen"), on_hashed)]),
    )
}
