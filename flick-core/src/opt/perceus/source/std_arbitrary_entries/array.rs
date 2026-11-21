//! Array shrink-tree helpers (`arb_rebuild_array`, `arb_array_sample`).

use flick_ir::{Arith, BinOp, Num};

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat, Stmt};
use super::rset_e;

fn div2(b: &ExprBuilder, e: Expr) -> Expr {
    let half = b.bin(
        BinOp::Arith(Arith::Div, Num::Float),
        b.app(b.global("Int", "to_float"), e),
        b.float(2.0),
    );
    b.app(b.global("Int", "truncate"), half)
}

fn advance_sr(b: &ExprBuilder) -> Expr {
    let next_st = b.tuple([
        b.add_i(b.loc("idx"), b.int(1)),
        b.int(0),
        b.bool_(false),
        b.loc("rem_opt"),
        b.none(),
    ]);
    rset_e(b, b.loc("sr"), next_st)
}

fn rem_opt_branch(b: &ExprBuilder) -> Expr {
    let next_sr = b.tuple([
        b.loc("idx"),
        b.loc("rem"),
        b.loc("rem_set"),
        div2(b, b.loc("cnt")),
        b.loc("items_opt"),
    ]);
    let taken = b.app2(
        b.global("List.Core", "take"),
        b.sub_i(b.loc("clen"), b.loc("cnt")),
        b.loc("children"),
    );
    let next_shape = b.tuple([
        b.sub_i(b.loc("fixed"), b.loc("cnt")),
        b.sub_i(b.loc("opt_n"), b.loc("cnt")),
        b.loc("rep"),
        b.loc("tail"),
        b.loc("min_n"),
    ]);
    let rebuilt = b.app3(
        rt(b, "Helper.arb_rebuild_array"),
        taken,
        next_shape,
        b.int(0),
    );
    b.block(
        [
            b.let_("cnt", b.loc("rem_opt")),
            b.rset(b.loc("sr"), next_sr),
            b.assign("out", b.some(b.some(rebuilt))),
            b.assign("done", b.bool_(true)),
        ],
        b.unit(),
    )
}

fn init_rem_branch(b: &ExprBuilder) -> Expr {
    let in_rep = b.bin(
        BinOp::And,
        b.bin(BinOp::Ge, b.loc("idx"), b.loc("fixed")),
        b.bin(BinOp::Lt, b.loc("idx"), b.loc("rep_end")),
    );
    let min_rem = b.app2(
        b.global("Int", "min"),
        b.sub_i(b.loc("rep_end"), b.loc("idx")),
        b.sub_i(b.loc("clen"), b.loc("min_n")),
    );
    let next_sr = b.tuple([
        b.loc("idx"),
        b.loc("nrem"),
        b.bool_(true),
        b.loc("rem_opt"),
        b.loc("items_opt"),
    ]);
    b.block(
        [
            b.let_("nrem", b.if_(in_rep, min_rem, b.int(0))),
            b.rset(b.loc("sr"), next_sr),
        ],
        b.unit(),
    )
}

fn rem_branch(b: &ExprBuilder) -> Expr {
    let next_sr = b.tuple([
        b.loc("idx"),
        div2(b, b.loc("cnt")),
        b.bool_(true),
        b.loc("rem_opt"),
        b.loc("items_opt"),
    ]);
    let head = b.app2(
        b.global("List.Core", "take"),
        b.loc("idx"),
        b.loc("children"),
    );
    let tail = b.app2(
        b.global("List.Core", "drop"),
        b.add_i(b.loc("idx"), b.loc("cnt")),
        b.loc("children"),
    );
    let rest = b.app2(b.global("List.Core", "append"), head, tail);
    let next_shape = b.tuple([
        b.loc("fixed"),
        b.loc("opt_n"),
        b.sub_i(b.loc("rep"), b.loc("cnt")),
        b.loc("tail"),
        b.loc("min_n"),
    ]);
    let rebuilt = b.app3(
        rt(b, "Helper.arb_rebuild_array"),
        b.loc("rest"),
        next_shape,
        b.int(0),
    );
    b.block(
        [
            b.let_("cnt", b.loc("rem")),
            b.rset(b.loc("sr"), next_sr),
            b.let_("rest", rest),
            b.assign("out", b.some(b.some(rebuilt))),
            b.assign("done", b.bool_(true)),
        ],
        b.unit(),
    )
}

fn cur_it_expr(b: &ExprBuilder) -> Expr {
    let open_csh = b.match_(
        b.loc("csh"),
        [
            arm(b.some_pat("mkc"), b.some(b.app(b.loc("mkc"), b.unit()))),
            arm(Pat::Wild, b.none()),
        ],
    );
    let lookup_child = b.match_(
        b.app2(rt(b, "Helper.list_at"), b.loc("idx"), b.loc("children")),
        [arm(pair_pat(b, "_cv", "csh"), open_csh)],
    );
    let fallback = b.if_(
        b.bin(BinOp::Ge, b.loc("idx"), b.loc("start")),
        lookup_child,
        b.none(),
    );
    b.match_(
        b.loc("items_opt"),
        [
            arm(b.some_pat("it"), b.some(b.loc("it"))),
            arm(Pat::Wild, fallback),
        ],
    )
}

fn item_shrink_branch(b: &ExprBuilder) -> Expr {
    let replaced = b.app3(
        rt(b, "Helper.arb_replace_at"),
        b.loc("children"),
        b.loc("idx"),
        b.loc("r"),
    );
    let rebuilt = b.app3(
        rt(b, "Helper.arb_rebuild_array"),
        replaced,
        b.loc("shape"),
        b.loc("idx"),
    );
    let out_val = b.match_(
        b.loc("opt_r"),
        [
            arm(b.some_pat("r"), b.some(b.some(rebuilt))),
            arm(Pat::Wild, b.some(b.none())),
        ],
    );
    let keep_sr = b.tuple([
        b.loc("idx"),
        b.loc("rem"),
        b.bool_(true),
        b.loc("rem_opt"),
        b.some(b.loc("itf")),
    ]);
    let on_opt_r = b.block(
        [
            b.rset(b.loc("sr"), keep_sr),
            b.assign("out", out_val),
            b.assign("done", b.bool_(true)),
        ],
        b.unit(),
    );
    let pull_itf = b.match_(
        b.app(b.loc("itf"), b.unit()),
        [
            arm(b.some_pat("opt_r"), on_opt_r),
            arm(Pat::Wild, advance_sr(b)),
        ],
    );
    let body = b.match_(
        b.loc("cur_it"),
        [
            arm(b.some_pat("itf"), pull_itf),
            arm(Pat::Wild, advance_sr(b)),
        ],
    );
    b.block([b.let_("cur_it", cur_it_expr(b))], body)
}

pub(super) fn rebuild_array_helper(b: &ExprBuilder) -> Expr {
    let shp_pat = b.tup_pat(&["fixed", "opt_n", "rep", "tail", "min_n"]);
    let st_pat = b.tup_pat(&["idx", "rem", "rem_set", "rem_opt", "items_opt"]);
    let step_rem = b.if_(
        b.bin(BinOp::Gt, b.loc("rem"), b.int(0)),
        rem_branch(b),
        item_shrink_branch(b),
    );
    let step_set = b.if_(b.not(b.loc("rem_set")), init_rem_branch(b), step_rem);
    let step_idx = b.if_(
        b.bin(BinOp::Ge, b.loc("idx"), b.loc("clen")),
        b.assign_e("done", b.bool_(true)),
        step_set,
    );
    let step = b.if_(
        b.bin(BinOp::Gt, b.loc("rem_opt"), b.int(0)),
        rem_opt_branch(b),
        step_idx,
    );
    let pull_loop = b.while_(
        b.not(b.var("done")),
        b.match_(b.rget(b.loc("sr")), [arm(st_pat, step)]),
    );
    let pull_fn = b.lam(
        &["_u"],
        b.block(
            [
                b.var_decl("out", b.none()),
                b.var_decl("done", b.bool_(false)),
                Stmt::Expr(pull_loop),
            ],
            b.var("out"),
        ),
    );
    let no_rep_tail = b.bin(
        BinOp::And,
        b.bin(BinOp::Eq, b.loc("rep"), b.int(0)),
        b.bin(BinOp::Eq, b.loc("tail"), b.int(0)),
    );
    let init_ro = b.if_(
        no_rep_tail,
        b.app2(
            b.global("Int", "min"),
            b.loc("opt_n"),
            b.sub_i(b.loc("clen"), b.loc("min_n")),
        ),
        b.int(0),
    );
    let init_sr = b.rmake(b.tuple([
        b.int(0),
        b.int(0),
        b.bool_(false),
        b.loc("init_ro"),
        b.none(),
    ]));
    let vals = b.gen_strict(b.each(
        "c",
        b.loc("children"),
        b.match_(
            b.loc("c"),
            [arm(pair_pat(b, "v", "_"), b.yield_(b.loc("v")))],
        ),
    ));
    let body = b.block(
        [
            b.let_(
                "clen",
                b.app(b.global("List.Core", "length"), b.loc("children")),
            ),
            b.let_("init_ro", init_ro),
            b.let_("rep_end", b.add_i(b.loc("fixed"), b.loc("rep"))),
            b.let_("sr", init_sr),
            b.let_("vals", vals),
        ],
        b.tuple([b.loc("vals"), b.some(pull_fn)]),
    );
    b.lam(
        &["children", "shape", "start"],
        b.match_(b.loc("shape"), [arm(shp_pat, body)]),
    )
}

pub(super) fn array_sample_helper(b: &ExprBuilder) -> Expr {
    let shp_pat = b.tup_pat(&["_f", "opt_n", "rep", "_t", "min_n"]);
    let vals = b.gen_strict(b.each(
        "c",
        b.loc("children"),
        b.match_(
            b.loc("c"),
            [arm(pair_pat(b, "v", "_"), b.yield_(b.loc("v")))],
        ),
    ));
    let rets = b.gen_strict(b.each(
        "c",
        b.loc("children"),
        b.yield_(b.app(rt(b, "Helper.arb_retain"), b.loc("c"))),
    ));
    let rebuilt = b.app3(
        rt(b, "Helper.arb_rebuild_array"),
        b.loc("rets"),
        b.loc("shape"),
        b.int(0),
    );
    let lazy_body = b.block(
        [b.let_("rets", rets)],
        b.match_(rebuilt, [arm(pair_pat(b, "_v", "sh"), b.loc("sh"))]),
    );
    let no_shrink_len = b.bin(
        BinOp::Or,
        b.bin(
            BinOp::Le,
            b.app(b.global("List.Core", "length"), b.loc("children")),
            b.loc("min_n"),
        ),
        b.bin(
            BinOp::And,
            b.bin(BinOp::Eq, b.loc("rep"), b.int(0)),
            b.bin(BinOp::Eq, b.loc("opt_n"), b.int(0)),
        ),
    );
    let prod_only = b.app2(
        rt(b, "Helper.arb_product_sample"),
        b.loc("children"),
        b.lam(&["vs"], b.loc("vs")),
    );
    let lazy = b.app2(
        rt(b, "Helper.arb_lazy_shrinks"),
        b.loc("vals"),
        b.lam(&["_u"], lazy_body),
    );
    let shrunk = b.match_(
        b.loc("shape"),
        [arm(shp_pat, b.if_(no_shrink_len, prod_only, lazy))],
    );
    let body = b.if_(
        b.not(b.loc("shrinks")),
        b.tuple([b.loc("vals"), b.none()]),
        shrunk,
    );
    b.lam(
        &["children", "shape", "shrinks"],
        b.block([b.let_("vals", vals)], body),
    )
}
