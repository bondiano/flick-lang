//! Synthetic `$rt.Helper.*` structural `Schema.validate` helpers.

use flick_ir::BinOp;

use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::{
    cat, expected_err, map_result_list, pair_pat, rt, schema_err, str_lit, triple_pat,
};
use super::{Expr, Pat, Stmt};

pub(super) fn validate_list_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["vf", "input"],
        b.match_(
            b.app(rt(b, "Schema.ListFromWire"), b.loc("input")),
            vec![
                arm(
                    b.some_pat("xs"),
                    map_result_list(
                        b,
                        "$vl",
                        b.loc("xs"),
                        "x",
                        "i",
                        b.app(b.loc("vf"), b.loc("x")),
                        |idx, err| b.app2(rt(b, "Schema.PrefixIssues"), idx, err),
                    ),
                ),
                arm(
                    Pat::Wild,
                    expected_err(b, str_lit(b, "a list"), b.loc("input")),
                ),
            ],
        ),
    )
}

pub(super) fn validate_tuple_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["vfs", "input"],
        b.block(
            vec![b.let_("n", b.app(b.global("List.Core", "length"), b.loc("vfs")))],
            b.match_(
                b.app(rt(b, "Schema.TupleToList"), b.loc("input")),
                vec![
                    arm(
                        b.some_pat("xs"),
                        b.block(
                            vec![
                                b.let_("xlen", b.app(b.global("List.Core", "length"), b.loc("xs"))),
                            ],
                            b.if_(
                                b.bin(BinOp::Lt, b.loc("xlen"), b.loc("n")),
                                b.err(b.app2(
                                    rt(b, "Schema.PrefixIssues"),
                                    b.loc("xlen"),
                                    schema_err(b, str_lit(b, "a missing key")),
                                )),
                                b.if_(
                                    b.bin(BinOp::Gt, b.loc("xlen"), b.loc("n")),
                                    b.err(b.app2(
                                        rt(b, "Schema.PrefixIssues"),
                                        b.loc("n"),
                                        schema_err(b, str_lit(b, "an unknown key")),
                                    )),
                                    b.match_(
                                        map_result_list(
                                            b,
                                            "$vt",
                                            b.loc("vfs"),
                                            "vf",
                                            "i",
                                            b.app(
                                                b.loc("vf"),
                                                b.app2(
                                                    rt(b, "Helper.list_at"),
                                                    b.var("i"),
                                                    b.loc("xs"),
                                                ),
                                            ),
                                            |idx, err| {
                                                b.app2(rt(b, "Schema.PrefixIssues"), idx, err)
                                            },
                                        ),
                                        vec![
                                            arm(
                                                b.ok_pat("out"),
                                                b.ok(b.app(
                                                    rt(b, "Schema.TupleFromList"),
                                                    b.loc("out"),
                                                )),
                                            ),
                                            arm(b.err_pat("e"), b.err(b.loc("e"))),
                                        ],
                                    ),
                                ),
                            ),
                        ),
                    ),
                    arm(
                        Pat::Wild,
                        expected_err(
                            b,
                            cat(
                                b,
                                str_lit(b, "a tuple of "),
                                b.app(b.global("Int", "to_string"), b.loc("n")),
                            ),
                            b.loc("input"),
                        ),
                    ),
                ],
            ),
        ),
    )
}

pub(super) fn validate_record_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["fields", "input"],
        b.match_(
            b.app(rt(b, "Schema.RecordEntries"), b.loc("input")),
            vec![
                arm(
                    b.some_pat("_ents"),
                    b.match_(
                        map_result_list(
                            b,
                            "$vr",
                            b.loc("fields"),
                            "fld",
                            "i",
                            b.match_(
                                b.loc("fld"),
                                vec![arm(
                                    pair_pat(b, "k", "s"),
                                    b.match_(
                                        b.app2(rt(b, "Schema.RecordLookup"), b.loc("input"), b.loc("k")),
                                        vec![
                                            arm(
                                                b.some_pat("fv"),
                                                b.match_(
                                                    b.app(b.app(rt(b, "Helper.schema_val"), b.loc("s")), b.loc("fv")),
                                                    vec![
                                                        arm(b.ok_pat("vv"), b.ok(b.tuple(vec![b.loc("k"), b.loc("vv")]))),
                                                        arm(b.err_pat("e"), b.err(b.app2(rt(b, "Schema.PrefixIssues"), b.loc("k"), b.loc("e")))),
                                                    ],
                                                ),
                                            ),
                                            arm(Pat::Wild, b.err(b.app2(rt(b, "Schema.PrefixIssues"), b.loc("k"), schema_err(b, str_lit(b, "a missing key"))))),
                                        ],
                                    ),
                                )],
                            ),
                            |_idx, err| err,
                        ),
                        vec![
                            arm(b.ok_pat("pairs"), b.ok(b.app(rt(b, "Schema.RecordFromSortedEntries"), b.loc("pairs")))),
                            arm(b.err_pat("e"), b.err(b.loc("e"))),
                        ],
                    ),
                ),
                arm(Pat::Wild, expected_err(b, str_lit(b, "a record"), b.loc("input"))),
            ],
        ),
    )
}

#[expect(clippy::too_many_lines, reason = "closed ADT validator AST template")]
pub(super) fn validate_adt_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["name", "cases", "input"],
        b.match_(
            b.app(rt(b, "Schema.DeconstructAdt"), b.loc("input")),
            vec![
                arm(
                    b.some_pat("df"),
                    b.match_(
                        b.loc("df"),
                        vec![arm(
                            pair_pat(b, "tag", "fvals"),
                            b.block(
                                vec![
                                    b.let_(
                                        "flen",
                                        b.app(b.global("List.Core", "length"), b.loc("fvals")),
                                    ),
                                    b.var_decl("found", b.none()),
                                    Stmt::Expr(b.for_ctrl(
                                        "$va",
                                        "c",
                                        b.loc("cases"),
                                        b.match_(
                                            b.loc("c"),
                                            vec![arm(
                                                triple_pat(b, "ft", "_sn", "it"),
                                                b.if_(
                                                    b.bin(
                                                        BinOp::And,
                                                        b.bin(BinOp::Eq, b.loc("ft"), b.loc("tag")),
                                                        b.bin(
                                                            BinOp::Eq,
                                                            b.app(
                                                                b.global("List.Core", "length"),
                                                                b.loc("it"),
                                                            ),
                                                            b.loc("flen"),
                                                        ),
                                                    ),
                                                    b.block(
                                                        vec![
                                                            b.assign("found", b.some(b.loc("it"))),
                                                        ],
                                                        b.break_("$va", b.unit()),
                                                    ),
                                                    b.unit(),
                                                ),
                                            )],
                                        ),
                                    )),
                                ],
                                b.match_(
                                    b.var("found"),
                                    vec![
                                        arm(
                                            b.some_pat("items"),
                                            b.match_(
                                                map_result_list(
                                                    b,
                                                    "$vaf",
                                                    b.loc("items"),
                                                    "fs",
                                                    "i",
                                                    b.app(
                                                        b.app(
                                                            rt(b, "Helper.schema_val"),
                                                            b.loc("fs"),
                                                        ),
                                                        b.app2(
                                                            rt(b, "Helper.list_at"),
                                                            b.var("i"),
                                                            b.loc("fvals"),
                                                        ),
                                                    ),
                                                    |idx, err| {
                                                        b.app2(
                                                            rt(b, "Schema.PrefixIssues"),
                                                            cat(
                                                                b,
                                                                str_lit(b, "_"),
                                                                b.app(
                                                                    b.global("Int", "to_string"),
                                                                    idx,
                                                                ),
                                                            ),
                                                            err,
                                                        )
                                                    },
                                                ),
                                                vec![
                                                    arm(
                                                        b.ok_pat("out"),
                                                        b.ok(b.app2(
                                                            rt(b, "Schema.ConstructAdt"),
                                                            b.loc("tag"),
                                                            b.loc("out"),
                                                        )),
                                                    ),
                                                    arm(b.err_pat("e"), b.err(b.loc("e"))),
                                                ],
                                            ),
                                        ),
                                        arm(
                                            Pat::Wild,
                                            expected_err(
                                                b,
                                                cat(b, str_lit(b, "a "), b.loc("name")),
                                                b.loc("input"),
                                            ),
                                        ),
                                    ],
                                ),
                            ),
                        )],
                    ),
                ),
                arm(
                    Pat::Wild,
                    expected_err(b, cat(b, str_lit(b, "a "), b.loc("name")), b.loc("input")),
                ),
            ],
        ),
    )
}
