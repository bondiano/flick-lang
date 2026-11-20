//! Synthetic `$rt.Helper.*` ADT wire codec helpers (`decode_adt`, `encode_adt`).

use flick_ir::BinOp;

use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::{
    cat, expected_err, map_result_list, pair_pat, rt, schema_err, str_lit, triple_pat,
};
use super::{Expr, Pat, Stmt};

#[expect(
    clippy::too_many_lines,
    reason = "closed ADT wire decoder AST template"
)]
pub(super) fn decode_adt_helper(b: &ExprBuilder) -> Expr {
    let mk_key_decs = b.block(
        vec![b.var_decl("fi", b.int(0))],
        b.app2(
            rt(b, "Helper.list_cons"),
            b.tuple(vec![str_lit(b, "_tag"), b.lam(&["tv"], b.ok(b.loc("tv")))]),
            b.gen_strict(b.each(
                "fs",
                b.loc("items"),
                b.block(
                    vec![
                        b.let_(
                            "fk",
                            cat(
                                b,
                                str_lit(b, "_"),
                                b.app(b.global("Int", "to_string"), b.var("fi")),
                            ),
                        ),
                        b.assign("fi", b.add_i(b.var("fi"), b.int(1))),
                    ],
                    b.yield_(b.tuple(vec![
                        b.loc("fk"),
                        b.app(
                            rt(b, "Helper.codec_dec"),
                            b.app(rt(b, "Helper.schema_codec"), b.loc("fs")),
                        ),
                    ])),
                ),
            )),
        ),
    );
    let append_name = b.if_(
        b.var("first"),
        b.block(
            vec![
                b.assign("first", b.bool_(false)),
                b.assign("acc", b.loc("sn")),
            ],
            b.unit(),
        ),
        b.block(
            vec![b.assign(
                "acc",
                cat(b, b.var("acc"), cat(b, str_lit(b, ", "), b.loc("sn"))),
            )],
            b.unit(),
        ),
    );
    let join_names = b.block(
        vec![
            b.var_decl("acc", str_lit(b, "")),
            b.var_decl("first", b.bool_(true)),
        ],
        b.block(
            vec![Stmt::Expr(b.each(
                "cs",
                b.loc("cases"),
                b.match_(
                    b.loc("cs"),
                    vec![arm(triple_pat(b, "_ft", "sn", "_it"), append_name)],
                ),
            ))],
            b.var("acc"),
        ),
    );
    let check_case = b.match_(
        b.loc("tag_opt"),
        vec![
            arm(
                b.some_pat("tv"),
                b.if_(
                    b.app2(rt(b, "Schema.SameValue"), b.loc("tv"), b.loc("sn")),
                    b.block(
                        vec![b.assign("found", b.some(b.tuple(vec![b.loc("ft"), b.loc("it")])))],
                        b.break_("$da", b.unit()),
                    ),
                    b.unit(),
                ),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let find_case = b.for_ctrl(
        "$da",
        "c",
        b.loc("cases"),
        b.match_(
            b.loc("c"),
            vec![arm(triple_pat(b, "ft", "sn", "it"), check_case)],
        ),
    );
    let extract_fvals = b.gen_strict(b.block(
        vec![b.var_decl("skip", b.bool_(true))],
        b.each(
            "kv",
            b.loc("kvs"),
            b.if_(
                b.var("skip"),
                b.block(vec![b.assign("skip", b.bool_(false))], b.unit()),
                b.match_(
                    b.loc("kv"),
                    vec![arm(pair_pat(b, "_k", "v"), b.yield_(b.loc("v")))],
                ),
            ),
        ),
    ));
    let on_kvs = b.block(
        vec![b.let_("fvals", extract_fvals)],
        b.ok(b.app2(rt(b, "Schema.ConstructAdt"), b.loc("ftag"), b.loc("fvals"))),
    );
    let decode_obj = b.match_(
        b.app3(
            rt(b, "Helper.decode_object"),
            mk_key_decs,
            b.loc("entries"),
            b.loc("input"),
        ),
        vec![
            arm(b.err_pat("e"), b.err(b.loc("e"))),
            arm(b.ok_pat("kvs"), on_kvs),
        ],
    );
    let on_found = b.match_(
        b.var("found"),
        vec![
            arm(
                b.some_pat("pair"),
                b.match_(
                    b.loc("pair"),
                    vec![arm(pair_pat(b, "ftag", "items"), decode_obj)],
                ),
            ),
            arm(
                Pat::Wild,
                b.err(b.app2(
                    rt(b, "Schema.PrefixIssues"),
                    str_lit(b, "_tag"),
                    schema_err(b, cat(b, str_lit(b, "expected one of "), join_names)),
                )),
            ),
        ],
    );
    let on_entries = b.block(
        vec![
            b.let_(
                "tag_opt",
                b.app2(
                    rt(b, "Schema.RecordLookup"),
                    b.loc("input"),
                    str_lit(b, "_tag"),
                ),
            ),
            b.var_decl("found", b.none()),
            Stmt::Expr(find_case),
        ],
        on_found,
    );
    b.lam(
        &["cases", "input"],
        b.match_(
            b.app(rt(b, "Schema.RecordEntries"), b.loc("input")),
            vec![
                arm(b.some_pat("entries"), on_entries),
                arm(
                    Pat::Wild,
                    expected_err(b, str_lit(b, "an object with a `_tag`"), b.loc("input")),
                ),
            ],
        ),
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "closed ADT wire encoder AST template"
)]
pub(super) fn encode_adt_helper(b: &ExprBuilder) -> Expr {
    let find_case = b.for_ctrl(
        "$ea",
        "c",
        b.loc("cases"),
        b.match_(
            b.loc("c"),
            vec![arm(
                triple_pat(b, "ft", "sn", "it"),
                b.if_(
                    b.bin(BinOp::Eq, b.loc("ft"), b.loc("tag")),
                    b.block(
                        vec![b.assign("found", b.some(b.tuple(vec![b.loc("sn"), b.loc("it")])))],
                        b.break_("$ea", b.unit()),
                    ),
                    b.unit(),
                ),
            )],
        ),
    );
    let encode_field = b.block(
        vec![b.let_(
            "fk",
            cat(
                b,
                str_lit(b, "_"),
                b.app(b.global("Int", "to_string"), b.var("i")),
            ),
        )],
        b.match_(
            b.app(
                b.app(
                    rt(b, "Helper.codec_enc"),
                    b.app(rt(b, "Helper.schema_codec"), b.loc("fs")),
                ),
                b.app2(rt(b, "Helper.list_at"), b.var("i"), b.loc("fvals")),
            ),
            vec![
                arm(b.ok_pat("w"), b.ok(b.tuple(vec![b.loc("fk"), b.loc("w")]))),
                arm(
                    b.err_pat("e"),
                    b.err(b.app2(rt(b, "Schema.PrefixIssues"), b.loc("fk"), b.loc("e"))),
                ),
            ],
        ),
    );
    let encode_items = b.match_(
        map_result_list(
            b,
            "$eaf",
            b.loc("items"),
            "fs",
            "i",
            encode_field,
            |_i, e| e,
        ),
        vec![
            arm(b.err_pat("e"), b.err(b.loc("e"))),
            arm(
                b.ok_pat("epairs"),
                b.ok(b.app(
                    rt(b, "Schema.RecordFromEntries"),
                    b.app2(
                        rt(b, "Helper.list_cons"),
                        b.tuple(vec![str_lit(b, "_tag"), b.loc("sn")]),
                        b.loc("epairs"),
                    ),
                )),
            ),
        ],
    );
    let bad_schema = b.app(
        rt(b, "Schema.Defect"),
        str_lit(b, "a value its schema does not describe"),
    );
    let on_found = b.match_(
        b.var("found"),
        vec![
            arm(
                b.some_pat("pair"),
                b.match_(
                    b.loc("pair"),
                    vec![arm(pair_pat(b, "sn", "items"), encode_items)],
                ),
            ),
            arm(Pat::Wild, bad_schema.clone()),
        ],
    );
    let on_deconstructed = b.block(
        vec![b.var_decl("found", b.none()), Stmt::Expr(find_case)],
        on_found,
    );
    b.lam(
        &["cases", "val"],
        b.match_(
            b.app(rt(b, "Schema.DeconstructAdt"), b.loc("val")),
            vec![
                arm(
                    b.some_pat("df"),
                    b.match_(
                        b.loc("df"),
                        vec![arm(pair_pat(b, "tag", "fvals"), on_deconstructed)],
                    ),
                ),
                arm(Pat::Wild, bad_schema),
            ],
        ),
    )
}
