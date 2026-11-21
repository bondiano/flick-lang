//! Synthetic `$rt.Helper.make_block_codec` helper for `codec` blocks.

use flick_ir::BinOp;

use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::{
    cat, expected_err, extra_con, make_codec, map_result_list, meta_con, pair_pat, rt, schema_err,
    str_lit,
};
use super::std_schema_rt_more::contains_str;
use super::{Expr, Pat, Stmt};

#[expect(clippy::too_many_lines, reason = "closed codec block AST template")]
pub(super) fn make_block_codec_helper(b: &ExprBuilder) -> Expr {
    let fld_pat = |w: &str, c: &str, opt: &str, om: &str| {
        Pat::Con(
            ExprBuilder::con("Codec", "$Field", "$Field", 4),
            vec![
                Pat::Bind(b.name(w)),
                Pat::Bind(b.name(c)),
                Pat::Bind(b.name(opt)),
                Pat::Bind(b.name(om)),
            ],
        )
    };
    let dup_msg = cat(
        b,
        str_lit(b, "two fields of the codec of "),
        cat(
            b,
            b.loc("name"),
            cat(
                b,
                str_lit(b, " use the wire name \""),
                cat(b, b.loc("w"), str_lit(b, "\"")),
            ),
        ),
    );
    let dup_step = b.if_(
        contains_str(b, "$bdc", b.var("seen_w"), b.loc("w")),
        b.app(rt(b, "Schema.Defect"), dup_msg),
        b.block(
            vec![b.assign(
                "seen_w",
                b.app2(rt(b, "Helper.list_cons"), b.loc("w"), b.var("seen_w")),
            )],
            b.unit(),
        ),
    );
    let dup_check = b.block(
        vec![b.var_decl("seen_w", b.list(vec![]))],
        b.each(
            "bf",
            b.loc("fields"),
            b.match_(
                b.loc("bf"),
                vec![arm(
                    pair_pat(b, "_lbl", "spec"),
                    b.match_(
                        b.loc("spec"),
                        vec![arm(fld_pat("w", "_c", "_o", "_m"), dup_step)],
                    ),
                )],
            ),
        ),
    );
    let unk_step = b.if_(
        b.not(contains_str(b, "$buk", b.loc("wires"), b.loc("ek"))),
        b.block(
            vec![b.assign("unk", b.some(b.loc("ek")))],
            b.break_("$bu", b.unit()),
        ),
        b.unit(),
    );
    let unk_check = b.block(
        vec![
            b.var_decl("unk", b.none()),
            Stmt::Expr(b.if_(
                b.not(b.loc("ignore_extra")),
                b.for_ctrl(
                    "$bu",
                    "ent",
                    b.loc("entries"),
                    b.match_(b.loc("ent"), vec![arm(pair_pat(b, "ek", "_ev"), unk_step)]),
                ),
                b.unit(),
            )),
        ],
        b.var("unk"),
    );
    let dec_present = b.match_(
        b.app(b.app(rt(b, "Helper.codec_dec"), b.loc("c")), b.loc("fv")),
        vec![
            arm(
                b.ok_pat("dv"),
                b.ok(b.tuple(vec![
                    b.loc("lbl"),
                    b.if_(b.loc("opt"), b.some(b.loc("dv")), b.loc("dv")),
                ])),
            ),
            arm(
                b.err_pat("fe"),
                b.err(b.app2(rt(b, "Schema.PrefixIssues"), b.loc("w"), b.loc("fe"))),
            ),
        ],
    );
    let is_nullish_expr = b.match_(
        b.app(rt(b, "Helper.codec_extra"), b.loc("c")),
        vec![
            arm(
                Pat::Con(ExprBuilder::con("Codec", "$Extra", "$Nullish", 0), vec![]),
                b.bool_(true),
            ),
            arm(Pat::Wild, b.bool_(false)),
        ],
    );
    let dec_default = b.match_(
        b.app(b.loc("th"), b.unit()),
        vec![
            arm(
                b.ok_pat("dv"),
                b.ok(b.tuple(vec![b.loc("lbl"), b.loc("dv")])),
            ),
            arm(
                b.err_pat("fe"),
                b.err(b.app2(rt(b, "Schema.PrefixIssues"), b.loc("w"), b.loc("fe"))),
            ),
        ],
    );
    let dec_missing_fallback = b.match_(
        b.loc("om"),
        vec![
            arm(b.some_pat("th"), dec_default),
            arm(
                Pat::Wild,
                b.err(b.app2(
                    rt(b, "Schema.PrefixIssues"),
                    b.loc("w"),
                    schema_err(b, str_lit(b, "a missing key")),
                )),
            ),
        ],
    );
    let dec_missing = b.block(
        vec![b.let_("is_nullish", is_nullish_expr)],
        b.if_(
            b.bin(BinOp::Or, b.loc("opt"), b.loc("is_nullish")),
            b.ok(b.tuple(vec![b.loc("lbl"), b.none()])),
            dec_missing_fallback,
        ),
    );
    let dec_lookup = b.match_(
        b.app2(rt(b, "Schema.RecordLookup"), b.loc("input"), b.loc("w")),
        vec![
            arm(b.some_pat("fv"), dec_present),
            arm(Pat::Wild, dec_missing),
        ],
    );
    let dec_one = b.match_(
        b.loc("bf"),
        vec![arm(
            pair_pat(b, "lbl", "spec"),
            b.match_(
                b.loc("spec"),
                vec![arm(fld_pat("w", "c", "opt", "om"), dec_lookup)],
            ),
        )],
    );
    let dec_fields = map_result_list(b, "$bd", b.loc("fields"), "bf", "i", dec_one, |_i, e| e);

    let enc_value = |val_name: &str| {
        b.match_(
            b.app(
                b.app(rt(b, "Helper.codec_enc"), b.loc("c")),
                b.loc(val_name),
            ),
            vec![
                arm(
                    b.ok_pat("wv"),
                    b.yield_(b.tuple(vec![b.loc("w"), b.loc("wv")])),
                ),
                arm(
                    b.err_pat("fe"),
                    b.block(
                        vec![b.assign(
                            "eerr",
                            b.some(b.app2(rt(b, "Schema.PrefixIssues"), b.loc("w"), b.loc("fe"))),
                        )],
                        b.break_("$be", b.unit()),
                    ),
                ),
            ],
        )
    };
    let enc_opt_rv = b.match_(
        b.loc("rv"),
        vec![
            arm(b.some_pat("iv"), enc_value("iv")),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let enc_lookup = b.match_(
        b.app2(rt(b, "Schema.RecordLookup"), b.loc("rec"), b.loc("lbl")),
        vec![
            arm(
                b.some_pat("rv"),
                b.if_(b.loc("opt"), enc_opt_rv, enc_value("rv")),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let enc_one = b.match_(
        b.loc("bf"),
        vec![arm(
            pair_pat(b, "lbl", "spec"),
            b.match_(
                b.loc("spec"),
                vec![arm(fld_pat("w", "c", "opt", "_om"), enc_lookup)],
            ),
        )],
    );
    let enc_fields = b.block(
        vec![
            b.var_decl("eerr", b.none()),
            b.let_(
                "epairs",
                b.gen_strict(b.for_ctrl("$be", "bf", b.loc("fields"), enc_one)),
            ),
        ],
        b.match_(
            b.var("eerr"),
            vec![
                arm(b.some_pat("e"), b.err(b.loc("e"))),
                arm(
                    Pat::Wild,
                    b.ok(b.app(rt(b, "Schema.RecordFromEntries"), b.loc("epairs"))),
                ),
            ],
        ),
    );
    let dec_after_unk = b.match_(
        dec_fields,
        vec![
            arm(
                b.ok_pat("pairs"),
                b.ok(b.app(rt(b, "Schema.RecordFromSortedEntries"), b.loc("pairs"))),
            ),
            arm(b.err_pat("e"), b.err(b.loc("e"))),
        ],
    );
    let on_entries = b.match_(
        unk_check,
        vec![
            arm(
                b.some_pat("uk"),
                b.err(b.app2(
                    rt(b, "Schema.PrefixIssues"),
                    b.loc("uk"),
                    schema_err(b, str_lit(b, "an unknown key")),
                )),
            ),
            arm(Pat::Wild, dec_after_unk),
        ],
    );
    let dec_fn = b.lam(
        &["input"],
        b.match_(
            b.app(rt(b, "Schema.RecordEntries"), b.loc("input")),
            vec![
                arm(b.some_pat("entries"), on_entries),
                arm(
                    Pat::Wild,
                    expected_err(b, str_lit(b, "an object"), b.loc("input")),
                ),
            ],
        ),
    );
    let wires_expr = b.gen_strict(b.each(
        "bf",
        b.loc("fields"),
        b.match_(
            b.loc("bf"),
            vec![arm(
                pair_pat(b, "_l", "sp"),
                b.match_(
                    b.loc("sp"),
                    vec![arm(fld_pat("w", "_c", "_o", "_m"), b.yield_(b.loc("w")))],
                ),
            )],
        ),
    ));
    let bmetas_expr = b.gen_strict(b.each(
        "bf",
        b.loc("fields"),
        b.match_(
            b.loc("bf"),
            vec![arm(
                pair_pat(b, "_l", "sp"),
                b.match_(
                    b.loc("sp"),
                    vec![arm(
                        fld_pat("w", "c", "_o", "_m"),
                        b.yield_(b.tuple(vec![
                            b.loc("w"),
                            b.app(rt(b, "Helper.codec_meta"), b.loc("c")),
                        ])),
                    )],
                ),
            )],
        ),
    ));
    let guard_fn = b.lam(
        &["in"],
        b.match_(
            b.app(rt(b, "Schema.RecordEntries"), b.loc("in")),
            vec![
                arm(b.some_pat("_"), b.bool_(true)),
                arm(Pat::Wild, b.bool_(false)),
            ],
        ),
    );
    b.lam(
        &["name", "fields", "ignore_extra"],
        b.block(
            vec![
                Stmt::Expr(dup_check),
                b.let_("wires", wires_expr),
                b.let_("bmetas", bmetas_expr),
            ],
            make_codec(
                b,
                dec_fn,
                b.lam(&["rec"], enc_fields),
                guard_fn,
                meta_con(b, "$Block", vec![b.loc("bmetas"), b.loc("ignore_extra")]),
                extra_con(b, "$Block", vec![b.loc("name"), b.loc("fields")]),
            ),
        ),
    )
}
