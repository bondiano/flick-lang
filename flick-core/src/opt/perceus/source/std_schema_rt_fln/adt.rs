//! ADT and enum FLN decoder/encoder templates.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{
    cat, map_result_list, pair_pat, rt, schema_err, str_lit, triple_pat,
};
use super::super::{Expr, Pat, Stmt};
use super::{fln_con, fln_pat};

#[expect(clippy::too_many_lines, reason = "closed FLN ADT decoder AST template")]
pub(super) fn decode_fln_adt(b: &ExprBuilder) -> Expr {
    let find_case = b.block(
        vec![
            b.var_decl("hit", b.none()),
            Stmt::Expr(b.for_ctrl(
                "$fda",
                "c",
                b.loc("cases"),
                b.match_(
                    b.loc("c"),
                    vec![arm(
                        triple_pat(b, "ft", "sn", "fschemas"),
                        b.if_(
                            b.bin(BinOp::Eq, b.loc("sn"), b.loc("cn")),
                            b.block(
                                vec![b.assign(
                                    "hit",
                                    b.some(b.tuple(vec![b.loc("ft"), b.loc("fschemas")])),
                                )],
                                b.break_("$fda", b.unit()),
                            ),
                            b.unit(),
                        ),
                    )],
                ),
            )),
        ],
        b.var("hit"),
    );
    b.match_(
        b.loc("input"),
        vec![
            arm(
                fln_pat(b, "Constructor", &["_", "cn", "args"]),
                b.match_(
                    find_case,
                    vec![
                        arm(
                            b.some_pat("pair"),
                            b.match_(
                                b.loc("pair"),
                                vec![arm(
                                    pair_pat(b, "ft", "fschemas"),
                                    b.block(
                                        vec![
                                            b.let_(
                                                "wn",
                                                b.app(
                                                    b.global("List.Core", "length"),
                                                    b.loc("fschemas"),
                                                ),
                                            ),
                                            b.let_(
                                                "gn",
                                                b.app(
                                                    b.global("List.Core", "length"),
                                                    b.loc("args"),
                                                ),
                                            ),
                                        ],
                                        b.if_(
                                            b.bin(BinOp::NotEq, b.loc("wn"), b.loc("gn")),
                                            b.err(schema_err(
                                                b,
                                                cat(
                                                    b,
                                                    str_lit(b, "wrong number of arguments for "),
                                                    b.loc("cn"),
                                                ),
                                            )),
                                            b.match_(
                                                map_result_list(
                                                    b,
                                                    "$dfaa",
                                                    b.loc("fschemas"),
                                                    "fs",
                                                    "i",
                                                    b.app2(
                                                        rt(b, "Helper.decode_fln"),
                                                        b.loc("fs"),
                                                        b.app2(
                                                            rt(b, "Helper.list_at"),
                                                            b.var("i"),
                                                            b.loc("args"),
                                                        ),
                                                    ),
                                                    |idx, err| {
                                                        b.app2(
                                                            rt(b, "Schema.PrefixIssues"),
                                                            cat(
                                                                b,
                                                                str_lit(b, "_"),
                                                                b.app(
                                                                    rt(b, "Schema.RouteJsString"),
                                                                    idx,
                                                                ),
                                                            ),
                                                            err,
                                                        )
                                                    },
                                                ),
                                                vec![
                                                    arm(
                                                        b.ok_pat("vals"),
                                                        b.ok(b.app2(
                                                            rt(b, "Schema.ConstructAdt"),
                                                            b.loc("ft"),
                                                            b.loc("vals"),
                                                        )),
                                                    ),
                                                    arm(b.err_pat("e"), b.err(b.loc("e"))),
                                                ],
                                            ),
                                        ),
                                    ),
                                )],
                            ),
                        ),
                        arm(
                            Pat::Wild,
                            b.err(schema_err(
                                b,
                                cat(b, str_lit(b, "unknown constructor "), b.loc("cn")),
                            )),
                        ),
                    ],
                ),
            ),
            arm(
                Pat::Wild,
                b.err(schema_err(b, str_lit(b, "expected a constructor"))),
            ),
        ],
    )
}

pub(super) fn decode_fln_enum(b: &ExprBuilder) -> Expr {
    b.match_(
        b.app2(
            rt(b, "Schema.DecodeLeaf"),
            str_lit(b, "fln_literal"),
            b.loc("input"),
        ),
        vec![
            arm(
                b.ok_pat("raw"),
                b.block(
                    vec![
                        b.var_decl("hit", b.none()),
                        Stmt::Expr(b.for_ctrl(
                            "$dfe",
                            "v",
                            b.loc("vals"),
                            b.if_(
                                b.app2(rt(b, "Schema.SameValue"), b.loc("v"), b.loc("raw")),
                                b.block(
                                    vec![b.assign("hit", b.some(b.loc("v")))],
                                    b.break_("$dfe", b.unit()),
                                ),
                                b.unit(),
                            ),
                        )),
                    ],
                    b.match_(
                        b.var("hit"),
                        vec![
                            arm(b.some_pat("matched"), b.ok(b.loc("matched"))),
                            arm(
                                Pat::Wild,
                                b.err(schema_err(b, str_lit(b, "expected a valid enum literal"))),
                            ),
                        ],
                    ),
                ),
            ),
            arm(
                Pat::Wild,
                b.err(schema_err(b, str_lit(b, "expected a valid enum literal"))),
            ),
        ],
    )
}

pub(super) fn decode_fln_enum_tags(b: &ExprBuilder) -> Expr {
    let find_pair = b.block(
        vec![
            b.var_decl("hit", b.none()),
            Stmt::Expr(b.for_ctrl(
                "$dfet",
                "p",
                b.loc("pairs"),
                b.match_(
                    b.loc("p"),
                    vec![arm(
                        pair_pat(b, "n", "v"),
                        b.if_(
                            b.bin(BinOp::Eq, b.loc("n"), b.loc("cn")),
                            b.block(
                                vec![b.assign("hit", b.some(b.loc("v")))],
                                b.break_("$dfet", b.unit()),
                            ),
                            b.unit(),
                        ),
                    )],
                ),
            )),
        ],
        b.var("hit"),
    );
    b.match_(
        b.loc("input"),
        vec![
            arm(
                fln_pat(b, "Constructor", &["_", "cn", "args"]),
                b.if_(
                    b.bin(
                        BinOp::Eq,
                        b.app(b.global("List.Core", "length"), b.loc("args")),
                        b.int(0),
                    ),
                    b.match_(
                        find_pair,
                        vec![
                            arm(b.some_pat("v"), b.ok(b.loc("v"))),
                            arm(
                                Pat::Wild,
                                b.err(schema_err(
                                    b,
                                    cat(b, str_lit(b, "unknown enum constructor "), b.loc("cn")),
                                )),
                            ),
                        ],
                    ),
                    b.err(schema_err(
                        b,
                        str_lit(b, "enum constructor takes no arguments"),
                    )),
                ),
            ),
            arm(
                Pat::Wild,
                b.err(schema_err(b, str_lit(b, "expected an enum constructor"))),
            ),
        ],
    )
}

pub(super) fn encode_fln_enum(b: &ExprBuilder) -> Expr {
    b.if_(
        b.app2(rt(b, "Schema.LiteralAccepts"), b.loc("vals"), b.loc("val")),
        b.app2(
            rt(b, "Schema.EncodeLeaf"),
            str_lit(b, "fln_literal"),
            b.loc("val"),
        ),
        b.err(schema_err(b, str_lit(b, "expected a valid enum literal"))),
    )
}

pub(super) fn encode_fln_enum_tags(b: &ExprBuilder) -> Expr {
    b.block(
        vec![
            b.var_decl("hit", b.none()),
            Stmt::Expr(b.for_ctrl(
                "$efet",
                "p",
                b.loc("pairs"),
                b.match_(
                    b.loc("p"),
                    vec![arm(
                        pair_pat(b, "n", "pv"),
                        b.if_(
                            b.app2(rt(b, "Schema.SameValue"), b.loc("pv"), b.loc("val")),
                            b.block(
                                vec![b.assign("hit", b.some(b.loc("n")))],
                                b.break_("$efet", b.unit()),
                            ),
                            b.unit(),
                        ),
                    )],
                ),
            )),
        ],
        b.match_(
            b.var("hit"),
            vec![
                arm(
                    b.some_pat("n"),
                    b.ok(fln_con(
                        b,
                        "Constructor",
                        vec![b.list(vec![]), b.loc("n"), b.list(vec![])],
                    )),
                ),
                arm(
                    Pat::Wild,
                    b.err(schema_err(b, str_lit(b, "expected a valid enum value"))),
                ),
            ],
        ),
    )
}
