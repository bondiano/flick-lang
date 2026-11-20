//! Synthetic `$rt.Helper.*` string-tree codec helpers (`Codec.string_tree`).

use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::{cat, expected_err, pair_pat, rt, schema_err, str_lit};
use super::std_schema_rt_more::contains_str;
use super::{Expr, Pat};

pub(super) fn check_tree_helper(b: &ExprBuilder) -> Expr {
    let kc = |c: &str, n: usize| ExprBuilder::con("Schema", "$Kind", c, n);
    b.lam(
        &["seen", "sch"],
        b.match_(
            b.app(rt(b, "Helper.schema_kind"), b.loc("sch")),
            vec![
                arm(Pat::Con(kc("$Number", 0), vec![]), b.unit()),
                arm(Pat::Con(kc("$Int", 0), vec![]), b.unit()),
                arm(Pat::Con(kc("$String", 0), vec![]), b.unit()),
                arm(Pat::Con(kc("$Bool", 0), vec![]), b.unit()),
                arm(Pat::Con(kc("$BigInt", 0), vec![]), b.unit()),
                arm(
                    Pat::Con(kc("$List", 1), vec![Pat::Bind(b.name("it"))]),
                    b.app2(rt(b, "Helper.check_tree"), b.loc("seen"), b.loc("it")),
                ),
                arm(
                    Pat::Con(
                        kc("$Refined", 3),
                        vec![Pat::Wild, Pat::Bind(b.name("bs")), Pat::Wild],
                    ),
                    b.app2(rt(b, "Helper.check_tree"), b.loc("seen"), b.loc("bs")),
                ),
                arm(
                    Pat::Con(kc("$Tuple", 1), vec![Pat::Bind(b.name("its"))]),
                    b.each(
                        "it",
                        b.loc("its"),
                        b.app2(rt(b, "Helper.check_tree"), b.loc("seen"), b.loc("it")),
                    ),
                ),
                arm(
                    Pat::Con(kc("$Record", 1), vec![Pat::Bind(b.name("flds"))]),
                    b.each(
                        "f",
                        b.loc("flds"),
                        b.match_(
                            b.loc("f"),
                            vec![arm(
                                pair_pat(b, "_k", "fs"),
                                b.app2(rt(b, "Helper.check_tree"), b.loc("seen"), b.loc("fs")),
                            )],
                        ),
                    ),
                ),
                arm(
                    Pat::Con(
                        kc("$Lazy", 2),
                        vec![Pat::Bind(b.name("nm")), Pat::Bind(b.name("th"))],
                    ),
                    b.if_(
                        contains_str(b, "$ct", b.loc("seen"), b.loc("nm")),
                        b.unit(),
                        b.app2(
                            rt(b, "Helper.check_tree"),
                            b.app2(rt(b, "Helper.list_cons"), b.loc("nm"), b.loc("seen")),
                            b.app(b.loc("th"), b.unit()),
                        ),
                    ),
                ),
                arm(
                    Pat::Wild,
                    b.app(
                        rt(b, "Schema.Defect"),
                        str_lit(b, "Codec.string_tree: this schema has no string-tree form"),
                    ),
                ),
            ],
        ),
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "closed Schema tree decoder AST template"
)]
pub(super) fn decode_tree_helper(b: &ExprBuilder) -> Expr {
    let kc = |c: &str, n: usize| ExprBuilder::con("Schema", "$Kind", c, n);
    let leaf = |k: &str| b.app2(rt(b, "Schema.DecodeLeaf"), str_lit(b, k), b.loc("input"));
    b.lam(
        &["sch", "input"],
        b.match_(
            b.app(rt(b, "Helper.schema_kind"), b.loc("sch")),
            vec![
                arm(Pat::Con(kc("$Number", 0), vec![]), leaf("tree_number")),
                arm(Pat::Con(kc("$Int", 0), vec![]), leaf("tree_int")),
                arm(Pat::Con(kc("$String", 0), vec![]), leaf("string")),
                arm(Pat::Con(kc("$Bool", 0), vec![]), leaf("tree_bool")),
                arm(Pat::Con(kc("$BigInt", 0), vec![]), leaf("tree_bigint")),
                arm(
                    Pat::Con(kc("$Lazy", 2), vec![Pat::Wild, Pat::Bind(b.name("th"))]),
                    b.app2(
                        rt(b, "Helper.decode_tree"),
                        b.app(b.loc("th"), b.unit()),
                        b.loc("input"),
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
                    b.match_(
                        b.app2(rt(b, "Helper.decode_tree"), b.loc("bs"), b.loc("input")),
                        vec![
                            arm(
                                b.ok_pat("v"),
                                b.if_(
                                    b.app(b.loc("chk"), b.loc("v")),
                                    b.ok(b.loc("v")),
                                    b.err(schema_err(
                                        b,
                                        cat(b, str_lit(b, "not a valid "), b.loc("nm")),
                                    )),
                                ),
                            ),
                            arm(b.err_pat("e"), b.err(b.loc("e"))),
                        ],
                    ),
                ),
                arm(
                    Pat::Con(kc("$List", 1), vec![Pat::Bind(b.name("it"))]),
                    b.app2(
                        rt(b, "Helper.validate_list"),
                        b.app(rt(b, "Helper.decode_tree"), b.loc("it")),
                        b.loc("input"),
                    ),
                ),
                arm(
                    Pat::Con(kc("$Tuple", 1), vec![Pat::Bind(b.name("its"))]),
                    b.block(
                        vec![b.let_(
                            "decs",
                            b.gen_strict(b.each(
                                "it",
                                b.loc("its"),
                                b.yield_(b.app(rt(b, "Helper.decode_tree"), b.loc("it"))),
                            )),
                        )],
                        b.app2(rt(b, "Helper.decode_tuple"), b.loc("decs"), b.loc("input")),
                    ),
                ),
                arm(
                    Pat::Con(kc("$Record", 1), vec![Pat::Bind(b.name("flds"))]),
                    b.match_(
                        b.app(rt(b, "Schema.RecordEntries"), b.loc("input")),
                        vec![
                            arm(
                                b.some_pat("ents"),
                                b.block(
                                    vec![b.let_(
                                        "decs",
                                        b.gen_strict(b.each(
                                            "f",
                                            b.loc("flds"),
                                            b.match_(
                                                b.loc("f"),
                                                vec![arm(
                                                    pair_pat(b, "k", "fs"),
                                                    b.yield_(b.tuple(vec![
                                                        b.loc("k"),
                                                        b.app(
                                                            rt(b, "Helper.decode_tree"),
                                                            b.loc("fs"),
                                                        ),
                                                    ])),
                                                )],
                                            ),
                                        )),
                                    )],
                                    b.match_(
                                        b.app3(
                                            rt(b, "Helper.decode_object"),
                                            b.loc("decs"),
                                            b.loc("ents"),
                                            b.loc("input"),
                                        ),
                                        vec![
                                            arm(
                                                b.ok_pat("pairs"),
                                                b.ok(b.app(
                                                    rt(b, "Schema.RecordFromSortedEntries"),
                                                    b.loc("pairs"),
                                                )),
                                            ),
                                            arm(b.err_pat("e"), b.err(b.loc("e"))),
                                        ],
                                    ),
                                ),
                            ),
                            arm(
                                Pat::Wild,
                                expected_err(b, str_lit(b, "a record"), b.loc("input")),
                            ),
                        ],
                    ),
                ),
                arm(Pat::Wild, b.ok(b.loc("input"))),
            ],
        ),
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "closed Schema tree encoder AST template"
)]
pub(super) fn encode_tree_helper(b: &ExprBuilder) -> Expr {
    let kc = |c: &str, n: usize| ExprBuilder::con("Schema", "$Kind", c, n);
    b.lam(
        &["sch", "val"],
        b.match_(
            b.app(rt(b, "Helper.schema_kind"), b.loc("sch")),
            vec![
                arm(
                    Pat::Con(kc("$Number", 0), vec![]),
                    b.app2(
                        rt(b, "Schema.EncodeLeaf"),
                        str_lit(b, "tree_number"),
                        b.loc("val"),
                    ),
                ),
                arm(
                    Pat::Con(kc("$Int", 0), vec![]),
                    b.app2(
                        rt(b, "Schema.EncodeLeaf"),
                        str_lit(b, "tree_int"),
                        b.loc("val"),
                    ),
                ),
                arm(
                    Pat::Con(kc("$Bool", 0), vec![]),
                    b.app2(
                        rt(b, "Schema.EncodeLeaf"),
                        str_lit(b, "tree_bool"),
                        b.loc("val"),
                    ),
                ),
                arm(
                    Pat::Con(kc("$BigInt", 0), vec![]),
                    b.app2(
                        rt(b, "Schema.EncodeLeaf"),
                        str_lit(b, "tree_bigint"),
                        b.loc("val"),
                    ),
                ),
                arm(
                    Pat::Con(kc("$Lazy", 2), vec![Pat::Wild, Pat::Bind(b.name("th"))]),
                    b.app2(
                        rt(b, "Helper.encode_tree"),
                        b.app(b.loc("th"), b.unit()),
                        b.loc("val"),
                    ),
                ),
                arm(
                    Pat::Con(
                        kc("$Refined", 3),
                        vec![Pat::Wild, Pat::Bind(b.name("bs")), Pat::Wild],
                    ),
                    b.app2(rt(b, "Helper.encode_tree"), b.loc("bs"), b.loc("val")),
                ),
                arm(
                    Pat::Con(kc("$List", 1), vec![Pat::Bind(b.name("it"))]),
                    b.app2(
                        rt(b, "Helper.encode_list"),
                        b.app(rt(b, "Helper.encode_tree"), b.loc("it")),
                        b.loc("val"),
                    ),
                ),
                arm(
                    Pat::Con(kc("$Tuple", 1), vec![Pat::Bind(b.name("its"))]),
                    b.block(
                        vec![b.let_(
                            "encs",
                            b.gen_strict(b.each(
                                "it",
                                b.loc("its"),
                                b.yield_(b.app(rt(b, "Helper.encode_tree"), b.loc("it"))),
                            )),
                        )],
                        b.app2(rt(b, "Helper.encode_tuple"), b.loc("encs"), b.loc("val")),
                    ),
                ),
                arm(
                    Pat::Con(kc("$Record", 1), vec![Pat::Bind(b.name("flds"))]),
                    b.block(
                        vec![b.let_(
                            "encs",
                            b.gen_strict(b.each(
                                "f",
                                b.loc("flds"),
                                b.match_(
                                    b.loc("f"),
                                    vec![arm(
                                        pair_pat(b, "k", "fs"),
                                        b.yield_(b.tuple(vec![
                                            b.loc("k"),
                                            b.app(rt(b, "Helper.encode_tree"), b.loc("fs")),
                                        ])),
                                    )],
                                ),
                            )),
                        )],
                        b.app2(rt(b, "Helper.encode_record"), b.loc("encs"), b.loc("val")),
                    ),
                ),
                arm(Pat::Wild, b.ok(b.loc("val"))),
            ],
        ),
    )
}
