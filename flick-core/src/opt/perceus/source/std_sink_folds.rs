//! Audited `Sink` fold/collect/predicate pure IR adapters (Stage 4E).

mod basic;
mod predicates;

use flick_ir::{Arith, BinOp, Num};

use super::expr_builder::ExprBuilder;
use super::std_schema_rt::rt;
use super::{Expr, Span};

pub(super) fn sink_folds_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("fold" | "reduceWhile" | "reduceWhileEffect", 3) => Some(rt(b, "Helper.sk_fold")),
        ("foldArray" | "reduceWhileArray" | "reduceWhileArrayEffect", 3) => {
            Some(rt(b, "Helper.sk_fold_array"))
        }
        ("foldUntil", 3) => Some(rt(b, "Helper.sk_fold_until")),
        ("every", 1) => Some(rt(b, "Helper.sk_every")),
        ("some", 1) => Some(rt(b, "Helper.sk_some")),
        ("take", 1) => Some(rt(b, "Helper.sk_take")),
        ("reduce" | "reduceEffect", 2) => Some(reduce_tpl(b, "Helper.sk_fold")),
        ("reduceArray", 2) => Some(reduce_tpl(b, "Helper.sk_fold_array")),
        ("head", 1) => Some(b.lam(&["_u"], rt(b, "Helper.sk_head"))),
        ("last", 1) => Some(b.lam(&["_u"], rt(b, "Helper.sk_last"))),
        ("find" | "findEffect", 1) => Some(rt(b, "Helper.sk_find")),
        ("sum", 0) => Some(sum_tpl(b)),
        ("count", 0) => Some(count_tpl(b)),
        ("collect", 1) => Some(b.lam(&["_u"], rt(b, "Helper.sk_collect"))),
        ("takeWhile" | "takeWhileEffect", 1) => Some(rt(b, "Helper.sk_take_while")),
        ("takeWhileFilter" | "takeWhileFilterEffect", 1) => {
            Some(rt(b, "Helper.sk_take_while_filter"))
        }
        ("takeUntil" | "takeUntilEffect", 1) => Some(rt(b, "Helper.sk_take_until")),
        ("forEach", 1) => Some(for_each_tpl(b, "Helper.sk_fold", "x")),
        ("forEachArray", 1) => Some(for_each_tpl(b, "Helper.sk_fold_array", "xs")),
        ("forEachWhile", 1) => Some(rt(b, "Helper.sk_for_each_while")),
        ("forEachWhileArray", 1) => Some(rt(b, "Helper.sk_for_each_while_array")),
        _ => None,
    }
}

pub(super) fn sink_folds_helper(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$skf");
    match name {
        "sk_fold" => Some(basic::sk_fold_helper(&b)),
        "sk_fold_array" => Some(basic::sk_fold_array_helper(&b)),
        "sk_fold_until" => Some(basic::sk_fold_until_helper(&b)),
        "sk_take" => Some(basic::sk_take_helper(&b)),
        "sk_head" => Some(basic::sk_head_helper(&b)),
        "sk_last" => Some(basic::sk_last_helper(&b)),
        "sk_collect" => Some(basic::sk_collect_helper(&b)),
        "sk_find" => Some(predicates::sk_find_helper(&b)),
        "sk_every" => Some(predicates::sk_every_helper(&b)),
        "sk_some" => Some(predicates::sk_some_helper(&b)),
        "sk_take_while" => Some(predicates::sk_take_while_helper(&b)),
        "sk_take_while_filter" => Some(predicates::sk_take_while_filter_helper(&b)),
        "sk_take_until" => Some(predicates::sk_take_until_helper(&b)),
        "sk_for_each_while" => Some(predicates::sk_for_each_while_helper(&b)),
        "sk_for_each_while_array" => Some(predicates::sk_for_each_while_array_helper(&b)),
        _ => None,
    }
}

fn reduce_tpl(b: &ExprBuilder, helper: &str) -> Expr {
    let cont = b.lam(&["_s"], b.bool_(true));
    b.lam(
        &["init", "f"],
        b.app3(rt(b, helper), b.loc("init"), cont, b.loc("f")),
    )
}

fn sum_tpl(b: &ExprBuilder) -> Expr {
    let init = b.lam(&["_u"], b.float(0.0));
    let cont = b.lam(&["_s"], b.bool_(true));
    let step = b.lam(
        &["a", "x"],
        b.bin(BinOp::Arith(Arith::Add, Num::Float), b.loc("a"), b.loc("x")),
    );
    b.app3(rt(b, "Helper.sk_fold"), init, cont, step)
}

fn count_tpl(b: &ExprBuilder) -> Expr {
    let init = b.lam(&["_u"], b.int(0));
    let cont = b.lam(&["_s"], b.bool_(true));
    let len = b.app(b.global("List.Core", "length"), b.loc("xs"));
    let step = b.lam(&["n", "xs"], b.add_i(b.loc("n"), len));
    b.app3(rt(b, "Helper.sk_fold_array"), init, cont, step)
}

fn for_each_tpl(b: &ExprBuilder, helper: &str, item: &str) -> Expr {
    let init = b.lam(&["_u"], b.unit());
    let cont = b.lam(&["_s"], b.bool_(true));
    let body = b.block([b.let_("_r", b.app(b.loc("f"), b.loc(item)))], b.unit());
    let step = b.lam(&["_s", item], body);
    b.lam(&["f"], b.app3(rt(b, helper), init, cont, step))
}
