//! Pure-IR `Arbitrary` `GenState`, runner, shrinking loop, and replay helpers (Stage 4D).

mod check;
mod replay;
mod sample;
mod shrink;
mod state;

use flick_ir::{Arith, BinOp, Num};

use super::expr_builder::ExprBuilder;
use super::{Expr, Pat, Span};
use check::arb_check_helper;
use replay::arb_replay_check_helper;
use sample::arb_sample_helper;
use shrink::{
    evaluate_helper, follow_replay_helper, make_replay_helper, prop_tag_helper,
    shrink_failure_helper,
};
use state::{
    gen_attempt_helper, gen_product_helper, reserving_helper, shuffle_helper, st_below_helper,
    st_bool_helper, st_index_helper, st_int_helper, st_len_helper,
};

pub(super) fn arb_run_helper(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$ar");
    match name {
        "arb_st_below" => Some(st_below_helper(&b)),
        "arb_st_index" => Some(st_index_helper(&b)),
        "arb_st_int" => Some(st_int_helper(&b)),
        "arb_st_bool" => Some(st_bool_helper(&b)),
        "arb_st_len" => Some(st_len_helper(&b)),
        "arb_shuffle" => Some(shuffle_helper(&b)),
        "arb_reserving" => Some(reserving_helper(&b)),
        "arb_gen_product" => Some(gen_product_helper(&b)),
        "arb_gen_attempt" => Some(gen_attempt_helper(&b)),
        "arb_evaluate" => Some(evaluate_helper(&b)),
        "arb_prop_tag" => Some(prop_tag_helper(&b)),
        "arb_shrink_failure" => Some(shrink_failure_helper(&b)),
        "arb_follow_replay" => Some(follow_replay_helper(&b)),
        "arb_make_replay" => Some(make_replay_helper(&b)),
        "arb_sample" => Some(arb_sample_helper(&b)),
        "arb_check" => Some(arb_check_helper(&b)),
        "arb_replay_check" => Some(arb_replay_check_helper(&b)),
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
