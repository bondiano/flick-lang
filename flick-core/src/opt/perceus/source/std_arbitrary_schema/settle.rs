//! Graph recursion analysis and Bellman-Ford min-cost fixpoint (`arb_settle_graph`).

use flick_ir::{Arith, BinOp, Num};

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{rt, str_lit};
use super::super::{Expr, Pat, Stmt};
use super::{add_f, arb_err, float_inf, mul_f};

fn cost_arrays_arm(b: &ExprBuilder) -> Expr {
    let inf = float_inf(b);
    let sum_eids = b.each(
        "e",
        b.loc("eids"),
        b.assign_e(
            "rc",
            add_f(
                b,
                b.var("rc"),
                b.app2(rt(b, "Helper.list_at"), b.loc("e"), b.var("mcs")),
            ),
        ),
    );
    let req = b.app(
        b.global("Int", "to_float"),
        b.app(b.global("List.Core", "length"), b.loc("eids")),
    );
    let diff = b.bin(
        BinOp::Arith(Arith::Sub, Num::Float),
        b.loc("amin"),
        b.loc("req"),
    );
    let mrep = b.if_(
        b.bin(BinOp::Gt, diff.clone(), b.float(0.0)),
        diff,
        b.float(0.0),
    );
    let with_hid = add_f(
        b,
        b.var("rc"),
        mul_f(
            b,
            b.loc("mrep"),
            b.app2(rt(b, "Helper.list_at"), b.loc("hid"), b.var("mcs")),
        ),
    );
    let rep_cost = b.match_(
        b.loc("rid_o"),
        [arm(b.some_pat("hid"), with_hid), arm(Pat::Wild, inf)],
    );
    b.block(
        [
            b.var_decl("rc", b.float(0.0)),
            Stmt::Expr(sum_eids),
            b.let_("req", req),
            b.let_("mrep", mrep),
        ],
        b.if_(
            b.bin(BinOp::Gt, b.loc("mrep"), b.float(0.0)),
            rep_cost,
            b.var("rc"),
        ),
    )
}

fn node_pat(b: &ExprBuilder, c: &str, names: &[&str]) -> Pat {
    let args = names
        .iter()
        .map(|n| {
            if *n == "_" {
                Pat::Wild
            } else {
                Pat::Bind(b.name(n))
            }
        })
        .collect();
    Pat::Con(ExprBuilder::con("Arbitrary", "$Node", c, names.len()), args)
}

fn cost_objects_arm(b: &ExprBuilder) -> Expr {
    b.block(
        [
            b.var_decl("s", b.float(0.0)),
            Stmt::Expr(b.each(
                "p",
                b.loc("pids"),
                b.assign_e(
                    "s",
                    add_f(
                        b,
                        b.var("s"),
                        b.app2(rt(b, "Helper.list_at"), b.loc("p"), b.var("mcs")),
                    ),
                ),
            )),
        ],
        b.var("s"),
    )
}

fn cost_union_arm(b: &ExprBuilder) -> Expr {
    let union_step = b.block(
        [b.let_(
            "c",
            b.app2(rt(b, "Helper.list_at"), b.loc("m"), b.var("mcs")),
        )],
        b.if_(
            b.bin(BinOp::Lt, b.loc("c"), b.var("best")),
            b.assign_e("best", b.loc("c")),
            b.unit(),
        ),
    );
    b.block(
        [
            b.var_decl("best", float_inf(b)),
            Stmt::Expr(b.each("m", b.loc("mids"), union_step)),
        ],
        b.var("best"),
    )
}

fn cost_suspend_arm(b: &ExprBuilder) -> Expr {
    let inf = float_inf(b);
    let suspend_finite = b.if_(
        b.app2(rt(b, "Helper.list_at"), b.var("i"), b.var("recs")),
        add_f(b, b.loc("bc"), b.float(1.0)),
        b.loc("bc"),
    );
    b.block(
        [b.let_(
            "bc",
            b.app2(rt(b, "Helper.list_at"), b.loc("bid"), b.var("mcs")),
        )],
        b.if_(
            b.bin(BinOp::Eq, b.loc("bc"), inf.clone()),
            inf,
            suspend_finite,
        ),
    )
}

fn compute_cost_expr(b: &ExprBuilder) -> Expr {
    let arms = [
        arm(node_pat(b, "$Never", &[]), float_inf(b)),
        arm(node_pat(b, "$Literal", &["_"]), b.float(0.0)),
        arm(node_pat(b, "$Bool", &[]), b.float(0.0)),
        arm(node_pat(b, "$Number", &["_", "_", "_"]), b.float(0.0)),
        arm(node_pat(b, "$BigInt", &["_", "_", "_"]), b.float(0.0)),
        arm(node_pat(b, "$Decimal", &[]), b.float(0.0)),
        arm(
            node_pat(b, "$String", &["_", "_", "_", "_", "_"]),
            b.float(0.0),
        ),
        arm(node_pat(b, "$Record", &["_", "_"]), b.float(0.0)),
        arm(node_pat(b, "$Objects", &["pids", "_"]), cost_objects_arm(b)),
        arm(node_pat(b, "$Union", &["mids"]), cost_union_arm(b)),
        arm(node_pat(b, "$Suspend", &["bid"]), cost_suspend_arm(b)),
        arm(
            node_pat(b, "$Map", &["iid", "_"]),
            b.app2(rt(b, "Helper.list_at"), b.loc("iid"), b.var("mcs")),
        ),
        arm(
            node_pat(b, "$Arrays", &["eids", "rid_o", "_", "amin", "_"]),
            cost_arrays_arm(b),
        ),
    ];
    b.match_(b.loc("spec"), arms)
}

fn recs_expr(b: &ExprBuilder, ep: Pat) -> Expr {
    let push_deps = b.match_(
        b.app2(rt(b, "Helper.list_at"), b.loc("cur"), b.loc("nodes")),
        [arm(
            b.tup_pat(&["cd", "_", "_", "_", "_"]),
            b.assign_e(
                "stk",
                b.app2(b.global("List.Core", "append"), b.var("stk"), b.loc("cd")),
            ),
        )],
    );
    let visit_cur = b.block(
        [
            b.assign(
                "vis",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("vis"),
                    b.list([b.loc("cur")]),
                ),
            ),
            Stmt::Expr(push_deps),
        ],
        b.unit(),
    );
    let seen = b.app2(
        b.global("List.Core", "any"),
        b.lam(&["x"], b.bin(BinOp::Eq, b.loc("x"), b.loc("cur"))),
        b.var("vis"),
    );
    let bfs_step = b.block(
        [
            b.let_(
                "cur",
                b.app2(rt(b, "Helper.list_at"), b.int(0), b.var("stk")),
            ),
            b.assign(
                "stk",
                b.app2(b.global("List.Core", "drop"), b.int(1), b.var("stk")),
            ),
        ],
        b.if_(
            b.bin(BinOp::Eq, b.loc("cur"), b.loc("me")),
            b.assign_e("hit", b.bool_(true)),
            b.if_(seen, b.unit(), visit_cur),
        ),
    );
    let bfs_cond = b.bin(
        BinOp::And,
        b.not(b.var("hit")),
        b.bin(
            BinOp::Gt,
            b.app(b.global("List.Core", "length"), b.var("stk")),
            b.int(0),
        ),
    );
    let check_sus = b.block(
        [
            b.var_decl("vis", b.list(vec![])),
            b.var_decl("stk", b.loc("deps")),
            b.var_decl("hit", b.bool_(false)),
            Stmt::Expr(b.while_(bfs_cond, bfs_step)),
        ],
        b.yield_(b.var("hit")),
    );
    let on_nd = b.match_(
        b.loc("nd"),
        [arm(
            ep,
            b.if_(b.not(b.loc("is_sus")), b.yield_(b.bool_(false)), check_sus),
        )],
    );
    let step = b.block(
        [
            b.let_("me", b.var("i0")),
            b.assign("i0", b.add_i(b.var("i0"), b.int(1))),
        ],
        on_nd,
    );
    b.gen_strict(b.each("nd", b.loc("nodes"), step))
}

fn may_recurse_loop(b: &ExprBuilder) -> Expr {
    let any_dep = b.app2(
        b.global("List.Core", "any"),
        b.lam(
            &["d"],
            b.app2(rt(b, "Helper.list_at"), b.loc("d"), b.var("mrs")),
        ),
        b.loc("ds"),
    );
    let mark_mr = b.block(
        [
            b.assign(
                "mrs",
                b.app3(
                    rt(b, "Helper.arb_replace_at"),
                    b.var("mrs"),
                    b.var("j"),
                    b.bool_(true),
                ),
            ),
            b.assign("changed", b.bool_(true)),
        ],
        b.unit(),
    );
    let check_deps = b.match_(
        b.app2(rt(b, "Helper.list_at"), b.var("j"), b.loc("nodes")),
        [arm(
            b.tup_pat(&["ds", "_", "_", "_", "_"]),
            b.if_(any_dep, mark_mr, b.unit()),
        )],
    );
    let inner_step = b.block(
        [
            Stmt::Expr(b.if_(
                b.not(b.app2(rt(b, "Helper.list_at"), b.var("j"), b.var("mrs"))),
                check_deps,
                b.unit(),
            )),
            b.assign("j", b.add_i(b.var("j"), b.int(1))),
        ],
        b.unit(),
    );
    let outer_step = b.block(
        [
            b.assign("changed", b.bool_(false)),
            b.var_decl("j", b.int(0)),
            Stmt::Expr(b.while_(b.bin(BinOp::Lt, b.var("j"), b.loc("n")), inner_step)),
        ],
        b.unit(),
    );
    b.while_(b.var("changed"), outer_step)
}

fn min_cost_loop(b: &ExprBuilder, ep: Pat) -> Expr {
    let nxt = b.match_(
        b.app2(rt(b, "Helper.list_at"), b.var("i"), b.loc("nodes")),
        [arm(ep, compute_cost_expr(b))],
    );
    let update_mc = b.block(
        [
            b.assign(
                "mcs",
                b.app3(
                    rt(b, "Helper.arb_replace_at"),
                    b.var("mcs"),
                    b.var("i"),
                    b.loc("nxt"),
                ),
            ),
            b.assign("mc_ch", b.bool_(true)),
        ],
        b.unit(),
    );
    let inner_step = b.block(
        [
            b.let_("nxt", nxt),
            b.let_(
                "old",
                b.app2(rt(b, "Helper.list_at"), b.var("i"), b.var("mcs")),
            ),
            Stmt::Expr(b.if_(
                b.bin(BinOp::Lt, b.loc("nxt"), b.loc("old")),
                update_mc,
                b.unit(),
            )),
            b.assign("i", b.add_i(b.var("i"), b.int(1))),
        ],
        b.unit(),
    );
    let outer_step = b.block(
        [
            b.assign("mc_ch", b.bool_(false)),
            b.var_decl("i", b.int(0)),
            Stmt::Expr(b.while_(b.bin(BinOp::Lt, b.var("i"), b.loc("n")), inner_step)),
        ],
        b.unit(),
    );
    b.while_(b.var("mc_ch"), outer_step)
}

pub(super) fn settle_graph_helper(b: &ExprBuilder) -> Expr {
    let inf = float_inf(b);
    let ep = b.tup_pat(&["deps", "spec", "chks", "conv_o", "is_sus"]);
    let check_root = Stmt::Expr(b.if_(
        b.bin(
            BinOp::Eq,
            b.app2(rt(b, "Helper.list_at"), b.loc("root"), b.var("mcs")),
            inf.clone(),
        ),
        arb_err(b, str_lit(b, "a schema without a finite generation path")),
        b.unit(),
    ));
    let row = b.block(
        [
            b.let_("ck", b.var("k")),
            b.assign("k", b.add_i(b.var("k"), b.int(1))),
        ],
        b.yield_(b.tuple([
            b.app2(rt(b, "Helper.list_at"), b.loc("ck"), b.var("mcs")),
            b.app2(rt(b, "Helper.list_at"), b.loc("ck"), b.var("recs")),
            b.app2(rt(b, "Helper.list_at"), b.loc("ck"), b.var("mrs")),
            b.loc("spec"),
            b.loc("chks"),
            b.loc("conv_o"),
        ])),
    );
    let table = b.gen_strict(b.each(
        "nd",
        b.loc("nodes"),
        b.match_(b.loc("nd"), [arm(ep.clone(), row)]),
    ));
    let stmts = [
        b.let_("n", b.app(b.global("List.Core", "length"), b.loc("nodes"))),
        b.var_decl("i0", b.int(0)),
        b.var_decl("recs", recs_expr(b, ep.clone())),
        b.var_decl("mrs", b.var("recs")),
        b.var_decl("changed", b.bool_(true)),
        Stmt::Expr(may_recurse_loop(b)),
        b.var_decl(
            "mcs",
            b.gen_strict(b.each("_nd", b.loc("nodes"), b.yield_(inf))),
        ),
        b.var_decl("mc_ch", b.bool_(true)),
        Stmt::Expr(min_cost_loop(b, ep)),
        check_root,
        b.var_decl("k", b.int(0)),
    ];
    b.lam(&["nodes", "root"], b.block(stmts, table))
}
