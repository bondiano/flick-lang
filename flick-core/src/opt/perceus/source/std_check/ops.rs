//! `Check` codec application, int-range, `Ord`-dictionary, and multiple-of templates.

use flick_ir::{BinOp, Dict};

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{cat, con_app, extra_con, make_codec, meta_con, rt, str_lit};
use super::super::{Expr, ExprKind, Pat};
use super::{atom, builtin_check, expected_fail};

pub(super) fn check_apply(b: &ExprBuilder) -> Expr {
    let atom_pat = Pat::Con(
        ExprBuilder::con("Check", "$Atom", "$Atom", 3),
        vec![Pat::Wild, Pat::Bind(b.name("m")), Pat::Wild],
    );
    let new_ms = b.gen_strict(b.each(
        "a",
        b.loc("chk"),
        b.match_(b.loc("a"), [arm(atom_pat, b.yield_(b.loc("m")))]),
    ));
    let checked_pat = Pat::Con(
        ExprBuilder::con("Codec", "$Meta", "$Checked", 4),
        vec![
            Pat::Bind(b.name("bm")),
            Pat::Bind(b.name("cs")),
            Pat::Wild,
            Pat::Wild,
        ],
    );
    let merged_meta = meta_con(
        b,
        "$Checked",
        vec![
            b.loc("bm"),
            b.app2(
                b.global("List.Core", "append"),
                b.loc("cs"),
                b.loc("new_ms"),
            ),
            b.none(),
            b.none(),
        ],
    );
    let fresh_meta = meta_con(
        b,
        "$Checked",
        vec![b.loc("om"), b.loc("new_ms"), b.none(), b.none()],
    );
    let nm = b.match_(
        b.loc("om"),
        [arm(checked_pat, merged_meta), arm(Pat::Wild, fresh_meta)],
    );
    let dec = b.lam(
        &["in"],
        b.match_(
            b.app(b.app(rt(b, "Helper.codec_dec"), b.loc("c")), b.loc("in")),
            [
                arm(
                    b.ok_pat("v"),
                    b.app2(rt(b, "Helper.run_checks"), b.loc("chk"), b.loc("v")),
                ),
                arm(b.err_pat("e"), b.err(b.loc("e"))),
            ],
        ),
    );
    let enc = b.lam(
        &["v"],
        b.match_(
            b.app2(rt(b, "Helper.run_checks"), b.loc("chk"), b.loc("v")),
            [
                arm(
                    b.ok_pat("_"),
                    b.app(b.app(rt(b, "Helper.codec_enc"), b.loc("c")), b.loc("v")),
                ),
                arm(b.err_pat("e"), b.err(b.loc("e"))),
            ],
        ),
    );
    let codec = make_codec(
        b,
        dec,
        enc,
        b.app(rt(b, "Helper.codec_acc"), b.loc("c")),
        b.loc("nm"),
        extra_con(b, "$Checked", vec![b.loc("c"), b.loc("chk")]),
    );
    b.lam(
        &["chk", "c"],
        b.block(
            [
                b.let_("new_ms", new_ms),
                b.let_("om", b.app(rt(b, "Helper.codec_meta"), b.loc("c"))),
                b.let_("nm", nm),
            ],
            codec,
        ),
    )
}

pub(super) fn int_range_check(b: &ExprBuilder, lo: f64, hi: f64) -> Expr {
    let range = b.at(ExprKind::Record(vec![
        ("exclusiveMaximum".into(), b.bool_(false)),
        ("exclusiveMinimum".into(), b.bool_(false)),
        ("maximum".into(), b.float(hi)),
        ("minimum".into(), b.float(lo)),
    ]));
    b.app2(
        b.global("List.Core", "append"),
        builtin_check(b, "int", b.unit()),
        builtin_check(b, "between", range),
    )
}

fn ord_is_variant(b: &ExprBuilder, variant: &str, expr: Expr) -> Expr {
    b.match_(
        expr,
        [
            arm(
                Pat::Con(ExprBuilder::con("Prelude", "Ordering", variant, 0), vec![]),
                b.bool_(true),
            ),
            arm(Pat::Wild, b.bool_(false)),
        ],
    )
}

fn ord_cmp_fn(b: &ExprBuilder) -> Expr {
    b.at(ExprKind::Method {
        dict: Dict::Field(b.name("ord")),
        name: "compare".into(),
    })
}

pub(super) fn ord_check_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    let less_than = |e: Expr| ord_is_variant(b, "LT", e);
    let greater_than = |e: Expr| ord_is_variant(b, "GT", e);
    let at_most = |e: Expr| b.not(greater_than(e));
    let at_least = |e: Expr| b.not(less_than(e));
    match (bind, stages) {
        (
            "flickMakeIsGreaterThan"
            | "flickMakeIsGreaterThanOrEqualTo"
            | "flickMakeIsLessThan"
            | "flickMakeIsLessThanOrEqualTo",
            1,
        ) => {
            let word = match bind {
                "flickMakeIsGreaterThan" => "a value greater than ",
                "flickMakeIsGreaterThanOrEqualTo" => "a value greater than or equal to ",
                "flickMakeIsLessThan" => "a value less than ",
                _ => "a value less than or equal to ",
            };
            let cmp_call = b.app2(ord_cmp_fn(b), b.loc("v"), b.loc("bound"));
            let pass = match bind {
                "flickMakeIsGreaterThan" => greater_than(cmp_call),
                "flickMakeIsGreaterThanOrEqualTo" => at_least(cmp_call),
                "flickMakeIsLessThan" => less_than(cmp_call),
                _ => at_most(cmp_call),
            };
            let desc = cat(
                b,
                str_lit(b, word),
                b.app(rt(b, "Schema.FormatValue"), b.loc("bound")),
            );
            let run_fn = b.lam(
                &["v"],
                b.if_(pass, b.none(), expected_fail(b, b.loc("desc"), b.loc("v"))),
            );
            let meta = con_app(b, "Check", "$Meta", "$None", vec![b.loc("desc")]);
            Some(b.lam(
                &["ord", "bound"],
                b.block([b.let_("desc", desc)], b.list([atom(b, run_fn, meta)])),
            ))
        }
        ("flickMakeIsBetween", 2) => {
            let tail = cat(
                b,
                str_lit(b, " and "),
                b.app(rt(b, "Schema.FormatValue"), b.loc("hi")),
            );
            let desc = cat(
                b,
                str_lit(b, "a value between "),
                cat(b, b.app(rt(b, "Schema.FormatValue"), b.loc("lo")), tail),
            );
            let in_bounds = b.bin(
                BinOp::And,
                at_least(b.app2(ord_cmp_fn(b), b.loc("v"), b.loc("lo"))),
                at_most(b.app2(ord_cmp_fn(b), b.loc("v"), b.loc("hi"))),
            );
            let run_fn = b.lam(
                &["v"],
                b.if_(
                    in_bounds,
                    b.none(),
                    expected_fail(b, b.loc("desc"), b.loc("v")),
                ),
            );
            let meta = con_app(b, "Check", "$Meta", "$None", vec![b.loc("desc")]);
            Some(b.lam(
                &["ord", "lo", "hi"],
                b.block([b.let_("desc", desc)], b.list([atom(b, run_fn, meta)])),
            ))
        }
        _ => None,
    }
}

pub(super) fn check_multiple_of_by(b: &ExprBuilder) -> Expr {
    let desc = cat(
        b,
        str_lit(b, "a value that is a multiple of "),
        b.app(rt(b, "Schema.FormatValue"), b.loc("div")),
    );
    let is_zero = b.app2(
        rt(b, "Schema.SameValue"),
        b.app2(b.loc("rem"), b.loc("v"), b.loc("div")),
        b.loc("zero"),
    );
    let run_fn = b.lam(
        &["v"],
        b.if_(
            is_zero,
            b.none(),
            expected_fail(b, b.loc("desc"), b.loc("v")),
        ),
    );
    let meta = con_app(b, "Check", "$Meta", "$None", vec![b.loc("desc")]);
    b.lam(
        &["rem", "zero", "div"],
        b.block([b.let_("desc", desc)], b.list([atom(b, run_fn, meta)])),
    )
}
