//! Audited `Check` size and object-property pure IR adapters.

use flick_ir::BinOp;

use super::expr_builder::{ExprBuilder, arm};
use super::std_check::{atom, expected_fail};
use super::std_schema_rt::{cat, con_app, pair_pat, rt, str_lit};
use super::{Expr, ExprKind, Pat, Stmt};

fn clamp_card(b: &ExprBuilder, n: Expr) -> Expr {
    b.if_(b.bin(BinOp::Lt, n.clone(), b.int(0)), b.int(0), n)
}

pub(super) fn check_size_one(b: &ExprBuilder, is_min: bool) -> Expr {
    let prefix = if is_min {
        "a value with a size of at least "
    } else {
        "a value with a size of at most "
    };
    let op = if is_min { BinOp::Ge } else { BinOp::Le };
    b.lam(
        &["n"],
        b.block(
            vec![
                b.let_("m", clamp_card(b, b.loc("n"))),
                b.let_(
                    "desc",
                    cat(
                        b,
                        str_lit(b, prefix),
                        b.app(b.global("Int", "to_string"), b.loc("m")),
                    ),
                ),
            ],
            b.list(vec![atom(
                b,
                b.lam(
                    &["v"],
                    b.if_(
                        b.bin(
                            op,
                            b.app(b.global("HashSet", "size"), b.loc("v")),
                            b.loc("m"),
                        ),
                        b.none(),
                        expected_fail(b, b.loc("desc"), b.loc("v")),
                    ),
                ),
                con_app(b, "Check", "$Meta", "$Approx", vec![b.loc("desc")]),
            )]),
        ),
    )
}

pub(super) fn check_size_between(b: &ExprBuilder) -> Expr {
    b.lam(
        &["lo", "hi"],
        b.block(
            vec![
                b.let_("l", clamp_card(b, b.loc("lo"))),
                b.let_("h", clamp_card(b, b.loc("hi"))),
                b.let_(
                    "desc",
                    b.if_(
                        b.bin(BinOp::Eq, b.loc("l"), b.loc("h")),
                        cat(
                            b,
                            str_lit(b, "a value with a size of "),
                            b.app(b.global("Int", "to_string"), b.loc("l")),
                        ),
                        cat(
                            b,
                            str_lit(b, "a value with a size between "),
                            cat(
                                b,
                                b.app(b.global("Int", "to_string"), b.loc("l")),
                                cat(
                                    b,
                                    str_lit(b, " and "),
                                    b.app(b.global("Int", "to_string"), b.loc("h")),
                                ),
                            ),
                        ),
                    ),
                ),
            ],
            b.list(vec![atom(
                b,
                b.lam(
                    &["v"],
                    b.block(
                        vec![b.let_("sz", b.app(b.global("HashSet", "size"), b.loc("v")))],
                        b.if_(
                            b.bin(
                                BinOp::And,
                                b.bin(BinOp::Ge, b.loc("sz"), b.loc("l")),
                                b.bin(BinOp::Le, b.loc("sz"), b.loc("h")),
                            ),
                            b.none(),
                            expected_fail(b, b.loc("desc"), b.loc("v")),
                        ),
                    ),
                ),
                con_app(b, "Check", "$Meta", "$Approx", vec![b.loc("desc")]),
            )]),
        ),
    )
}

fn entry_count(b: &ExprBuilder, n: Expr) -> Expr {
    b.if_(
        b.bin(BinOp::Eq, n.clone(), b.int(1)),
        str_lit(b, "1 entry"),
        cat(
            b,
            b.app(b.global("Int", "to_string"), n),
            str_lit(b, " entries"),
        ),
    )
}

fn obj_key_count(b: &ExprBuilder, obj: Expr) -> Expr {
    b.match_(
        b.app(rt(b, "Schema.RecordEntries"), obj),
        vec![
            arm(
                b.some_pat("es"),
                b.app(b.global("List.Core", "length"), b.loc("es")),
            ),
            arm(Pat::Wild, b.int(0)),
        ],
    )
}

pub(super) fn check_props_one(b: &ExprBuilder, is_min: bool) -> Expr {
    let prefix = if is_min {
        "a value with at least "
    } else {
        "a value with at most "
    };
    let kw = if is_min {
        "minProperties"
    } else {
        "maxProperties"
    };
    let op = if is_min { BinOp::Ge } else { BinOp::Le };
    b.lam(
        &["n", "c"],
        b.block(
            vec![
                b.let_("m", clamp_card(b, b.loc("n"))),
                b.let_(
                    "desc",
                    cat(b, str_lit(b, prefix), entry_count(b, b.loc("m"))),
                ),
                b.let_(
                    "kw",
                    b.at(ExprKind::Record(vec![(
                        kw.into(),
                        b.app(b.global("Int", "to_float"), b.loc("m")),
                    )])),
                ),
                b.let_(
                    "at",
                    atom(
                        b,
                        b.lam(
                            &["obj"],
                            b.if_(
                                b.bin(op, obj_key_count(b, b.loc("obj")), b.loc("m")),
                                b.none(),
                                expected_fail(b, b.loc("desc"), b.loc("obj")),
                            ),
                        ),
                        con_app(
                            b,
                            "Check",
                            "$Meta",
                            "$Props",
                            vec![b.loc("desc"), b.loc("kw")],
                        ),
                    ),
                ),
            ],
            b.app2(rt(b, "Helper.apply_prop_check"), b.loc("at"), b.loc("c")),
        ),
    )
}

pub(super) fn check_props_between(b: &ExprBuilder) -> Expr {
    b.lam(
        &["lo", "hi", "c"],
        b.block(
            vec![
                b.let_("l", clamp_card(b, b.loc("lo"))),
                b.let_("h", clamp_card(b, b.loc("hi"))),
                b.let_(
                    "desc",
                    b.if_(
                        b.bin(BinOp::Eq, b.loc("l"), b.loc("h")),
                        cat(
                            b,
                            str_lit(b, "a value with exactly "),
                            entry_count(b, b.loc("l")),
                        ),
                        cat(
                            b,
                            str_lit(b, "a value with between "),
                            cat(
                                b,
                                b.app(b.global("Int", "to_string"), b.loc("l")),
                                cat(
                                    b,
                                    str_lit(b, " and "),
                                    cat(
                                        b,
                                        b.app(b.global("Int", "to_string"), b.loc("h")),
                                        str_lit(b, " entries"),
                                    ),
                                ),
                            ),
                        ),
                    ),
                ),
                b.let_(
                    "kw",
                    b.at(ExprKind::Record(vec![
                        (
                            "maxProperties".into(),
                            b.app(b.global("Int", "to_float"), b.loc("h")),
                        ),
                        (
                            "minProperties".into(),
                            b.app(b.global("Int", "to_float"), b.loc("l")),
                        ),
                    ])),
                ),
                b.let_(
                    "at",
                    atom(
                        b,
                        b.lam(
                            &["obj"],
                            b.block(
                                vec![b.let_("k", obj_key_count(b, b.loc("obj")))],
                                b.if_(
                                    b.bin(
                                        BinOp::And,
                                        b.bin(BinOp::Ge, b.loc("k"), b.loc("l")),
                                        b.bin(BinOp::Le, b.loc("k"), b.loc("h")),
                                    ),
                                    b.none(),
                                    expected_fail(b, b.loc("desc"), b.loc("obj")),
                                ),
                            ),
                        ),
                        con_app(
                            b,
                            "Check",
                            "$Meta",
                            "$Props",
                            vec![b.loc("desc"), b.loc("kw")],
                        ),
                    ),
                ),
            ],
            b.app2(rt(b, "Helper.apply_prop_check"), b.loc("at"), b.loc("c")),
        ),
    )
}

pub(super) fn check_property_names(b: &ExprBuilder) -> Expr {
    let desc = "an object with property names matching the schema";
    b.lam(
        &["key_c", "c"],
        b.block(
            vec![
                b.let_("kd", b.app(rt(b, "Helper.codec_dec"), b.loc("key_c"))),
                b.let_(
                    "at",
                    atom(
                        b,
                        b.lam(
                            &["obj"],
                            b.match_(
                                b.app(rt(b, "Schema.RecordEntries"), b.loc("obj")),
                                vec![
                                    arm(
                                        b.some_pat("es"),
                                        b.block(
                                            vec![
                                                b.var_decl("ok", b.bool_(true)),
                                                Stmt::Expr(b.for_ctrl(
                                                    "$cpn",
                                                    "ent",
                                                    b.loc("es"),
                                                    b.match_(
                                                        b.loc("ent"),
                                                        vec![arm(
                                                            pair_pat(b, "k", "_v"),
                                                            b.match_(
                                                                b.app(b.loc("kd"), b.loc("k")),
                                                                vec![
                                                                    arm(b.ok_pat("_"), b.unit()),
                                                                    arm(
                                                                        Pat::Wild,
                                                                        b.block(
                                                                            vec![b.assign(
                                                                                "ok",
                                                                                b.bool_(false),
                                                                            )],
                                                                            b.break_(
                                                                                "$cpn",
                                                                                b.unit(),
                                                                            ),
                                                                        ),
                                                                    ),
                                                                ],
                                                            ),
                                                        )],
                                                    ),
                                                )),
                                            ],
                                            b.if_(
                                                b.var("ok"),
                                                b.none(),
                                                expected_fail(b, str_lit(b, desc), b.loc("obj")),
                                            ),
                                        ),
                                    ),
                                    arm(
                                        Pat::Wild,
                                        expected_fail(b, str_lit(b, desc), b.loc("obj")),
                                    ),
                                ],
                            ),
                        ),
                        con_app(
                            b,
                            "Check",
                            "$Meta",
                            "$Props",
                            vec![str_lit(b, desc), b.at(ExprKind::Record(vec![]))],
                        ),
                    ),
                ),
            ],
            b.app2(rt(b, "Helper.apply_prop_check"), b.loc("at"), b.loc("c")),
        ),
    )
}
