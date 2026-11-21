//! `flickFormatCheckFailure` template for `Arbitrary`.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{cat, rt, str_lit};
use super::super::{Expr, Pat};

fn cat_chain(b: &ExprBuilder, init: impl IntoIterator<Item = Expr>, last: Expr) -> Expr {
    let mut parts: Vec<Expr> = init.into_iter().collect();
    let mut acc = last;
    while let Some(prev) = parts.pop() {
        acc = cat(b, prev, acc);
    }
    acc
}

fn falsified_body(b: &ExprBuilder) -> Expr {
    let pe_pat = Pat::Con(
        ExprBuilder::con("Arbitrary", "PropertyFailure", "PropertyError", 1),
        vec![Pat::Bind(b.name("err"))],
    );
    let err_line = cat(
        b,
        str_lit(b, "Failure: "),
        b.app(rt(b, "Arbitrary.FormatSpaced"), b.loc("err")),
    );
    let fail_line = b.match_(
        b.field(b.loc("f"), "failure"),
        [
            arm(pe_pat, err_line),
            arm(Pat::Wild, str_lit(b, "Failure: returned false")),
        ],
    );
    let l1 = cat_chain(
        b,
        [
            str_lit(b, "Property falsified after "),
            b.app(b.global("Int", "to_string"), b.field(b.loc("f"), "runs")),
            str_lit(b, " run(s) and "),
            b.app(b.global("Int", "to_string"), b.field(b.loc("f"), "shrinks")),
        ],
        str_lit(b, " shrink(s)"),
    );
    let l2 = cat(
        b,
        str_lit(b, "Shrunk input: "),
        b.app(
            rt(b, "Arbitrary.FormatSpaced"),
            b.field(b.loc("f"), "shrunkInput"),
        ),
    );
    let l4 = cat(b, str_lit(b, "Replay: "), b.field(b.loc("f"), "replay"));
    let joined = cat_chain(
        b,
        [
            b.loc("l1"),
            str_lit(b, "\n"),
            b.loc("l2"),
            str_lit(b, "\n"),
            b.loc("fail_line"),
            str_lit(b, "\n"),
        ],
        b.loc("l4"),
    );
    b.block(
        [
            b.let_("fail_line", fail_line),
            b.let_("l1", l1),
            b.let_("l2", l2),
            b.let_("l4", l4),
        ],
        b.some(joined),
    )
}

fn exhausted_body(b: &ExprBuilder) -> Expr {
    let msg = cat_chain(
        b,
        [
            str_lit(b, "Property exhausted after "),
            b.app(b.global("Int", "to_string"), b.field(b.loc("f"), "runs")),
            str_lit(b, " run(s) and "),
            b.app(
                b.global("Int", "to_string"),
                b.field(b.loc("f"), "discards"),
            ),
            str_lit(b, " discard(s)\nSeed: "),
        ],
        b.app(rt(b, "Arbitrary.QuoteJson"), b.field(b.loc("f"), "seed")),
    );
    b.some(msg)
}

pub(super) fn format_failure_template(b: &ExprBuilder) -> Expr {
    let rc = |c: &str, n: usize| ExprBuilder::con("Arbitrary", "CheckResult", c, n);
    let mismatch = b.some(cat(b, str_lit(b, "Property replay failed: "), b.loc("msg")));
    let arms = [
        arm(
            Pat::Con(rc("Falsified", 1), vec![Pat::Bind(b.name("f"))]),
            falsified_body(b),
        ),
        arm(
            Pat::Con(rc("Exhausted", 1), vec![Pat::Bind(b.name("f"))]),
            exhausted_body(b),
        ),
        arm(
            Pat::Con(rc("ReplayMismatch", 1), vec![Pat::Bind(b.name("msg"))]),
            mismatch,
        ),
        arm(Pat::Wild, b.none()),
    ];
    b.lam(&["res"], b.match_(b.loc("res"), arms))
}
