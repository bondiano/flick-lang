//! Compound graph node generators (`arb_gen_union`, `arb_gen_samples`, `arb_gen_repeated`).

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat, Stmt};
use super::{add_f, mul_f, st_pat, sub_f};

fn node_mc(b: &ExprBuilder, id: Expr) -> Expr {
    b.match_(
        b.app2(rt(b, "Helper.list_at"), id, b.loc("tbl")),
        [arm(
            b.tup_pat(&["mc", "_", "_", "_", "_", "_"]),
            b.loc("mc"),
        )],
    )
}

fn union_with_fallback(b: &ExprBuilder) -> Expr {
    let cst = b.tuple([
        b.loc("sz"),
        b.loc("shk"),
        b.loc("bias"),
        b.rmake(b.loc("saved_rng")),
        b.rmake(b.loc("fb_cost")),
    ]);
    let eval_fb = b.block(
        [b.rset(b.loc("pulled"), b.bool_(true)), b.let_("cst", cst)],
        b.some(b.app3(
            rt(b, "Helper.arb_eval_node"),
            b.loc("tbl"),
            b.loc("fb_id"),
            b.loc("cst"),
        )),
    );
    let fb_pull = b.lam(&["_u"], b.if_(b.rget(b.loc("pulled")), b.none(), eval_fb));
    let merged_sh = b.match_(
        b.loc("ssh"),
        [
            arm(
                b.some_pat("sp"),
                b.app(
                    rt(b, "Helper.arb_concat_pulls"),
                    b.list([b.loc("fb_pull"), b.loc("sp")]),
                ),
            ),
            arm(Pat::Wild, b.loc("fb_pull")),
        ],
    );
    let wrap_s = b.match_(
        b.loc("s"),
        [arm(
            pair_pat(b, "sv", "ssh"),
            b.some(b.tuple([b.loc("sv"), b.some(merged_sh)])),
        )],
    );
    b.block(
        [
            b.let_("saved_rng", b.rget(b.loc("rr"))),
            b.let_("pulled", b.rmake(b.bool_(false))),
            b.let_("fb_id", b.var("fb")),
            b.let_("fb_cost", b.var("fb_mc")),
            b.let_("fb_pull", fb_pull),
        ],
        wrap_s,
    )
}

fn union_shrunk_some(b: &ExprBuilder) -> Expr {
    let update_fb = b.if_(
        b.bin(BinOp::Lt, b.loc("mmc"), b.var("fb_mc")),
        b.block(
            [b.assign("fb", b.loc("m")), b.assign("fb_mc", b.loc("mmc"))],
            b.unit(),
        ),
        b.unit(),
    );
    let find_fb = b.each(
        "m",
        b.loc("mids"),
        b.block([b.let_("mmc", node_mc(b, b.loc("m")))], update_fb),
    );
    let stmts = [
        b.var_decl(
            "fb",
            b.app2(rt(b, "Helper.list_at"), b.int(0), b.loc("mids")),
        ),
        b.var_decl("fb_mc", node_mc(b, b.var("fb"))),
        Stmt::Expr(find_fb),
    ];
    let tail = b.if_(
        b.bin(BinOp::Ge, b.var("fb_mc"), b.loc("sel_mc")),
        b.some(b.loc("s")),
        union_with_fallback(b),
    );
    b.block(stmts, tail)
}

pub(super) fn gen_union_helper(b: &ExprBuilder) -> Expr {
    let elig = b.gen_strict(b.each(
        "m",
        b.loc("mids"),
        b.if_(
            b.bin(BinOp::Le, node_mc(b, b.loc("m")), b.loc("bd")),
            b.yield_(b.loc("m")),
            b.unit(),
        ),
    ));
    let sel = b.app2(
        rt(b, "Helper.list_at"),
        b.app2(rt(b, "Helper.arb_st_index"), b.loc("st"), b.loc("elen")),
        b.loc("elig"),
    );
    let shrunk = b.match_(
        b.loc("s_opt"),
        [
            arm(b.some_pat("s"), union_shrunk_some(b)),
            arm(Pat::Wild, b.none()),
        ],
    );
    let pick = b.block(
        [
            b.let_("sel", sel),
            b.let_("sel_mc", node_mc(b, b.loc("sel"))),
            b.let_(
                "s_opt",
                b.app3(
                    rt(b, "Helper.arb_eval_node"),
                    b.loc("tbl"),
                    b.loc("sel"),
                    b.loc("st"),
                ),
            ),
        ],
        b.if_(b.not(b.loc("shk")), b.loc("s_opt"), shrunk),
    );
    let body = b.block(
        [
            b.let_("bd", b.rget(b.loc("br"))),
            b.let_("elig", elig),
            b.let_(
                "elen",
                b.app(b.global("List.Core", "length"), b.loc("elig")),
            ),
        ],
        b.if_(b.bin(BinOp::Eq, b.loc("elen"), b.int(0)), b.none(), pick),
    );
    b.lam(
        &["tbl", "mids", "st"],
        b.match_(b.loc("st"), [arm(st_pat(b), body)]),
    )
}

fn samples_order_expr(b: &ExprBuilder) -> Expr {
    let take_shuf = b.block(
        [
            b.let_(
                "nxt",
                b.app2(rt(b, "Helper.list_at"), b.var("j"), b.loc("shuf")),
            ),
            b.assign("j", b.add_i(b.var("j"), b.int(1))),
        ],
        b.yield_(b.loc("nxt")),
    );
    let per_cid = b.block(
        [
            b.let_("cur_k", b.var("k")),
            b.assign("k", b.add_i(b.var("k"), b.int(1))),
        ],
        b.if_(b.loc("mr"), take_shuf, b.yield_(b.loc("cur_k"))),
    );
    let order = b.gen_strict(b.each(
        "cid",
        b.loc("cids"),
        b.match_(
            b.app2(rt(b, "Helper.list_at"), b.loc("cid"), b.loc("tbl")),
            [arm(b.tup_pat(&["_", "_", "mr", "_", "_", "_"]), per_cid)],
        ),
    ));
    let shuffled = b.block(
        [
            b.let_(
                "shuf",
                b.app2(rt(b, "Helper.arb_shuffle"), b.loc("st"), b.loc("recs")),
            ),
            b.var_decl("k", b.int(0)),
            b.var_decl("j", b.int(0)),
        ],
        b.some(order),
    );
    let multi = b.bin(
        BinOp::Gt,
        b.app(b.global("List.Core", "length"), b.loc("recs")),
        b.int(1),
    );
    b.if_(multi, shuffled, b.none())
}

pub(super) fn gen_samples_helper(b: &ExprBuilder) -> Expr {
    let rec_step = b.block(
        [
            b.let_("ci", b.var("idx")),
            b.assign("idx", b.add_i(b.var("idx"), b.int(1))),
            b.assign("res", add_f(b, b.var("res"), b.loc("mc"))),
        ],
        b.if_(b.loc("mr"), b.yield_(b.loc("ci")), b.unit()),
    );
    let recs = b.gen_strict(b.each(
        "cid",
        b.loc("cids"),
        b.match_(
            b.app2(rt(b, "Helper.list_at"), b.loc("cid"), b.loc("tbl")),
            [arm(b.tup_pat(&["mc", "_", "mr", "_", "_", "_"]), rec_step)],
        ),
    ));
    let arb_fn = b.lam(
        &["s2"],
        b.app3(
            rt(b, "Helper.arb_eval_node"),
            b.loc("tbl"),
            b.loc("cid"),
            b.loc("s2"),
        ),
    );
    let arbs = b.gen_strict(b.each(
        "cid",
        b.loc("cids"),
        b.match_(
            b.app2(rt(b, "Helper.list_at"), b.loc("cid"), b.loc("tbl")),
            [arm(
                b.tup_pat(&["mc", "_", "_", "_", "_", "_"]),
                b.yield_(b.tuple([b.loc("mc"), arb_fn])),
            )],
        ),
    ));
    let stmts = [
        b.var_decl("res", b.float(0.0)),
        b.var_decl("idx", b.int(0)),
        b.let_("recs", recs),
        b.let_("order_o", samples_order_expr(b)),
        b.let_("arbs", arbs),
    ];
    let tail = b.app4(
        rt(b, "Helper.arb_gen_product"),
        b.loc("arbs"),
        b.loc("st"),
        b.loc("order_o"),
        b.var("res"),
    );
    b.lam(&["tbl", "cids", "st"], b.block(stmts, tail))
}

fn repeated_order_expr(b: &ExprBuilder) -> Expr {
    let step = b.block(
        [
            b.let_("c", b.var("k")),
            b.assign("k", b.add_i(b.var("k"), b.int(1))),
        ],
        b.yield_(b.loc("c")),
    );
    let idxs = b.gen_strict(b.while_(b.bin(BinOp::Lt, b.var("k"), b.loc("cnt")), step));
    let shuf = b.block(
        [b.var_decl("k", b.int(0)), b.let_("idxs", idxs)],
        b.some(b.app2(rt(b, "Helper.arb_shuffle"), b.loc("st"), b.loc("idxs"))),
    );
    b.if_(b.loc("hmr"), shuf, b.none())
}

fn repeated_loop(b: &ExprBuilder) -> Expr {
    let at_expr = b.match_(
        b.loc("order_o"),
        [
            arm(
                b.some_pat("ord"),
                b.app2(rt(b, "Helper.list_at"), b.var("i"), b.loc("ord")),
            ),
            arm(Pat::Wild, b.var("i")),
        ],
    );
    let store_v = b.block(
        [
            b.let_("at", at_expr),
            b.assign(
                "out",
                b.app3(
                    rt(b, "Helper.arb_replace_at"),
                    b.var("out"),
                    b.loc("at"),
                    b.loc("v"),
                ),
            ),
            b.assign("i", b.add_i(b.var("i"), b.int(1))),
        ],
        b.unit(),
    );
    let on_s = b.match_(b.loc("s"), [arm(pair_pat(b, "v", "_"), store_v)]);
    let resv = b.app3(
        rt(b, "Helper.arb_reserving"),
        b.loc("harb"),
        b.loc("st"),
        b.var("res"),
    );
    let body = b.block(
        [b.assign("res", sub_f(b, b.var("res"), b.loc("hmc")))],
        b.match_(
            resv,
            [
                arm(b.some_pat("s"), on_s),
                arm(Pat::Wild, b.assign_e("ok", b.bool_(false))),
            ],
        ),
    );
    let cond = b.bin(
        BinOp::And,
        b.var("ok"),
        b.bin(BinOp::Lt, b.var("i"), b.loc("cnt")),
    );
    b.while_(cond, body)
}

pub(super) fn gen_repeated_helper(b: &ExprBuilder) -> Expr {
    let harb = b.tuple([
        b.loc("hmc"),
        b.lam(
            &["s2"],
            b.app3(
                rt(b, "Helper.arb_eval_node"),
                b.loc("tbl"),
                b.loc("hid"),
                b.loc("s2"),
            ),
        ),
    ]);
    let fill_step = b.block(
        [b.assign("m", b.add_i(b.var("m"), b.int(1)))],
        b.yield_(b.unit()),
    );
    let init_out = b.block(
        [b.var_decl("m", b.int(0))],
        b.gen_strict(b.while_(b.bin(BinOp::Lt, b.var("m"), b.loc("cnt")), fill_step)),
    );
    let stmts = [
        b.let_("order_o", repeated_order_expr(b)),
        b.let_("harb", harb),
        b.var_decl(
            "res",
            mul_f(
                b,
                b.app(b.global("Int", "to_float"), b.loc("cnt")),
                b.loc("hmc"),
            ),
        ),
        b.var_decl("i", b.int(0)),
        b.var_decl("ok", b.bool_(true)),
        b.var_decl("out", init_out),
        Stmt::Expr(repeated_loop(b)),
    ];
    let body = b.block(
        stmts,
        b.if_(
            b.var("ok"),
            b.some(b.tuple([b.var("out"), b.none()])),
            b.none(),
        ),
    );
    b.lam(
        &["tbl", "hid", "cnt", "st"],
        b.match_(
            b.app2(rt(b, "Helper.list_at"), b.loc("hid"), b.loc("tbl")),
            [arm(b.tup_pat(&["hmc", "_", "hmr", "_", "_", "_"]), body)],
        ),
    )
}
