//! Audited enum and refined `Schema` pure IR adapters (`unsafe_literals`, `unsafe_enum_tags`, `unsafe_refined`).

use flick_ir::BinOp;

use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::{
    cat, expected_err, extra_con, kind_con, make_codec, make_schema, meta_con, pair_pat, rt,
    schema_err, str_lit,
};
use super::{Expr, Pat, Stmt};

pub(super) fn schema_literals(b: &ExprBuilder) -> Expr {
    b.lam(
        &["base", "_value", "values"],
        b.block(
            vec![
                b.let_("bc", b.app(rt(b, "Helper.schema_codec"), b.loc("base"))),
                b.let_("bd", b.app(rt(b, "Helper.codec_dec"), b.loc("bc"))),
                b.let_("be", b.app(rt(b, "Helper.codec_enc"), b.loc("bc"))),
                b.let_(
                    "lits",
                    b.gen_strict(b.each(
                        "v",
                        b.loc("values"),
                        b.match_(
                            b.app(b.loc("be"), b.loc("v")),
                            vec![
                                arm(b.ok_pat("w"), b.yield_(b.loc("w"))),
                                arm(Pat::Wild, b.yield_(b.loc("v"))),
                            ],
                        ),
                    )),
                ),
            ],
            make_schema(
                b,
                b.lam(
                    &["in"],
                    b.match_(
                        b.app(
                            b.app(rt(b, "Helper.schema_val"), b.loc("base")),
                            b.loc("in"),
                        ),
                        vec![
                            arm(
                                b.ok_pat("v"),
                                b.if_(
                                    b.app2(
                                        rt(b, "Schema.LiteralAccepts"),
                                        b.loc("values"),
                                        b.loc("v"),
                                    ),
                                    b.ok(b.loc("v")),
                                    expected_err(b, str_lit(b, "Union"), b.loc("in")),
                                ),
                            ),
                            arm(b.err_pat("e"), b.err(b.loc("e"))),
                        ],
                    ),
                ),
                make_codec(
                    b,
                    b.lam(
                        &["in"],
                        b.match_(
                            b.app(b.loc("bd"), b.loc("in")),
                            vec![
                                arm(
                                    b.ok_pat("v"),
                                    b.if_(
                                        b.app2(
                                            rt(b, "Schema.LiteralAccepts"),
                                            b.loc("values"),
                                            b.loc("v"),
                                        ),
                                        b.ok(b.loc("v")),
                                        expected_err(b, str_lit(b, "Union"), b.loc("in")),
                                    ),
                                ),
                                arm(b.err_pat("e"), b.err(b.loc("e"))),
                            ],
                        ),
                    ),
                    b.lam(
                        &["v"],
                        b.if_(
                            b.app2(rt(b, "Schema.LiteralAccepts"), b.loc("values"), b.loc("v")),
                            b.app(b.loc("be"), b.loc("v")),
                            expected_err(b, str_lit(b, "Union"), b.loc("v")),
                        ),
                    ),
                    b.app(rt(b, "Helper.codec_acc"), b.loc("bc")),
                    meta_con(b, "$Enum", vec![b.loc("lits")]),
                    extra_con(b, "$None", vec![]),
                ),
                kind_con(b, "$Enum", vec![b.loc("values")]),
                b.none(),
            ),
        ),
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "closed enum tags schema AST template"
)]
pub(super) fn schema_enum_tags(b: &ExprBuilder) -> Expr {
    b.lam(
        &["base", "pairs"],
        b.block(
            vec![
                b.let_(
                    "vals",
                    b.gen_strict(b.each(
                        "p",
                        b.loc("pairs"),
                        b.match_(
                            b.loc("p"),
                            vec![arm(pair_pat(b, "_n", "v"), b.yield_(b.loc("v")))],
                        ),
                    )),
                ),
                b.let_(
                    "cases",
                    b.gen_strict(b.each(
                        "p",
                        b.loc("pairs"),
                        b.match_(
                            b.loc("p"),
                            vec![arm(
                                pair_pat(b, "n", "_v"),
                                b.yield_(b.tuple(vec![b.loc("n"), b.loc("n"), b.list(vec![])])),
                            )],
                        ),
                    )),
                ),
                b.let_(
                    "adt_s",
                    b.app2(
                        b.global("Schema", "unsafe_adt"),
                        str_lit(b, ""),
                        b.loc("cases"),
                    ),
                ),
                b.let_("adt_c", b.app(rt(b, "Helper.schema_codec"), b.loc("adt_s"))),
            ],
            make_schema(
                b,
                b.lam(
                    &["in"],
                    b.match_(
                        b.app(
                            b.app(rt(b, "Helper.schema_val"), b.loc("base")),
                            b.loc("in"),
                        ),
                        vec![
                            arm(
                                b.ok_pat("v"),
                                b.if_(
                                    b.app2(
                                        rt(b, "Schema.LiteralAccepts"),
                                        b.loc("vals"),
                                        b.loc("v"),
                                    ),
                                    b.ok(b.loc("v")),
                                    expected_err(b, str_lit(b, "Union"), b.loc("in")),
                                ),
                            ),
                            arm(b.err_pat("e"), b.err(b.loc("e"))),
                        ],
                    ),
                ),
                make_codec(
                    b,
                    b.lam(
                        &["in"],
                        b.match_(
                            b.app(
                                b.app(rt(b, "Helper.codec_dec"), b.loc("adt_c")),
                                b.loc("in"),
                            ),
                            vec![
                                arm(
                                    b.ok_pat("d"),
                                    b.match_(
                                        b.app(rt(b, "Schema.TagOf"), b.loc("d")),
                                        vec![arm(
                                            pair_pat(b, "t", "_s"),
                                            b.block(
                                                vec![
                                                    b.var_decl("out", b.none()),
                                                    Stmt::Expr(b.for_ctrl(
                                                        "$etd",
                                                        "p",
                                                        b.loc("pairs"),
                                                        b.match_(
                                                            b.loc("p"),
                                                            vec![arm(
                                                                pair_pat(b, "n", "v"),
                                                                b.if_(
                                                                    b.bin(
                                                                        BinOp::Eq,
                                                                        b.loc("n"),
                                                                        b.loc("t"),
                                                                    ),
                                                                    b.block(
                                                                        vec![b.assign(
                                                                            "out",
                                                                            b.some(b.loc("v")),
                                                                        )],
                                                                        b.break_("$etd", b.unit()),
                                                                    ),
                                                                    b.unit(),
                                                                ),
                                                            )],
                                                        ),
                                                    )),
                                                ],
                                                b.match_(
                                                    b.var("out"),
                                                    vec![
                                                        arm(b.some_pat("v"), b.ok(b.loc("v"))),
                                                        arm(
                                                            Pat::Wild,
                                                            expected_err(
                                                                b,
                                                                str_lit(b, "Union"),
                                                                b.loc("in"),
                                                            ),
                                                        ),
                                                    ],
                                                ),
                                            ),
                                        )],
                                    ),
                                ),
                                arm(b.err_pat("e"), b.err(b.loc("e"))),
                            ],
                        ),
                    ),
                    b.lam(
                        &["v"],
                        b.block(
                            vec![
                                b.var_decl("out", b.none()),
                                Stmt::Expr(b.for_ctrl(
                                    "$ete",
                                    "p",
                                    b.loc("pairs"),
                                    b.match_(
                                        b.loc("p"),
                                        vec![arm(
                                            pair_pat(b, "n", "pv"),
                                            b.if_(
                                                b.app2(
                                                    rt(b, "Schema.SameValue"),
                                                    b.loc("pv"),
                                                    b.loc("v"),
                                                ),
                                                b.block(
                                                    vec![b.assign("out", b.some(b.loc("n")))],
                                                    b.break_("$ete", b.unit()),
                                                ),
                                                b.unit(),
                                            ),
                                        )],
                                    ),
                                )),
                            ],
                            b.match_(
                                b.var("out"),
                                vec![
                                    arm(
                                        b.some_pat("n"),
                                        b.ok(b.app(
                                            rt(b, "Schema.RecordFromEntries"),
                                            b.list(vec![
                                                b.tuple(vec![str_lit(b, "_tag"), b.loc("n")]),
                                            ]),
                                        )),
                                    ),
                                    arm(
                                        Pat::Wild,
                                        expected_err(b, str_lit(b, "Union"), b.loc("v")),
                                    ),
                                ],
                            ),
                        ),
                    ),
                    b.app(rt(b, "Helper.codec_acc"), b.loc("adt_c")),
                    b.app(rt(b, "Helper.codec_meta"), b.loc("adt_c")),
                    extra_con(b, "$None", vec![]),
                ),
                kind_con(b, "$EnumTags", vec![b.loc("pairs")]),
                b.none(),
            ),
        ),
    )
}

pub(super) fn schema_refined(b: &ExprBuilder) -> Expr {
    b.lam(
        &["name", "known", "base", "parse"],
        b.block(
            vec![
                b.let_("bc", b.app(rt(b, "Helper.schema_codec"), b.loc("base"))),
                b.let_(
                    "chk",
                    b.lam(
                        &["v"],
                        b.match_(
                            b.app(b.loc("parse"), b.loc("v")),
                            vec![
                                arm(b.ok_pat("_"), b.bool_(true)),
                                arm(Pat::Wild, b.bool_(false)),
                            ],
                        ),
                    ),
                ),
                b.let_(
                    "rerr",
                    schema_err(b, cat(b, str_lit(b, "not a valid "), b.loc("name"))),
                ),
            ],
            make_schema(
                b,
                b.lam(
                    &["in"],
                    b.match_(
                        b.app(
                            b.app(rt(b, "Helper.schema_val"), b.loc("base")),
                            b.loc("in"),
                        ),
                        vec![
                            arm(
                                b.ok_pat("v"),
                                b.if_(
                                    b.app(b.loc("chk"), b.loc("v")),
                                    b.ok(b.loc("v")),
                                    b.err(b.loc("rerr")),
                                ),
                            ),
                            arm(b.err_pat("e"), b.err(b.loc("e"))),
                        ],
                    ),
                ),
                make_codec(
                    b,
                    b.lam(
                        &["in"],
                        b.match_(
                            b.app(b.app(rt(b, "Helper.codec_dec"), b.loc("bc")), b.loc("in")),
                            vec![
                                arm(
                                    b.ok_pat("v"),
                                    b.if_(
                                        b.app(b.loc("chk"), b.loc("v")),
                                        b.ok(b.loc("v")),
                                        b.err(b.loc("rerr")),
                                    ),
                                ),
                                arm(b.err_pat("e"), b.err(b.loc("e"))),
                            ],
                        ),
                    ),
                    b.lam(
                        &["v"],
                        b.if_(
                            b.app(b.loc("chk"), b.loc("v")),
                            b.app(b.app(rt(b, "Helper.codec_enc"), b.loc("bc")), b.loc("v")),
                            b.err(b.loc("rerr")),
                        ),
                    ),
                    b.app(rt(b, "Helper.codec_acc"), b.loc("bc")),
                    meta_con(
                        b,
                        "$Refined",
                        vec![
                            b.loc("name"),
                            b.loc("known"),
                            b.app(rt(b, "Helper.codec_meta"), b.loc("bc")),
                        ],
                    ),
                    extra_con(b, "$None", vec![]),
                ),
                kind_con(
                    b,
                    "$Refined",
                    vec![b.loc("name"), b.loc("base"), b.loc("chk")],
                ),
                b.none(),
            ),
        ),
    )
}
