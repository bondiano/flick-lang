//! Pure-IR `Arbitrary` node evaluator and scalar shrink-tree helpers (Stage 4D).

mod compound;
mod eval;
mod eval_collections;
mod eval_scalars;
mod scalars;

use flick_ir::{Arith, BinOp, Num};

use super::expr_builder::ExprBuilder;
use super::{Expr, Pat, Span};
use compound::{gen_repeated_helper, gen_samples_helper, gen_union_helper};
use eval::eval_node_helper;
use scalars::{
    bigint_sample_helper, decimal_sample_helper, number_sample_helper, numeric_int_helper,
    string_sample_helper,
};

pub(super) fn arb_node_helper(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$an");
    match name {
        "arb_number_sample" => Some(number_sample_helper(&b)),
        "arb_bigint_sample" => Some(bigint_sample_helper(&b)),
        "arb_decimal_sample" => Some(decimal_sample_helper(&b)),
        "arb_string_sample" => Some(string_sample_helper(&b)),
        "arb_numeric_int" => Some(numeric_int_helper(&b)),
        "arb_gen_union" => Some(gen_union_helper(&b)),
        "arb_gen_samples" => Some(gen_samples_helper(&b)),
        "arb_gen_repeated" => Some(gen_repeated_helper(&b)),
        "arb_eval_node" => Some(eval_node_helper(&b)),
        _ => None,
    }
}

fn add_f(b: &ExprBuilder, x: Expr, y: Expr) -> Expr {
    b.bin(BinOp::Arith(Arith::Add, Num::Float), x, y)
}

fn sub_f(b: &ExprBuilder, x: Expr, y: Expr) -> Expr {
    b.bin(BinOp::Arith(Arith::Sub, Num::Float), x, y)
}

fn mul_f(b: &ExprBuilder, x: Expr, y: Expr) -> Expr {
    b.bin(BinOp::Arith(Arith::Mul, Num::Float), x, y)
}

fn div_f(b: &ExprBuilder, x: Expr, y: Expr) -> Expr {
    b.bin(BinOp::Arith(Arith::Div, Num::Float), x, y)
}

fn st_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["sz", "shk", "bias", "rr", "br"])
}
