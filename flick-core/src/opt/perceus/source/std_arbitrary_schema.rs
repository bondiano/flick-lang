//! Pure-IR `Arbitrary` schema-to-flat-graph compiler (Stage 4D).

mod codec;
mod link;
mod primitives;
mod schema;
mod schema_arms;
mod settle;

use flick_ir::{Arith, BinOp, Num};

use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::{cat, con_app, pair_pat, rt, str_lit};
use super::{Expr, Pat, Span, Stmt};
use codec::add_codec_helper;
use link::build_link_helper;
use primitives::{build_bigint_helper, build_number_helper, build_string_helper};
use schema::add_schema_helper;
use settle::settle_graph_helper;

pub(super) fn arb_schema_helper(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$as");
    match name {
        "arb_compile_schema" => Some(compile_schema_helper(&b)),
        "arb_add_schema" => Some(add_schema_helper(&b)),
        "arb_add_codec" => Some(add_codec_helper(&b)),
        "arb_build_number" => Some(build_number_helper(&b)),
        "arb_build_string" => Some(build_string_helper(&b)),
        "arb_build_bigint" => Some(build_bigint_helper(&b)),
        "arb_build_link" => Some(build_link_helper(&b)),
        "arb_settle_graph" => Some(settle_graph_helper(&b)),
        _ => None,
    }
}

fn node_con(b: &ExprBuilder, c: &str, args: Vec<Expr>) -> Expr {
    con_app(b, "Arbitrary", "$Node", c, args)
}

fn add_f(b: &ExprBuilder, x: Expr, y: Expr) -> Expr {
    b.bin(BinOp::Arith(Arith::Add, Num::Float), x, y)
}

fn mul_f(b: &ExprBuilder, x: Expr, y: Expr) -> Expr {
    b.bin(BinOp::Arith(Arith::Mul, Num::Float), x, y)
}

fn float_inf(b: &ExprBuilder) -> Expr {
    b.bin(
        BinOp::Arith(Arith::Div, Num::Float),
        b.float(1.0),
        b.float(0.0),
    )
}

fn arb_err(b: &ExprBuilder, what: Expr) -> Expr {
    b.app(
        rt(b, "Schema.Defect"),
        cat(
            b,
            str_lit(b, "Error: Unable to derive an arbitrary for "),
            what,
        ),
    )
}

fn raw_entry(
    b: &ExprBuilder,
    deps: Expr,
    spec: Expr,
    chks: Expr,
    conv: Expr,
    is_sus: bool,
) -> Expr {
    b.tuple([deps, spec, chks, conv, b.bool_(is_sus)])
}

// Builder state `ctx`: `(nodes_ref, lazy_cache_ref)`
// Each raw node entry: `(deps: List Int, spec: $Node, checks: List (Val->Bool), conv_o: Option (Val->Val), is_suspend: Bool)`
fn push_node(b: &ExprBuilder, ctx: Expr, entry: Expr) -> Expr {
    let appended = b.app2(
        b.global("List.Core", "append"),
        b.loc("ns"),
        b.list([entry]),
    );
    let body = b.block(
        [
            b.let_("ns", b.rget(b.loc("nr"))),
            b.let_("id", b.app(b.global("List.Core", "length"), b.loc("ns"))),
            b.rset(b.loc("nr"), appended),
        ],
        b.loc("id"),
    );
    b.match_(ctx, [arm(pair_pat(b, "nr", "_lc"), body)])
}

fn push_simple(b: &ExprBuilder, deps: Expr, spec: Expr) -> Expr {
    push_node(
        b,
        b.loc("ctx"),
        raw_entry(b, deps, spec, b.list(vec![]), b.none(), false),
    )
}

fn set_node(b: &ExprBuilder, ctx: Expr, id: Expr, entry: Expr) -> Stmt {
    let replaced = b.app3(
        rt(b, "Helper.arb_replace_at"),
        b.rget(b.loc("nr")),
        id,
        entry,
    );
    let body = b.app2(b.global("Ref", "set"), b.loc("nr"), replaced);
    Stmt::Expr(b.match_(ctx, [arm(pair_pat(b, "nr", "_lc"), body)]))
}

fn c_pat(b: &ExprBuilder) -> Pat {
    Pat::Con(
        ExprBuilder::con("Arbitrary", "$Constraint", "$Constraint", 11),
        vec![
            Pat::Bind(b.name("cord")),
            Pat::Bind(b.name("cmin")),
            Pat::Bind(b.name("cmax")),
            Pat::Bind(b.name("cexmin")),
            Pat::Bind(b.name("cexmax")),
            Pat::Bind(b.name("cminl")),
            Pat::Bind(b.name("cmaxl")),
            Pat::Bind(b.name("cmincp")),
            Pat::Bind(b.name("cmaxcp")),
            Pat::Wild,
            Pat::Bind(b.name("cnum")),
        ],
    )
}

fn wire_finite_fn(b: &ExprBuilder) -> Expr {
    b.lam(&["x"], b.app(b.global("Float", "is_finite"), b.loc("x")))
}

fn js_int_fn(b: &ExprBuilder) -> Expr {
    let in_i32 = b.bin(
        BinOp::And,
        b.bin(BinOp::Ge, b.loc("x"), b.float(-2_147_483_648.0)),
        b.bin(BinOp::Le, b.loc("x"), b.float(2_147_483_647.0)),
    );
    let is_int = b.bin(
        BinOp::Eq,
        b.loc("x"),
        b.app(b.global("Float", "floor"), b.loc("x")),
    );
    b.lam(&["x"], b.bin(BinOp::And, is_int, in_i32))
}

fn compile_schema_helper(b: &ExprBuilder) -> Expr {
    let raw_nodes = b.match_(
        b.loc("ctx"),
        [arm(pair_pat(b, "nr", "_"), b.rget(b.loc("nr")))],
    );
    let rmc = b.match_(
        b.app2(rt(b, "Helper.list_at"), b.loc("root"), b.loc("tbl")),
        [arm(
            b.tup_pat(&["mc", "_", "_", "_", "_", "_"]),
            b.loc("mc"),
        )],
    );
    let stmts = [
        b.let_(
            "ctx",
            b.tuple([b.rmake(b.list(vec![])), b.rmake(b.list(vec![]))]),
        ),
        b.let_(
            "root",
            b.app4(
                rt(b, "Helper.arb_add_schema"),
                b.loc("ctx"),
                b.loc("sch"),
                b.bool_(false),
                b.none(),
            ),
        ),
        b.let_("raw_nodes", raw_nodes),
        b.let_(
            "tbl",
            b.app2(
                rt(b, "Helper.arb_settle_graph"),
                b.loc("raw_nodes"),
                b.loc("root"),
            ),
        ),
        b.let_("rmc", rmc),
    ];
    let gen_fn = b.lam(
        &["st"],
        b.app3(
            rt(b, "Helper.arb_eval_node"),
            b.loc("tbl"),
            b.loc("root"),
            b.loc("st"),
        ),
    );
    b.lam(&["sch"], b.block(stmts, b.tuple([b.loc("rmc"), gen_fn])))
}
