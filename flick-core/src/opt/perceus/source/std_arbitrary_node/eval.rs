//! Central `$Node` dispatcher (`arb_eval_node`) with check filtering and conversion.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat};
use super::eval_collections::{arrays_arm, record_arm};
use super::eval_scalars::{bigint_arm, decimal_arm, num_arm, string_arm};
use super::{st_pat, sub_f};

fn bool_arm(b: &ExprBuilder) -> Expr {
    let false_pull = b.app2(
        rt(b, "Helper.arb_pull_each"),
        b.list([b.bool_(false)]),
        b.lam(&["x"], b.some(b.tuple([b.loc("x"), b.none()]))),
    );
    let sample = b.if_(
        b.bin(BinOp::And, b.loc("shk"), b.loc("bv")),
        b.tuple([b.bool_(true), b.some(false_pull)]),
        b.tuple([b.loc("bv"), b.none()]),
    );
    b.block(
        [b.let_("bv", b.app(rt(b, "Helper.arb_st_bool"), b.loc("st")))],
        b.some(sample),
    )
}

fn objects_arm(b: &ExprBuilder) -> Expr {
    let vals = b.gen_strict(b.each(
        "s",
        b.loc("sams"),
        b.match_(
            b.loc("s"),
            [arm(pair_pat(b, "v", "_"), b.yield_(b.loc("v")))],
        ),
    ));
    let on_sams = b.some(b.if_(
        b.not(b.loc("shk")),
        b.tuple([b.app(b.loc("omk"), vals), b.none()]),
        b.app2(
            rt(b, "Helper.arb_product_sample"),
            b.loc("sams"),
            b.loc("omk"),
        ),
    ));
    b.match_(
        b.app3(
            rt(b, "Helper.arb_gen_samples"),
            b.loc("tbl"),
            b.loc("pids"),
            b.loc("st"),
        ),
        [arm(b.some_pat("sams"), on_sams), arm(Pat::Wild, b.none())],
    )
}

fn suspend_arm(b: &ExprBuilder) -> Expr {
    let eval_bid = b.app3(
        rt(b, "Helper.arb_eval_node"),
        b.loc("tbl"),
        b.loc("bid"),
        b.loc("st"),
    );
    let dec_eval = b.block(
        [b.rset(b.loc("br"), sub_f(b, b.loc("bd"), b.float(1.0)))],
        eval_bid.clone(),
    );
    let rec_branch = b.block(
        [b.let_("bd", b.rget(b.loc("br")))],
        b.if_(
            b.bin(BinOp::Le, b.loc("bd"), b.float(0.0)),
            b.none(),
            dec_eval,
        ),
    );
    b.if_(b.loc("is_rec"), rec_branch, eval_bid)
}

fn map_arm(b: &ExprBuilder) -> Expr {
    let mapped = b.some(b.app2(rt(b, "Helper.arb_map_sample"), b.loc("s"), b.loc("mf")));
    b.match_(
        b.app3(
            rt(b, "Helper.arb_eval_node"),
            b.loc("tbl"),
            b.loc("iid"),
            b.loc("st"),
        ),
        [arm(b.some_pat("s"), mapped), arm(Pat::Wild, b.none())],
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

fn spec_eval_expr(b: &ExprBuilder) -> Expr {
    let union_call = b.app3(
        rt(b, "Helper.arb_gen_union"),
        b.loc("tbl"),
        b.loc("mids"),
        b.loc("st"),
    );
    let arms = [
        arm(node_pat(b, "$Never", &[]), b.none()),
        arm(
            node_pat(b, "$Literal", &["lv"]),
            b.some(b.tuple([b.loc("lv"), b.none()])),
        ),
        arm(node_pat(b, "$Bool", &[]), bool_arm(b)),
        arm(
            node_pat(b, "$Number", &["min_o", "max_o", "nmode"]),
            num_arm(b),
        ),
        arm(
            node_pat(b, "$BigInt", &["bmin", "bmax", "btgt"]),
            bigint_arm(b),
        ),
        arm(node_pat(b, "$Decimal", &[]), decimal_arm(b)),
        arm(
            node_pat(b, "$String", &["smin", "smax", "has_cp", "cpmin", "cpmax"]),
            string_arm(b),
        ),
        arm(
            node_pat(b, "$Arrays", &["eids", "rid_o", "is_tup", "amin", "amax"]),
            arrays_arm(b),
        ),
        arm(node_pat(b, "$Objects", &["pids", "omk"]), objects_arm(b)),
        arm(node_pat(b, "$Union", &["mids"]), union_call),
        arm(node_pat(b, "$Suspend", &["bid"]), suspend_arm(b)),
        arm(node_pat(b, "$Record", &["kid", "vid"]), record_arm(b)),
        arm(node_pat(b, "$Map", &["iid", "mf"]), map_arm(b)),
    ];
    b.match_(b.loc("spec"), arms)
}

fn filtered_expr(b: &ExprBuilder) -> Expr {
    let pass = b.lam(
        &["v"],
        b.app2(
            b.global("List.Core", "all"),
            b.lam(&["p"], b.app(b.loc("p"), b.loc("v"))),
            b.loc("chks"),
        ),
    );
    let check_unshrunk = b.match_(
        b.loc("s"),
        [arm(
            pair_pat(b, "v", "_"),
            b.if_(
                b.app(b.loc("pass"), b.loc("v")),
                b.some(b.loc("s")),
                b.none(),
            ),
        )],
    );
    let on_s = b.block(
        [b.let_("pass", pass)],
        b.if_(
            b.loc("shk"),
            b.app2(rt(b, "Helper.arb_filter_sample"), b.loc("s"), b.loc("pass")),
            check_unshrunk,
        ),
    );
    let apply_checks = b.match_(
        b.loc("raw"),
        [arm(b.some_pat("s"), on_s), arm(Pat::Wild, b.none())],
    );
    let no_checks = b.bin(
        BinOp::Eq,
        b.app(b.global("List.Core", "length"), b.loc("chks")),
        b.int(0),
    );
    b.if_(no_checks, b.loc("raw"), apply_checks)
}

pub(super) fn eval_node_helper(b: &ExprBuilder) -> Expr {
    let map_filtered = b.match_(
        b.loc("filtered"),
        [
            arm(
                b.some_pat("s"),
                b.some(b.app2(rt(b, "Helper.arb_map_sample"), b.loc("s"), b.loc("cf"))),
            ),
            arm(Pat::Wild, b.none()),
        ],
    );
    let apply_conv = b.match_(
        b.loc("conv_o"),
        [
            arm(b.some_pat("cf"), map_filtered),
            arm(Pat::Wild, b.loc("filtered")),
        ],
    );
    let eval_body = b.block(
        [
            b.let_("raw", spec_eval_expr(b)),
            b.let_("filtered", filtered_expr(b)),
        ],
        apply_conv,
    );
    let row_pat = b.tup_pat(&["_mc", "is_rec", "_mr", "spec", "chks", "conv_o"]);
    let lookup_node = b.match_(
        b.app2(rt(b, "Helper.list_at"), b.loc("id"), b.loc("tbl")),
        [arm(row_pat, eval_body)],
    );
    b.lam(
        &["tbl", "id", "st"],
        b.match_(b.loc("st"), [arm(st_pat(b), lookup_node)]),
    )
}
