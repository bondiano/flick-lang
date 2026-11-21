//! Product shrink-tree and list-replacement helpers for `Arbitrary`.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat, Stmt};

pub(super) fn replace_at_helper(b: &ExprBuilder) -> Expr {
    let cur = b.if_(
        b.bin(BinOp::Eq, b.var("i"), b.loc("idx")),
        b.loc("val"),
        b.loc("x"),
    );
    let step = b.block(
        [
            b.let_("cur", cur),
            b.assign("i", b.add_i(b.var("i"), b.int(1))),
        ],
        b.yield_(b.loc("cur")),
    );
    let body = b.block(
        [b.var_decl("i", b.int(0))],
        b.gen_strict(b.each("x", b.loc("xs"), step)),
    );
    b.lam(&["xs", "idx", "val"], body)
}

pub(super) fn product_sample_helper(b: &ExprBuilder) -> Expr {
    let mark_sh = b.match_(
        b.loc("sh"),
        [
            arm(b.some_pat("_"), b.assign_e("any_sh", b.bool_(true))),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let each_child = b.match_(
        b.loc("c"),
        [arm(
            pair_pat(b, "v", "sh"),
            b.block([Stmt::Expr(mark_sh)], b.yield_(b.loc("v"))),
        )],
    );
    let vals = b.gen_strict(b.each("c", b.loc("children"), each_child));
    let rets = b.gen_strict(b.each(
        "c",
        b.loc("children"),
        b.yield_(b.app(rt(b, "Helper.arb_retain"), b.loc("c"))),
    ));
    let rebuilt = b.app3(
        rt(b, "Helper.arb_rebuild_product"),
        b.loc("rets"),
        b.int(0),
        b.loc("mk"),
    );
    let lazy_body = b.block(
        [b.let_("rets", rets)],
        b.match_(rebuilt, [arm(pair_pat(b, "_v", "sh"), b.loc("sh"))]),
    );
    let shrunk = b.app2(
        rt(b, "Helper.arb_lazy_shrinks"),
        b.loc("val"),
        b.lam(&["_u"], lazy_body),
    );
    let stmts = [
        b.var_decl("any_sh", b.bool_(false)),
        b.let_("vals", vals),
        b.let_("val", b.app(b.loc("mk"), b.loc("vals"))),
    ];
    let tail = b.if_(
        b.not(b.var("any_sh")),
        b.tuple([b.loc("val"), b.none()]),
        shrunk,
    );
    b.lam(&["children", "mk"], b.block(stmts, tail))
}

pub(super) fn rebuild_product_helper(b: &ExprBuilder) -> Expr {
    let replaced = b.app3(
        rt(b, "Helper.arb_replace_at"),
        b.loc("rets"),
        b.loc("idx"),
        b.loc("r"),
    );
    let rebuild_r = b.lam(
        &["r"],
        b.app3(
            rt(b, "Helper.arb_rebuild_product"),
            replaced,
            b.loc("idx"),
            b.loc("mk"),
        ),
    );
    let on_mk = b.yield_(b.app2(
        rt(b, "Helper.arb_map_retained"),
        b.app(b.loc("mk_cur"), b.unit()),
        rebuild_r,
    ));
    let on_sh = b.match_(
        b.loc("sh_opt"),
        [arm(b.some_pat("mk_cur"), on_mk), arm(Pat::Wild, b.unit())],
    );
    let step = b.block(
        [
            b.let_("idx", b.var("i")),
            b.assign("i", b.add_i(b.var("i"), b.int(1))),
            b.let_(
                "ch",
                b.app2(rt(b, "Helper.list_at"), b.loc("idx"), b.loc("rets")),
            ),
        ],
        b.match_(b.loc("ch"), [arm(pair_pat(b, "_v", "sh_opt"), on_sh)]),
    );
    let pulls = b.gen_strict(b.while_(b.bin(BinOp::Lt, b.var("i"), b.loc("len")), step));
    let vals = b.gen_strict(b.each(
        "r",
        b.loc("rets"),
        b.match_(
            b.loc("r"),
            [arm(pair_pat(b, "v", "_"), b.yield_(b.loc("v")))],
        ),
    ));
    let sh = b.if_(
        b.bin(
            BinOp::Gt,
            b.app(b.global("List.Core", "length"), b.loc("pulls")),
            b.int(0),
        ),
        b.some(b.app(rt(b, "Helper.arb_concat_pulls"), b.loc("pulls"))),
        b.none(),
    );
    let stmts = [
        b.let_("len", b.app(b.global("List.Core", "length"), b.loc("rets"))),
        b.var_decl("i", b.loc("start")),
        b.let_("pulls", pulls),
        b.let_("vals", vals),
        b.let_("sh", sh),
    ];
    b.lam(
        &["rets", "start", "mk"],
        b.block(
            stmts,
            b.tuple([b.app(b.loc("mk"), b.loc("vals")), b.loc("sh")]),
        ),
    )
}
