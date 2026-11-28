//! Closed Schema FLN encoder AST template.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{
    cat, map_result_list, pair_pat, rt, schema_err, str_lit, triple_pat,
};
use super::super::{Expr, Pat, Stmt};
use super::adt::{encode_fln_enum, encode_fln_enum_tags};
use super::{fln_con, kc};

#[expect(
    clippy::too_many_lines,
    reason = "closed Schema FLN encoder AST template"
)]
pub(in super::super) fn encode_fln_helper(b: &ExprBuilder) -> Expr {
    let leaf = |k: &str| b.app2(rt(b, "Schema.EncodeLeaf"), str_lit(b, k), b.loc("val"));
    let encode_list = b.match_(
        map_result_list(
            b,
            "$efl",
            b.loc("val"),
            "x",
            "i",
            b.app2(rt(b, "Helper.encode_fln"), b.loc("it"), b.loc("x")),
            |idx, err| b.app2(rt(b, "Schema.PrefixIssues"), idx, err),
        ),
        vec![
            arm(b.ok_pat("xs"), b.ok(fln_con(b, "List", vec![b.loc("xs")]))),
            arm(b.err_pat("e"), b.err(b.loc("e"))),
        ],
    );
    let encode_tuple = b.match_(
        b.app(rt(b, "Schema.TupleToList"), b.loc("val")),
        vec![
            arm(
                b.some_pat("xs"),
                b.match_(
                    map_result_list(
                        b,
                        "$eft",
                        b.loc("its"),
                        "is",
                        "i",
                        b.app2(
                            rt(b, "Helper.encode_fln"),
                            b.loc("is"),
                            b.app2(rt(b, "Helper.list_at"), b.var("i"), b.loc("xs")),
                        ),
                        |idx, err| b.app2(rt(b, "Schema.PrefixIssues"), idx, err),
                    ),
                    vec![
                        arm(b.ok_pat("es"), b.ok(fln_con(b, "Tuple", vec![b.loc("es")]))),
                        arm(b.err_pat("e"), b.err(b.loc("e"))),
                    ],
                ),
            ),
            arm(
                Pat::Wild,
                b.err(schema_err(b, str_lit(b, "expected a tuple"))),
            ),
        ],
    );
    let encode_option = b.match_(
        b.loc("val"),
        vec![
            arm(
                b.some_pat("x"),
                b.match_(
                    b.app2(rt(b, "Helper.encode_fln"), b.loc("it"), b.loc("x")),
                    vec![
                        arm(
                            b.ok_pat("w"),
                            b.ok(fln_con(
                                b,
                                "Constructor",
                                vec![b.list(vec![]), str_lit(b, "Some"), b.list(vec![b.loc("w")])],
                            )),
                        ),
                        arm(b.err_pat("e"), b.err(b.loc("e"))),
                    ],
                ),
            ),
            arm(
                Pat::Wild,
                b.ok(fln_con(
                    b,
                    "Constructor",
                    vec![b.list(vec![]), str_lit(b, "None"), b.list(vec![])],
                )),
            ),
        ],
    );
    let encode_result = b.match_(
        b.loc("val"),
        vec![
            arm(
                b.ok_pat("x"),
                b.match_(
                    b.app2(rt(b, "Helper.encode_fln"), b.loc("os"), b.loc("x")),
                    vec![
                        arm(
                            b.ok_pat("w"),
                            b.ok(fln_con(
                                b,
                                "Constructor",
                                vec![b.list(vec![]), str_lit(b, "Ok"), b.list(vec![b.loc("w")])],
                            )),
                        ),
                        arm(
                            b.err_pat("e"),
                            b.err(b.app2(
                                rt(b, "Schema.PrefixIssues"),
                                str_lit(b, "ok"),
                                b.loc("e"),
                            )),
                        ),
                    ],
                ),
            ),
            arm(
                b.err_pat("x"),
                b.match_(
                    b.app2(rt(b, "Helper.encode_fln"), b.loc("es"), b.loc("x")),
                    vec![
                        arm(
                            b.ok_pat("w"),
                            b.ok(fln_con(
                                b,
                                "Constructor",
                                vec![b.list(vec![]), str_lit(b, "Err"), b.list(vec![b.loc("w")])],
                            )),
                        ),
                        arm(
                            b.err_pat("e"),
                            b.err(b.app2(
                                rt(b, "Schema.PrefixIssues"),
                                str_lit(b, "err"),
                                b.loc("e"),
                            )),
                        ),
                    ],
                ),
            ),
        ],
    );
    let encode_record = b.match_(
        map_result_list(
            b,
            "$efr",
            b.loc("flds"),
            "fld",
            "_i",
            b.match_(
                b.loc("fld"),
                vec![arm(
                    pair_pat(b, "k", "fs"),
                    b.match_(
                        b.app2(rt(b, "Schema.RecordLookup"), b.loc("val"), b.loc("k")),
                        vec![
                            arm(
                                b.some_pat("fv"),
                                b.match_(
                                    b.app2(rt(b, "Helper.encode_fln"), b.loc("fs"), b.loc("fv")),
                                    vec![
                                        arm(
                                            b.ok_pat("w"),
                                            b.ok(b.tuple(vec![b.loc("k"), b.loc("w")])),
                                        ),
                                        arm(
                                            b.err_pat("e"),
                                            b.err(b.app2(
                                                rt(b, "Schema.PrefixIssues"),
                                                b.loc("k"),
                                                b.loc("e"),
                                            )),
                                        ),
                                    ],
                                ),
                            ),
                            arm(
                                Pat::Wild,
                                b.err(b.app2(
                                    rt(b, "Schema.PrefixIssues"),
                                    b.loc("k"),
                                    schema_err(b, str_lit(b, "a missing key")),
                                )),
                            ),
                        ],
                    ),
                )],
            ),
            |_, e| e,
        ),
        vec![
            arm(
                b.ok_pat("ps"),
                b.ok(fln_con(b, "Record", vec![b.loc("ps")])),
            ),
            arm(b.err_pat("e"), b.err(b.loc("e"))),
        ],
    );
    let encode_adt = b.match_(
        b.app(rt(b, "Schema.DeconstructAdt"), b.loc("val")),
        vec![
            arm(
                b.some_pat("decon"),
                b.match_(
                    b.loc("decon"),
                    vec![arm(
                        pair_pat(b, "tag", "fvals"),
                        b.block(
                            vec![
                                b.var_decl("hit", b.none()),
                                Stmt::Expr(b.for_ctrl(
                                    "$fea",
                                    "c",
                                    b.loc("cases"),
                                    b.match_(
                                        b.loc("c"),
                                        vec![arm(
                                            triple_pat(b, "ft", "sn", "fschemas"),
                                            b.if_(
                                                b.bin(BinOp::Eq, b.loc("ft"), b.loc("tag")),
                                                b.block(
                                                    vec![b.assign(
                                                        "hit",
                                                        b.some(b.tuple(vec![
                                                            b.loc("sn"),
                                                            b.loc("fschemas"),
                                                        ])),
                                                    )],
                                                    b.break_("$fea", b.unit()),
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
                                        b.some_pat("pair"),
                                        b.match_(
                                            b.loc("pair"),
                                            vec![arm(
                                                pair_pat(b, "sn", "fschemas"),
                                                b.match_(
                                                    map_result_list(
                                                        b,
                                                        "$efaa",
                                                        b.loc("fschemas"),
                                                        "fs",
                                                        "i",
                                                        b.app2(
                                                            rt(b, "Helper.encode_fln"),
                                                            b.loc("fs"),
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
                                                                        rt(
                                                                            b,
                                                                            "Schema.RouteJsString",
                                                                        ),
                                                                        idx,
                                                                    ),
                                                                ),
                                                                err,
                                                            )
                                                        },
                                                    ),
                                                    vec![
                                                        arm(
                                                            b.ok_pat("args"),
                                                            b.ok(fln_con(
                                                                b,
                                                                "Constructor",
                                                                vec![
                                                                    b.list(vec![]),
                                                                    b.loc("sn"),
                                                                    b.loc("args"),
                                                                ],
                                                            )),
                                                        ),
                                                        arm(b.err_pat("e"), b.err(b.loc("e"))),
                                                    ],
                                                ),
                                            )],
                                        ),
                                    ),
                                    arm(
                                        Pat::Wild,
                                        b.err(schema_err(b, str_lit(b, "expected a variant"))),
                                    ),
                                ],
                            ),
                        ),
                    )],
                ),
            ),
            arm(
                Pat::Wild,
                b.err(schema_err(b, str_lit(b, "expected a variant"))),
            ),
        ],
    );
    b.lam(
        &["sch", "val"],
        b.match_(
            b.app(rt(b, "Helper.schema_kind"), b.loc("sch")),
            vec![
                arm(Pat::Con(kc("$Unit", 0), vec![]), leaf("fln_unit")),
                arm(Pat::Con(kc("$Bool", 0), vec![]), leaf("fln_bool")),
                arm(Pat::Con(kc("$String", 0), vec![]), leaf("fln_string")),
                arm(Pat::Con(kc("$Int", 0), vec![]), leaf("fln_int")),
                arm(Pat::Con(kc("$Int8", 0), vec![]), leaf("fln_int8")),
                arm(Pat::Con(kc("$Int16", 0), vec![]), leaf("fln_int16")),
                arm(Pat::Con(kc("$Int64", 0), vec![]), leaf("fln_int64")),
                arm(Pat::Con(kc("$UInt8", 0), vec![]), leaf("fln_uint8")),
                arm(Pat::Con(kc("$UInt16", 0), vec![]), leaf("fln_uint16")),
                arm(Pat::Con(kc("$UInt32", 0), vec![]), leaf("fln_uint32")),
                arm(Pat::Con(kc("$UInt64", 0), vec![]), leaf("fln_uint64")),
                arm(Pat::Con(kc("$BigInt", 0), vec![]), leaf("fln_bigint")),
                arm(Pat::Con(kc("$Number", 0), vec![]), leaf("fln_number")),
                arm(Pat::Con(kc("$Float32", 0), vec![]), leaf("fln_float32")),
                arm(
                    Pat::Con(kc("$Never", 0), vec![]),
                    b.err(schema_err(b, str_lit(b, "expected nothing (Never)"))),
                ),
                arm(Pat::Con(kc("$Unknown", 0), vec![]), b.ok(b.loc("val"))),
                arm(
                    Pat::Con(kc("$Redacted", 1), vec![Pat::Wild]),
                    b.err(schema_err(
                        b,
                        str_lit(
                            b,
                            "a Redacted value has no FLN form without an explicit policy",
                        ),
                    )),
                ),
                arm(
                    Pat::Con(kc("$Lazy", 2), vec![Pat::Wild, Pat::Bind(b.name("th"))]),
                    b.app2(
                        rt(b, "Helper.encode_fln"),
                        b.app(b.loc("th"), b.unit()),
                        b.loc("val"),
                    ),
                ),
                arm(
                    Pat::Con(
                        kc("$Refined", 3),
                        vec![
                            Pat::Bind(b.name("nm")),
                            Pat::Bind(b.name("bs")),
                            Pat::Bind(b.name("chk")),
                        ],
                    ),
                    b.if_(
                        b.app(b.loc("chk"), b.loc("val")),
                        b.app2(rt(b, "Helper.encode_fln"), b.loc("bs"), b.loc("val")),
                        b.err(schema_err(
                            b,
                            cat(b, str_lit(b, "not a valid "), b.loc("nm")),
                        )),
                    ),
                ),
                arm(
                    Pat::Con(kc("$List", 1), vec![Pat::Bind(b.name("it"))]),
                    encode_list,
                ),
                arm(
                    Pat::Con(kc("$Tuple", 1), vec![Pat::Bind(b.name("its"))]),
                    encode_tuple,
                ),
                arm(
                    Pat::Con(kc("$Option", 1), vec![Pat::Bind(b.name("it"))]),
                    encode_option,
                ),
                arm(
                    Pat::Con(
                        kc("$Result", 2),
                        vec![Pat::Bind(b.name("es")), Pat::Bind(b.name("os"))],
                    ),
                    encode_result,
                ),
                arm(
                    Pat::Con(kc("$Record", 1), vec![Pat::Bind(b.name("flds"))]),
                    encode_record,
                ),
                arm(
                    Pat::Con(kc("$Adt", 2), vec![Pat::Wild, Pat::Bind(b.name("cases"))]),
                    encode_adt,
                ),
                arm(
                    Pat::Con(kc("$Enum", 1), vec![Pat::Bind(b.name("vals"))]),
                    encode_fln_enum(b),
                ),
                arm(
                    Pat::Con(kc("$EnumTags", 1), vec![Pat::Bind(b.name("pairs"))]),
                    encode_fln_enum_tags(b),
                ),
                arm(
                    Pat::Wild,
                    b.err(schema_err(
                        b,
                        str_lit(b, "schema has no canonical FLN form"),
                    )),
                ),
            ],
        ),
    )
}
