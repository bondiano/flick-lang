//! Generator templates (`flickSchema`, `flickArray`, `all`) for `Arbitrary`.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt, str_lit};
use super::super::{Expr, Pat, Stmt};
use super::{add_f, div_f, mul_f, st_pat};

fn vshrink_fun(b: &ExprBuilder) -> Stmt {
    let check_cand = b.match_(
        b.app(b.loc("val_fn"), b.loc("c")),
        [
            arm(
                b.ok_pat("okv"),
                b.some(b.app(b.loc("vshrink"), b.loc("okv"))),
            ),
            arm(Pat::Wild, b.none()),
        ],
    );
    let pull = b.some(b.app2(
        rt(b, "Helper.arb_pull_each"),
        b.app(b.loc("sh_fn"), b.loc("v")),
        b.lam(&["c"], check_cand),
    ));
    let body = b.app2(
        rt(b, "Helper.arb_lazy_shrinks"),
        b.loc("v"),
        b.lam(&["_u"], pull),
    );
    Stmt::Fun {
        name: b.name("vshrink"),
        rec: true,
        dicts: vec![],
        value: b.lam(&["v"], body),
    }
}

pub(super) fn schema_with_template(b: &ExprBuilder) -> Expr {
    let shrink_s = b.match_(
        b.loc("s"),
        [arm(
            pair_pat(b, "v", "_"),
            b.some(b.app(b.loc("vshrink"), b.loc("v"))),
        )],
    );
    let on_some = b.match_(
        b.loc("st"),
        [arm(
            st_pat(b),
            b.if_(b.not(b.loc("shk")), b.some(b.loc("s")), shrink_s),
        )],
    );
    let gen_body = b.match_(
        b.app(b.loc("gen"), b.loc("st")),
        [arm(b.some_pat("s"), on_some), arm(Pat::Wild, b.none())],
    );
    let wrapped = b.block(
        [
            b.let_("val_fn", b.app(rt(b, "Helper.schema_val"), b.loc("sch"))),
            vshrink_fun(b),
        ],
        b.tuple([b.loc("mc"), b.lam(&["st"], gen_body)]),
    );
    let with_shrink = b.match_(b.loc("comp"), [arm(pair_pat(b, "mc", "gen"), wrapped)]);
    let comp_let = b.let_(
        "comp",
        b.app(rt(b, "Helper.arb_compile_schema"), b.loc("sch")),
    );
    let body = b.match_(
        b.field(b.loc("opts"), "shrink"),
        [
            arm(b.some_pat("sh_fn"), with_shrink),
            arm(Pat::Wild, b.loc("comp")),
        ],
    );
    b.lam(&["opts", "sch"], b.block([comp_let], body))
}

fn array_order_expr(b: &ExprBuilder) -> Expr {
    let idx_step = b.block(
        [
            b.let_("c", b.var("j")),
            b.assign("j", b.add_i(b.var("j"), b.int(1))),
        ],
        b.yield_(b.loc("c")),
    );
    let idxs = b.gen_strict(b.while_(b.bin(BinOp::Lt, b.var("j"), b.loc("len")), idx_step));
    let shuffled = b.block(
        [b.var_decl("j", b.int(0)), b.let_("idxs", idxs)],
        b.some(b.app2(rt(b, "Helper.arb_shuffle"), b.loc("st"), b.loc("idxs"))),
    );
    b.if_(b.bin(BinOp::Ge, b.loc("len"), b.int(2)), shuffled, b.none())
}

fn array_gen_in_range(b: &ExprBuilder) -> Expr {
    let chs_step = b.block(
        [b.assign("k", b.add_i(b.var("k"), b.int(1)))],
        b.yield_(b.loc("item")),
    );
    let chs = b.gen_strict(b.while_(b.bin(BinOp::Lt, b.var("k"), b.loc("len")), chs_step));
    let grown_st = b.tuple([
        b.loc("len_f"),
        b.loc("shk"),
        b.loc("bias"),
        b.loc("rr"),
        b.loc("br"),
    ]);
    let ist = b.if_(
        b.bin(BinOp::Lt, b.loc("sz"), b.loc("len_f")),
        grown_st,
        b.loc("st"),
    );
    let res = b.if_(
        b.bin(BinOp::Gt, b.loc("len"), b.int(0)),
        mul_f(b, b.loc("len_f"), b.loc("imc")),
        b.float(0.0),
    );
    let shape = b.tuple([
        b.int(0),
        b.int(0),
        b.loc("len"),
        b.int(0),
        b.app(b.global("Int", "truncate"), b.loc("lo")),
    ]);
    let on_sams = b.some(b.app3(
        rt(b, "Helper.arb_array_sample"),
        b.loc("sams"),
        shape,
        b.loc("shk"),
    ));
    let prod = b.app4(
        rt(b, "Helper.arb_gen_product"),
        b.loc("chs"),
        b.loc("ist"),
        b.loc("order_o"),
        b.loc("res"),
    );
    let stmts = [
        b.let_(
            "len_f",
            b.app3(
                rt(b, "Helper.arb_st_len"),
                b.loc("st"),
                b.loc("lo"),
                b.loc("up"),
            ),
        ),
        b.let_("len", b.app(b.global("Int", "truncate"), b.loc("len_f"))),
        b.var_decl("k", b.int(0)),
        b.let_("chs", chs),
        b.let_("order_o", array_order_expr(b)),
        b.let_("ist", ist),
        b.let_("res", res),
    ];
    b.block(
        stmts,
        b.match_(
            prod,
            [arm(b.some_pat("sams"), on_sams), arm(Pat::Wild, b.none())],
        ),
    )
}

fn array_gen_body(b: &ExprBuilder) -> Expr {
    let aff = b.if_(
        b.bin(BinOp::Gt, b.loc("imc"), b.float(0.0)),
        b.app(
            b.global("Float", "floor"),
            div_f(b, b.rget(b.loc("br")), b.loc("imc")),
        ),
        b.loc("hi"),
    );
    let m1 = b.if_(
        b.bin(BinOp::Gt, b.loc("lo"), b.loc("sz")),
        b.loc("lo"),
        b.loc("sz"),
    );
    let m2 = b.if_(
        b.bin(BinOp::Lt, b.loc("m1"), b.loc("aff")),
        b.loc("m1"),
        b.loc("aff"),
    );
    let up = b.if_(
        b.bin(BinOp::Lt, b.loc("hi"), b.loc("m2")),
        b.loc("hi"),
        b.loc("m2"),
    );
    let inner = b.block(
        [
            b.let_("aff", aff),
            b.let_("m1", m1),
            b.let_("m2", m2),
            b.let_("up", up),
        ],
        b.if_(
            b.bin(BinOp::Lt, b.loc("up"), b.loc("lo")),
            b.none(),
            array_gen_in_range(b),
        ),
    );
    b.lam(&["st"], b.match_(b.loc("st"), [arm(st_pat(b), inner)]))
}

pub(super) fn array_gen_template(b: &ExprBuilder) -> Expr {
    let lo = b.match_(
        b.field(b.loc("opts"), "minLength"),
        [
            arm(
                b.some_pat("n"),
                b.app(b.global("Int", "to_float"), b.loc("n")),
            ),
            arm(Pat::Wild, b.float(0.0)),
        ],
    );
    let hi = b.match_(
        b.field(b.loc("opts"), "maxLength"),
        [
            arm(
                b.some_pat("n"),
                b.app(b.global("Int", "to_float"), b.loc("n")),
            ),
            arm(Pat::Wild, b.float(4_294_967_295.0)),
        ],
    );
    let bad_hi = b.bin(
        BinOp::Or,
        b.bin(BinOp::Lt, b.loc("hi"), b.loc("lo")),
        b.bin(BinOp::Gt, b.loc("hi"), b.float(4_294_967_295.0)),
    );
    let bad_range = b.bin(
        BinOp::Or,
        b.bin(BinOp::Lt, b.loc("lo"), b.float(0.0)),
        bad_hi,
    );
    let msg = "RangeError: Arbitrary.array: expected 0 <= minLength <= maxLength <= 4294967295 with integer lengths";
    let check_range = Stmt::Expr(b.if_(
        bad_range,
        b.app(rt(b, "Schema.Defect"), str_lit(b, msg)),
        b.unit(),
    ));
    let imc = b.match_(b.loc("item"), [arm(pair_pat(b, "m", "_"), b.loc("m"))]);
    let mc = b.if_(
        b.bin(BinOp::Eq, b.loc("lo"), b.float(0.0)),
        b.float(0.0),
        mul_f(b, b.loc("lo"), b.loc("imc")),
    );
    let stmts = [
        b.let_("lo", lo),
        b.let_("hi", hi),
        check_range,
        b.let_("imc", imc),
        b.let_("mc", mc),
    ];
    b.lam(
        &["opts", "item"],
        b.block(stmts, b.tuple([b.loc("mc"), array_gen_body(b)])),
    )
}

pub(super) fn all_gen_template(b: &ExprBuilder) -> Expr {
    let idx_body = b.block(
        [
            b.let_("c", b.var("i")),
            b.assign("i", b.add_i(b.var("i"), b.int(1))),
            b.assign("mc", add_f(b, b.var("mc"), b.loc("mmc"))),
        ],
        b.yield_(b.loc("c")),
    );
    let idxs = b.gen_strict(b.each(
        "m",
        b.loc("members"),
        b.match_(b.loc("m"), [arm(pair_pat(b, "mmc", "_"), idx_body)]),
    ));
    let ord_o = b.if_(
        b.bin(
            BinOp::Ge,
            b.app(b.global("List.Core", "length"), b.loc("idxs")),
            b.int(2),
        ),
        b.some(b.app2(rt(b, "Helper.arb_shuffle"), b.loc("st"), b.loc("idxs"))),
        b.none(),
    );
    let on_sams = b.some(b.app2(
        rt(b, "Helper.arb_product_sample"),
        b.loc("sams"),
        b.lam(&["vs"], b.loc("vs")),
    ));
    let prod = b.app4(
        rt(b, "Helper.arb_gen_product"),
        b.loc("members"),
        b.loc("st"),
        b.loc("ord_o"),
        b.loc("total_mc"),
    );
    let gen_fn = b.lam(
        &["st"],
        b.block(
            [b.let_("ord_o", ord_o)],
            b.match_(
                prod,
                [arm(b.some_pat("sams"), on_sams), arm(Pat::Wild, b.none())],
            ),
        ),
    );
    let stmts = [
        b.var_decl("mc", b.float(0.0)),
        b.var_decl("i", b.int(0)),
        b.let_("idxs", idxs),
        b.let_("total_mc", b.var("mc")),
    ];
    b.lam(
        &["members"],
        b.block(stmts, b.tuple([b.loc("total_mc"), gen_fn])),
    )
}
