//! Synthetic `$rt.Helper.*` tuple, record, and ADT wire codec helpers.

use flick_ir::BinOp;

use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::{expected_err, map_result_list, pair_pat, rt, schema_err, str_lit};
use super::{Expr, Pat, Stmt};

pub(super) fn decode_tuple_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["decs", "input"],
        b.match_(
            b.app(rt(b, "Schema.ListFromWire"), b.loc("input")),
            vec![
                arm(
                    b.some_pat("xs"),
                    b.block(
                        vec![b.let_("xlen", b.app(b.global("List.Core", "length"), b.loc("xs")))],
                        b.match_(
                            map_result_list(
                                b,
                                "$dt",
                                b.loc("decs"),
                                "dec",
                                "i",
                                b.if_(
                                    b.bin(BinOp::Ge, b.var("i"), b.loc("xlen")),
                                    b.err(schema_err(b, str_lit(b, "a missing key"))),
                                    b.app(
                                        b.loc("dec"),
                                        b.app2(rt(b, "Helper.list_at"), b.var("i"), b.loc("xs")),
                                    ),
                                ),
                                |idx, err| b.app2(rt(b, "Schema.PrefixIssues"), idx, err),
                            ),
                            vec![
                                arm(b.err_pat("e"), b.err(b.loc("e"))),
                                arm(
                                    b.ok_pat("vals"),
                                    b.block(
                                        vec![b.let_(
                                            "dlen",
                                            b.app(b.global("List.Core", "length"), b.loc("decs")),
                                        )],
                                        b.if_(
                                            b.bin(BinOp::Gt, b.loc("xlen"), b.loc("dlen")),
                                            b.err(b.app2(
                                                rt(b, "Schema.PrefixIssues"),
                                                b.loc("dlen"),
                                                schema_err(b, str_lit(b, "an unknown key")),
                                            )),
                                            b.ok(
                                                b.app(rt(b, "Schema.TupleFromList"), b.loc("vals"))
                                            ),
                                        ),
                                    ),
                                ),
                            ],
                        ),
                    ),
                ),
                arm(
                    Pat::Wild,
                    expected_err(b, str_lit(b, "an array"), b.loc("input")),
                ),
            ],
        ),
    )
}

pub(super) fn encode_tuple_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["encs", "tup"],
        b.match_(
            b.app(rt(b, "Schema.TupleToList"), b.loc("tup")),
            vec![
                arm(
                    b.some_pat("xs"),
                    map_result_list(
                        b,
                        "$et",
                        b.loc("encs"),
                        "enc",
                        "i",
                        b.app(
                            b.loc("enc"),
                            b.app2(rt(b, "Helper.list_at"), b.var("i"), b.loc("xs")),
                        ),
                        |idx, err| b.app2(rt(b, "Schema.PrefixIssues"), idx, err),
                    ),
                ),
                arm(
                    Pat::Wild,
                    b.app(rt(b, "Schema.Defect"), str_lit(b, "expected a tuple")),
                ),
            ],
        ),
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "closed object field decoder AST template"
)]
pub(super) fn decode_object_helper(b: &ExprBuilder) -> Expr {
    // key_decs: List (String, Unknown -> Result SchemaError a), entries: List (String, Unknown), obj: Record
    b.lam(
        &["key_decs", "entries", "obj"],
        b.block(
            vec![
                b.var_decl("unk", b.none()),
                Stmt::Expr(b.for_ctrl(
                    "$du",
                    "ent",
                    b.loc("entries"),
                    b.match_(
                        b.loc("ent"),
                        vec![arm(
                            pair_pat(b, "ek", "_ev"),
                            b.block(
                                vec![
                                    b.var_decl("known", b.bool_(false)),
                                    Stmt::Expr(b.for_ctrl(
                                        "$dk",
                                        "kd",
                                        b.loc("key_decs"),
                                        b.match_(
                                            b.loc("kd"),
                                            vec![arm(
                                                pair_pat(b, "wk", "_wd"),
                                                b.if_(
                                                    b.bin(BinOp::Eq, b.loc("wk"), b.loc("ek")),
                                                    b.block(
                                                        vec![b.assign("known", b.bool_(true))],
                                                        b.break_("$dk", b.unit()),
                                                    ),
                                                    b.unit(),
                                                ),
                                            )],
                                        ),
                                    )),
                                ],
                                b.if_(
                                    b.not(b.var("known")),
                                    b.block(
                                        vec![b.assign("unk", b.some(b.loc("ek")))],
                                        b.break_("$du", b.unit()),
                                    ),
                                    b.unit(),
                                ),
                            ),
                        )],
                    ),
                )),
            ],
            b.match_(
                b.var("unk"),
                vec![
                    arm(
                        b.some_pat("uk"),
                        b.err(b.app2(
                            rt(b, "Schema.PrefixIssues"),
                            b.loc("uk"),
                            schema_err(b, str_lit(b, "an unknown key")),
                        )),
                    ),
                    arm(
                        Pat::Wild,
                        map_result_list(
                            b,
                            "$do",
                            b.loc("key_decs"),
                            "kd",
                            "i",
                            b.match_(
                                b.loc("kd"),
                                vec![arm(
                                    pair_pat(b, "k", "dec"),
                                    b.match_(
                                        b.app2(
                                            rt(b, "Schema.RecordLookup"),
                                            b.loc("obj"),
                                            b.loc("k"),
                                        ),
                                        vec![
                                            arm(
                                                b.some_pat("fv"),
                                                b.match_(
                                                    b.app(b.loc("dec"), b.loc("fv")),
                                                    vec![
                                                        arm(
                                                            b.ok_pat("dv"),
                                                            b.ok(b.tuple(vec![
                                                                b.loc("k"),
                                                                b.loc("dv"),
                                                            ])),
                                                        ),
                                                        arm(
                                                            b.err_pat("fe"),
                                                            b.err(b.app2(
                                                                rt(b, "Schema.PrefixIssues"),
                                                                b.loc("k"),
                                                                b.loc("fe"),
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
                            |_idx, err| err,
                        ),
                    ),
                ],
            ),
        ),
    )
}

pub(super) fn encode_record_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["field_encs", "rec"],
        b.match_(
            map_result_list(
                b,
                "$er",
                b.loc("field_encs"),
                "fe",
                "i",
                b.match_(
                    b.loc("fe"),
                    vec![arm(
                        pair_pat(b, "k", "enc"),
                        b.match_(
                            b.app2(rt(b, "Schema.RecordLookup"), b.loc("rec"), b.loc("k")),
                            vec![
                                arm(
                                    b.some_pat("v"),
                                    b.match_(
                                        b.app(b.loc("enc"), b.loc("v")),
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
                                    b.app(
                                        rt(b, "Schema.Defect"),
                                        str_lit(b, "missing record field"),
                                    ),
                                ),
                            ],
                        ),
                    )],
                ),
                |_idx, err| err,
            ),
            vec![
                arm(
                    b.ok_pat("pairs"),
                    b.ok(b.app(rt(b, "Schema.RecordFromSortedEntries"), b.loc("pairs"))),
                ),
                arm(b.err_pat("e"), b.err(b.loc("e"))),
            ],
        ),
    )
}
