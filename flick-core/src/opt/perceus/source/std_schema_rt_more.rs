//! Synthetic `$rt.Helper.*` recursive helpers for JSON Schema meta, blocks, checks, entries, and string trees.

use flick_ir::BinOp;

use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::{
    expected_err, extra_con, make_codec, map_result_list, meta_con, pair_pat, rt, schema_err,
    str_lit,
};
use super::{Expr, Pat, Span, Stmt};

pub(super) fn helper_expr(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$srm");
    match name {
        "resolve_meta" => Some(resolve_meta_helper(&b)),
        "run_checks" => Some(run_checks_helper(&b)),
        "make_block_codec" => Some(super::std_schema_rt_block::make_block_codec_helper(&b)),
        "make_entries_codec" => Some(make_entries_codec_helper(&b)),
        "apply_prop_check" => Some(apply_prop_check_helper(&b)),
        "check_tree" => Some(super::std_schema_rt_tree::check_tree_helper(&b)),
        "decode_tree" => Some(super::std_schema_rt_tree::decode_tree_helper(&b)),
        "encode_tree" => Some(super::std_schema_rt_tree::encode_tree_helper(&b)),
        _ => None,
    }
}

pub(super) fn contains_str(b: &ExprBuilder, loop_id: &str, list: Expr, needle: Expr) -> Expr {
    b.block(
        vec![
            b.var_decl("has", b.bool_(false)),
            Stmt::Expr(b.for_ctrl(
                loop_id,
                "el",
                list,
                b.if_(
                    b.bin(BinOp::Eq, b.loc("el"), needle),
                    b.block(
                        vec![b.assign("has", b.bool_(true))],
                        b.break_(loop_id, b.unit()),
                    ),
                    b.unit(),
                ),
            )),
        ],
        b.var("has"),
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "closed metadata cycle-resolution AST template"
)]
fn resolve_meta_helper(b: &ExprBuilder) -> Expr {
    let mc = |c: &str, n: usize| ExprBuilder::con("Codec", "$Meta", c, n);
    let rec = |e: Expr| b.app2(rt(b, "Helper.resolve_meta"), b.loc("seen"), e);
    let map_items = |xs: Expr| b.gen_strict(b.each("m", xs, b.yield_(rec(b.loc("m")))));
    let map_pairs = |xs: Expr| {
        b.gen_strict(b.each(
            "p",
            xs,
            b.match_(
                b.loc("p"),
                vec![arm(
                    pair_pat(b, "k", "m"),
                    b.yield_(b.tuple(vec![b.loc("k"), rec(b.loc("m"))])),
                )],
            ),
        ))
    };
    b.lam(
        &["seen", "meta"],
        b.match_(
            b.loc("meta"),
            vec![
                arm(
                    Pat::Con(
                        mc("$Lazy", 2),
                        vec![Pat::Bind(b.name("nm")), Pat::Bind(b.name("th"))],
                    ),
                    b.if_(
                        contains_str(b, "$rml", b.loc("seen"), b.loc("nm")),
                        meta_con(b, "$ResolvedLazy", vec![b.loc("nm"), b.none()]),
                        meta_con(
                            b,
                            "$ResolvedLazy",
                            vec![
                                b.loc("nm"),
                                b.some(b.app2(
                                    rt(b, "Helper.resolve_meta"),
                                    b.app2(rt(b, "Helper.list_cons"), b.loc("nm"), b.loc("seen")),
                                    b.app(
                                        rt(b, "Helper.codec_meta"),
                                        b.app(
                                            rt(b, "Helper.schema_codec"),
                                            b.app(b.loc("th"), b.unit()),
                                        ),
                                    ),
                                )),
                            ],
                        ),
                    ),
                ),
                arm(
                    Pat::Con(mc("$Stage", 1), vec![Pat::Bind(b.name("x"))]),
                    meta_con(b, "$Stage", vec![rec(b.loc("x"))]),
                ),
                arm(
                    Pat::Con(mc("$List", 1), vec![Pat::Bind(b.name("x"))]),
                    meta_con(b, "$List", vec![rec(b.loc("x"))]),
                ),
                arm(
                    Pat::Con(mc("$NonEmptyList", 1), vec![Pat::Bind(b.name("x"))]),
                    meta_con(b, "$NonEmptyList", vec![rec(b.loc("x"))]),
                ),
                arm(
                    Pat::Con(mc("$ListEnsure", 1), vec![Pat::Bind(b.name("x"))]),
                    meta_con(b, "$ListEnsure", vec![rec(b.loc("x"))]),
                ),
                arm(
                    Pat::Con(mc("$Tuple", 1), vec![Pat::Bind(b.name("xs"))]),
                    meta_con(b, "$Tuple", vec![map_items(b.loc("xs"))]),
                ),
                arm(
                    Pat::Con(mc("$Option", 1), vec![Pat::Bind(b.name("x"))]),
                    meta_con(b, "$Option", vec![rec(b.loc("x"))]),
                ),
                arm(
                    Pat::Con(mc("$TaggedOption", 1), vec![Pat::Bind(b.name("x"))]),
                    meta_con(b, "$TaggedOption", vec![rec(b.loc("x"))]),
                ),
                arm(
                    Pat::Con(
                        mc("$Result", 2),
                        vec![Pat::Bind(b.name("em")), Pat::Bind(b.name("om"))],
                    ),
                    meta_con(b, "$Result", vec![rec(b.loc("em")), rec(b.loc("om"))]),
                ),
                arm(
                    Pat::Con(mc("$Record", 1), vec![Pat::Bind(b.name("ps"))]),
                    meta_con(b, "$Record", vec![map_pairs(b.loc("ps"))]),
                ),
                arm(
                    Pat::Con(
                        mc("$Block", 2),
                        vec![Pat::Bind(b.name("ps")), Pat::Bind(b.name("ig"))],
                    ),
                    meta_con(b, "$Block", vec![map_pairs(b.loc("ps")), b.loc("ig")]),
                ),
                arm(
                    Pat::Con(
                        mc("$Entries", 2),
                        vec![Pat::Bind(b.name("vm")), Pat::Bind(b.name("cs"))],
                    ),
                    meta_con(b, "$Entries", vec![rec(b.loc("vm")), b.loc("cs")]),
                ),
                arm(
                    Pat::Con(mc("$Adt", 1), vec![Pat::Bind(b.name("cs"))]),
                    meta_con(
                        b,
                        "$Adt",
                        vec![b.gen_strict(b.each(
                            "c",
                            b.loc("cs"),
                            b.match_(
                                b.loc("c"),
                                vec![arm(
                                    pair_pat(b, "sn", "fms"),
                                    b.yield_(b.tuple(vec![b.loc("sn"), map_items(b.loc("fms"))])),
                                )],
                            ),
                        ))],
                    ),
                ),
                arm(
                    Pat::Con(
                        mc("$Refined", 3),
                        vec![
                            Pat::Bind(b.name("n")),
                            Pat::Bind(b.name("k")),
                            Pat::Bind(b.name("bm")),
                        ],
                    ),
                    meta_con(
                        b,
                        "$Refined",
                        vec![b.loc("n"), b.loc("k"), rec(b.loc("bm"))],
                    ),
                ),
                arm(
                    Pat::Con(
                        mc("$Checked", 4),
                        vec![
                            Pat::Bind(b.name("bm")),
                            Pat::Bind(b.name("cs")),
                            Pat::Bind(b.name("t")),
                            Pat::Bind(b.name("d")),
                        ],
                    ),
                    meta_con(
                        b,
                        "$Checked",
                        vec![rec(b.loc("bm")), b.loc("cs"), b.loc("t"), b.loc("d")],
                    ),
                ),
                arm(Pat::Wild, b.loc("meta")),
            ],
        ),
    )
}

fn run_checks_helper(b: &ExprBuilder) -> Expr {
    let atom_pat = Pat::Con(
        ExprBuilder::con("Check", "$Atom", "$Atom", 2),
        vec![Pat::Bind(b.name("rf")), Pat::Wild],
    );
    b.lam(
        &["checks", "val"],
        b.block(
            vec![
                b.var_decl("fail", b.none()),
                Stmt::Expr(b.for_ctrl(
                    "$rchk",
                    "atom",
                    b.loc("checks"),
                    b.match_(
                        b.loc("atom"),
                        vec![arm(
                            atom_pat,
                            b.match_(
                                b.app(b.loc("rf"), b.loc("val")),
                                vec![
                                    arm(
                                        b.some_pat("msg"),
                                        b.block(
                                            vec![b.assign(
                                                "fail",
                                                b.some(schema_err(b, b.loc("msg"))),
                                            )],
                                            b.break_("$rchk", b.unit()),
                                        ),
                                    ),
                                    arm(Pat::Wild, b.unit()),
                                ],
                            ),
                        )],
                    ),
                )),
            ],
            b.match_(
                b.var("fail"),
                vec![
                    arm(b.some_pat("e"), b.err(b.loc("e"))),
                    arm(Pat::Wild, b.ok(b.loc("val"))),
                ],
            ),
        ),
    )
}

#[expect(clippy::too_many_lines, reason = "closed entries codec AST template")]
fn make_entries_codec_helper(b: &ExprBuilder) -> Expr {
    let atom_meta = b.gen_strict(b.each(
        "a",
        b.loc("chks"),
        b.match_(
            b.loc("a"),
            vec![arm(
                Pat::Con(
                    ExprBuilder::con("Check", "$Atom", "$Atom", 2),
                    vec![Pat::Wild, Pat::Bind(b.name("m"))],
                ),
                b.yield_(b.loc("m")),
            )],
        ),
    ));
    b.lam(
        &["val_c", "chks"],
        b.block(
            vec![
                b.let_("vd", b.app(rt(b, "Helper.codec_dec"), b.loc("val_c"))),
                b.let_("ve", b.app(rt(b, "Helper.codec_enc"), b.loc("val_c"))),
            ],
            make_codec(
                b,
                b.lam(
                    &["input"],
                    b.match_(
                        b.app(rt(b, "Schema.RecordEntries"), b.loc("input")),
                        vec![
                            arm(
                                b.some_pat("ents"),
                                b.match_(
                                    map_result_list(
                                        b,
                                        "$ed",
                                        b.loc("ents"),
                                        "ent",
                                        "i",
                                        b.match_(
                                            b.loc("ent"),
                                            vec![arm(
                                                pair_pat(b, "k", "v"),
                                                b.match_(
                                                    b.app(b.loc("vd"), b.loc("v")),
                                                    vec![
                                                        arm(
                                                            b.ok_pat("dv"),
                                                            b.ok(b.tuple(vec![
                                                                b.loc("k"),
                                                                b.loc("dv"),
                                                            ])),
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
                                            )],
                                        ),
                                        |_idx, err| err,
                                    ),
                                    vec![
                                        arm(b.err_pat("e"), b.err(b.loc("e"))),
                                        arm(
                                            b.ok_pat("pairs"),
                                            b.match_(
                                                b.app2(
                                                    rt(b, "Helper.run_checks"),
                                                    b.loc("chks"),
                                                    b.loc("input"),
                                                ),
                                                vec![
                                                    arm(b.ok_pat("_"), b.ok(b.loc("pairs"))),
                                                    arm(b.err_pat("e"), b.err(b.loc("e"))),
                                                ],
                                            ),
                                        ),
                                    ],
                                ),
                            ),
                            arm(
                                Pat::Wild,
                                expected_err(b, str_lit(b, "an object"), b.loc("input")),
                            ),
                        ],
                    ),
                ),
                b.lam(
                    &["pairs"],
                    b.match_(
                        b.app2(
                            rt(b, "Helper.run_checks"),
                            b.loc("chks"),
                            b.app(rt(b, "Schema.RecordFromEntries"), b.loc("pairs")),
                        ),
                        vec![
                            arm(b.err_pat("e"), b.err(b.loc("e"))),
                            arm(
                                b.ok_pat("_"),
                                b.match_(
                                    map_result_list(
                                        b,
                                        "$ee",
                                        b.loc("pairs"),
                                        "ent",
                                        "i",
                                        b.match_(
                                            b.loc("ent"),
                                            vec![arm(
                                                pair_pat(b, "k", "v"),
                                                b.match_(
                                                    b.app(b.loc("ve"), b.loc("v")),
                                                    vec![
                                                        arm(
                                                            b.ok_pat("wv"),
                                                            b.ok(b.tuple(vec![
                                                                b.loc("k"),
                                                                b.loc("wv"),
                                                            ])),
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
                                            )],
                                        ),
                                        |_idx, err| err,
                                    ),
                                    vec![
                                        arm(
                                            b.ok_pat("out"),
                                            b.ok(b.app(
                                                rt(b, "Schema.RecordFromEntries"),
                                                b.loc("out"),
                                            )),
                                        ),
                                        arm(b.err_pat("e"), b.err(b.loc("e"))),
                                    ],
                                ),
                            ),
                        ],
                    ),
                ),
                b.lam(
                    &["in"],
                    b.match_(
                        b.app(rt(b, "Schema.RecordEntries"), b.loc("in")),
                        vec![
                            arm(b.some_pat("_"), b.bool_(true)),
                            arm(Pat::Wild, b.bool_(false)),
                        ],
                    ),
                ),
                meta_con(
                    b,
                    "$Entries",
                    vec![b.app(rt(b, "Helper.codec_meta"), b.loc("val_c")), atom_meta],
                ),
                extra_con(b, "$Entries", vec![b.loc("val_c"), b.loc("chks")]),
            ),
        ),
    )
}

fn apply_prop_check_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["atom", "codec"],
        b.match_(
            b.app(rt(b, "Helper.codec_extra"), b.loc("codec")),
            vec![
                arm(
                    Pat::Con(ExprBuilder::con("Codec", "$Extra", "$Entries", 2), vec![Pat::Bind(b.name("vc")), Pat::Bind(b.name("chks"))]),
                    b.app2(
                        rt(b, "Helper.make_entries_codec"),
                        b.loc("vc"),
                        b.app2(b.global("List.Core", "append"), b.loc("chks"), b.list(vec![b.loc("atom")])),
                    ),
                ),
                arm(Pat::Wild, b.app(rt(b, "Schema.Defect"), str_lit(b, "TypeError: a property check applies to a codec made by `Codec.entries`"))),
            ],
        ),
    )
}
