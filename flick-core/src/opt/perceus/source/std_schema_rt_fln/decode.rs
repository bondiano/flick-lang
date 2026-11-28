//! Compound FLN decoder templates (list, tuple, option, result, record).

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{map_result_list, pair_pat, rt, schema_err, str_lit};
use super::super::{Expr, Pat, Stmt};
use super::fln_pat;

pub(super) fn decode_fln_list(b: &ExprBuilder) -> Expr {
    b.match_(
        b.loc("input"),
        vec![
            arm(
                fln_pat(b, "List", &["xs"]),
                map_result_list(
                    b,
                    "$dfl",
                    b.loc("xs"),
                    "x",
                    "i",
                    b.app2(rt(b, "Helper.decode_fln"), b.loc("it"), b.loc("x")),
                    |idx, err| b.app2(rt(b, "Schema.PrefixIssues"), idx, err),
                ),
            ),
            arm(
                Pat::Wild,
                b.err(schema_err(b, str_lit(b, "expected a list"))),
            ),
        ],
    )
}

pub(super) fn decode_fln_tuple(b: &ExprBuilder) -> Expr {
    b.match_(
        b.loc("input"),
        vec![
            arm(
                fln_pat(b, "Tuple", &["xs"]),
                b.block(
                    vec![
                        b.let_("wn", b.app(b.global("List.Core", "length"), b.loc("its"))),
                        b.let_("gn", b.app(b.global("List.Core", "length"), b.loc("xs"))),
                    ],
                    b.if_(
                        b.bin(BinOp::Lt, b.loc("gn"), b.loc("wn")),
                        b.err(b.app2(
                            rt(b, "Schema.PrefixIssues"),
                            b.loc("gn"),
                            schema_err(b, str_lit(b, "a missing tuple item")),
                        )),
                        b.if_(
                            b.bin(BinOp::Gt, b.loc("gn"), b.loc("wn")),
                            b.err(b.app2(
                                rt(b, "Schema.PrefixIssues"),
                                b.loc("wn"),
                                schema_err(b, str_lit(b, "an extra tuple item")),
                            )),
                            b.match_(
                                map_result_list(
                                    b,
                                    "$dft",
                                    b.loc("its"),
                                    "is",
                                    "i",
                                    b.app2(
                                        rt(b, "Helper.decode_fln"),
                                        b.loc("is"),
                                        b.app2(rt(b, "Helper.list_at"), b.var("i"), b.loc("xs")),
                                    ),
                                    |idx, err| b.app2(rt(b, "Schema.PrefixIssues"), idx, err),
                                ),
                                vec![
                                    arm(
                                        b.ok_pat("vals"),
                                        b.ok(b.app(rt(b, "Schema.TupleFromList"), b.loc("vals"))),
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
                b.err(schema_err(b, str_lit(b, "expected a tuple"))),
            ),
        ],
    )
}

pub(super) fn decode_fln_option(b: &ExprBuilder) -> Expr {
    b.match_(
        b.loc("input"),
        vec![
            arm(
                fln_pat(b, "Constructor", &["_", "cn", "args"]),
                b.block(
                    vec![b.let_("n", b.app(b.global("List.Core", "length"), b.loc("args")))],
                    b.if_(
                        b.bin(
                            BinOp::And,
                            b.bin(BinOp::Eq, b.loc("cn"), str_lit(b, "None")),
                            b.bin(BinOp::Eq, b.loc("n"), b.int(0)),
                        ),
                        b.ok(b.none()),
                        b.if_(
                            b.bin(
                                BinOp::And,
                                b.bin(BinOp::Eq, b.loc("cn"), str_lit(b, "Some")),
                                b.bin(BinOp::Eq, b.loc("n"), b.int(1)),
                            ),
                            b.match_(
                                b.app2(
                                    rt(b, "Helper.decode_fln"),
                                    b.loc("it"),
                                    b.app2(rt(b, "Helper.list_at"), b.int(0), b.loc("args")),
                                ),
                                vec![
                                    arm(b.ok_pat("v"), b.ok(b.some(b.loc("v")))),
                                    arm(b.err_pat("e"), b.err(b.loc("e"))),
                                ],
                            ),
                            b.err(schema_err(b, str_lit(b, "expected None or Some"))),
                        ),
                    ),
                ),
            ),
            arm(
                Pat::Wild,
                b.err(schema_err(b, str_lit(b, "expected None or Some"))),
            ),
        ],
    )
}

pub(super) fn decode_fln_result(b: &ExprBuilder) -> Expr {
    b.match_(
        b.loc("input"),
        vec![
            arm(
                fln_pat(b, "Constructor", &["_", "cn", "args"]),
                b.block(
                    vec![b.let_("n", b.app(b.global("List.Core", "length"), b.loc("args")))],
                    b.if_(
                        b.bin(
                            BinOp::And,
                            b.bin(BinOp::Eq, b.loc("cn"), str_lit(b, "Ok")),
                            b.bin(BinOp::Eq, b.loc("n"), b.int(1)),
                        ),
                        b.match_(
                            b.app2(
                                rt(b, "Helper.decode_fln"),
                                b.loc("os"),
                                b.app2(rt(b, "Helper.list_at"), b.int(0), b.loc("args")),
                            ),
                            vec![
                                arm(b.ok_pat("v"), b.ok(b.ok(b.loc("v")))),
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
                        b.if_(
                            b.bin(
                                BinOp::And,
                                b.bin(BinOp::Eq, b.loc("cn"), str_lit(b, "Err")),
                                b.bin(BinOp::Eq, b.loc("n"), b.int(1)),
                            ),
                            b.match_(
                                b.app2(
                                    rt(b, "Helper.decode_fln"),
                                    b.loc("es"),
                                    b.app2(rt(b, "Helper.list_at"), b.int(0), b.loc("args")),
                                ),
                                vec![
                                    arm(b.ok_pat("v"), b.ok(b.err(b.loc("v")))),
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
                            b.err(schema_err(b, str_lit(b, "expected Ok or Err"))),
                        ),
                    ),
                ),
            ),
            arm(
                Pat::Wild,
                b.err(schema_err(b, str_lit(b, "expected Ok or Err"))),
            ),
        ],
    )
}

pub(super) fn find_pair(b: &ExprBuilder, loop_id: &str, pairs: Expr, key: Expr) -> Expr {
    b.block(
        vec![
            b.var_decl("found", b.none()),
            Stmt::Expr(b.for_ctrl(
                loop_id,
                "p",
                pairs,
                b.match_(
                    b.loc("p"),
                    vec![arm(
                        pair_pat(b, "pk", "pv"),
                        b.if_(
                            b.bin(BinOp::Eq, b.loc("pk"), key),
                            b.block(
                                vec![b.assign("found", b.some(b.loc("pv")))],
                                b.break_(loop_id, b.unit()),
                            ),
                            b.unit(),
                        ),
                    )],
                ),
            )),
        ],
        b.var("found"),
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "closed FLN record decoder AST template"
)]
pub(super) fn decode_fln_record(b: &ExprBuilder) -> Expr {
    let field_step = b.match_(
        b.loc("fld"),
        vec![arm(
            pair_pat(b, "k", "fs"),
            b.match_(
                find_pair(b, "$frl", b.loc("pairs"), b.loc("k")),
                vec![
                    arm(
                        b.some_pat("fv"),
                        b.match_(
                            b.app2(rt(b, "Helper.decode_fln"), b.loc("fs"), b.loc("fv")),
                            vec![
                                arm(b.ok_pat("dv"), b.ok(b.tuple(vec![b.loc("k"), b.loc("dv")]))),
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
    );
    let check_unknown = b.block(
        vec![
            b.var_decl("unk", b.none()),
            Stmt::Expr(b.for_ctrl(
                "$fru",
                "p",
                b.loc("pairs"),
                b.match_(
                    b.loc("p"),
                    vec![arm(
                        pair_pat(b, "pk", "_pv"),
                        b.match_(
                            find_pair(b, "$frk", b.loc("flds"), b.loc("pk")),
                            vec![
                                arm(b.some_pat("_"), b.unit()),
                                arm(
                                    Pat::Wild,
                                    b.block(
                                        vec![b.assign("unk", b.some(b.loc("pk")))],
                                        b.break_("$fru", b.unit()),
                                    ),
                                ),
                            ],
                        ),
                    )],
                ),
            )),
        ],
        b.var("unk"),
    );
    b.match_(
        b.loc("input"),
        vec![
            arm(
                fln_pat(b, "Record", &["pairs"]),
                b.match_(
                    map_result_list(b, "$dfr", b.loc("flds"), "fld", "_i", field_step, |_, e| e),
                    vec![
                        arm(
                            b.ok_pat("ents"),
                            b.match_(
                                check_unknown,
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
                                        b.ok(b.app(
                                            rt(b, "Schema.RecordFromSortedEntries"),
                                            b.loc("ents"),
                                        )),
                                    ),
                                ],
                            ),
                        ),
                        arm(b.err_pat("e"), b.err(b.loc("e"))),
                    ],
                ),
            ),
            arm(
                Pat::Wild,
                b.err(schema_err(b, str_lit(b, "expected a record"))),
            ),
        ],
    )
}
