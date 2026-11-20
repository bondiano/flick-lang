//! Audited `Codec` wire combinators (`tagged_option`, `non_empty_list`, `list_ensure`, `json_string`).

use flick_ir::BinOp;

use super::expr_builder::{ExprBuilder, arm};
use super::std_codec::REFUSE_FALLIBLE;
pub(super) use super::std_codec_meta::{
    codec_annotate, codec_annotations, codec_catch, codec_check_effect, codec_flip,
};
use super::std_schema_rt::{
    expected_err, extra_con, make_codec, meta_con, pair_pat, rt, schema_err, str_lit,
};
use super::{Expr, Pat};

#[expect(
    clippy::too_many_lines,
    reason = "closed tagged-option codec AST template"
)]
pub(super) fn codec_tagged_option(b: &ExprBuilder) -> Expr {
    let ok_id = b.lam(&["x"], b.ok(b.loc("x")));
    let bad_tag = b.err(b.app2(
        rt(b, "Schema.PrefixIssues"),
        str_lit(b, "_tag"),
        schema_err(b, str_lit(b, "expected one of None, Some")),
    ));
    let dec_none = b.match_(
        b.app3(
            rt(b, "Helper.decode_object"),
            b.list(vec![b.tuple(vec![str_lit(b, "_tag"), ok_id.clone()])]),
            b.loc("ents"),
            b.loc("in"),
        ),
        vec![
            arm(b.ok_pat("_"), b.ok(b.none())),
            arm(b.err_pat("e"), b.err(b.loc("e"))),
        ],
    );
    let on_some_kvs = b.match_(
        b.app2(rt(b, "Helper.list_at"), b.int(1), b.loc("kvs")),
        vec![arm(pair_pat(b, "_k", "v"), b.ok(b.some(b.loc("v"))))],
    );
    let dec_some = b.match_(
        b.app3(
            rt(b, "Helper.decode_object"),
            b.list(vec![
                b.tuple(vec![str_lit(b, "_tag"), ok_id]),
                b.tuple(vec![str_lit(b, "value"), b.loc("id")]),
            ]),
            b.loc("ents"),
            b.loc("in"),
        ),
        vec![
            arm(b.ok_pat("kvs"), on_some_kvs),
            arm(b.err_pat("e"), b.err(b.loc("e"))),
        ],
    );
    let on_tv = b.if_(
        b.app2(rt(b, "Schema.SameValue"), b.loc("tv"), str_lit(b, "None")),
        dec_none,
        b.if_(
            b.app2(rt(b, "Schema.SameValue"), b.loc("tv"), str_lit(b, "Some")),
            dec_some,
            bad_tag.clone(),
        ),
    );
    let on_ents = b.match_(
        b.app2(
            rt(b, "Schema.RecordLookup"),
            b.loc("in"),
            str_lit(b, "_tag"),
        ),
        vec![arm(b.some_pat("tv"), on_tv), arm(Pat::Wild, bad_tag)],
    );
    let dec_fn = b.lam(
        &["in"],
        b.match_(
            b.app(rt(b, "Schema.RecordEntries"), b.loc("in")),
            vec![
                arm(b.some_pat("ents"), on_ents),
                arm(
                    Pat::Wild,
                    expected_err(
                        b,
                        str_lit(b, "an object with `_tag` None or Some"),
                        b.loc("in"),
                    ),
                ),
            ],
        ),
    );
    let enc_some = b.match_(
        b.app(b.loc("ie"), b.loc("x")),
        vec![
            arm(
                b.ok_pat("w"),
                b.ok(b.app(
                    rt(b, "Schema.RecordFromEntries"),
                    b.list(vec![
                        b.tuple(vec![str_lit(b, "_tag"), str_lit(b, "Some")]),
                        b.tuple(vec![str_lit(b, "value"), b.loc("w")]),
                    ]),
                )),
            ),
            arm(
                b.err_pat("e"),
                b.err(b.app2(
                    rt(b, "Schema.PrefixIssues"),
                    str_lit(b, "value"),
                    b.loc("e"),
                )),
            ),
        ],
    );
    let enc_none = b.ok(b.app(
        rt(b, "Schema.RecordFromEntries"),
        b.list(vec![b.tuple(vec![str_lit(b, "_tag"), str_lit(b, "None")])]),
    ));
    let enc_fn = b.lam(
        &["v"],
        b.match_(
            b.loc("v"),
            vec![arm(b.some_pat("x"), enc_some), arm(Pat::Wild, enc_none)],
        ),
    );
    let acc_fn = b.lam(
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
        &["item"],
        b.block(
            vec![
                b.let_("id", b.app(rt(b, "Helper.codec_dec"), b.loc("item"))),
                b.let_("ie", b.app(rt(b, "Helper.codec_enc"), b.loc("item"))),
            ],
            make_codec(
                b,
                dec_fn,
                enc_fn,
                acc_fn,
                meta_con(
                    b,
                    "$TaggedOption",
                    vec![b.app(rt(b, "Helper.codec_meta"), b.loc("item"))],
                ),
                extra_con(b, "$None", vec![]),
            ),
        ),
    )
}

pub(super) fn codec_non_empty_list(b: &ExprBuilder) -> Expr {
    let on_xs = b.if_(
        b.bin(
            BinOp::Eq,
            b.app(b.global("List.Core", "length"), b.loc("xs")),
            b.int(0),
        ),
        b.err(b.app2(
            rt(b, "Schema.PrefixIssues"),
            b.int(0),
            schema_err(b, str_lit(b, "a missing key")),
        )),
        b.app(b.app(rt(b, "Helper.codec_dec"), b.loc("lc")), b.loc("in")),
    );
    let dec_fn = b.lam(
        &["in"],
        b.match_(
            b.app(rt(b, "Schema.ListFromWire"), b.loc("in")),
            vec![
                arm(b.some_pat("xs"), on_xs),
                arm(
                    Pat::Wild,
                    expected_err(b, str_lit(b, "an array"), b.loc("in")),
                ),
            ],
        ),
    );
    b.lam(
        &["item"],
        b.block(
            vec![b.let_("lc", b.app(b.global("Codec", "list"), b.loc("item")))],
            make_codec(
                b,
                dec_fn,
                b.app(rt(b, "Helper.codec_enc"), b.loc("lc")),
                b.app(rt(b, "Helper.codec_acc"), b.loc("lc")),
                meta_con(
                    b,
                    "$NonEmptyList",
                    vec![b.app(rt(b, "Helper.codec_meta"), b.loc("item"))],
                ),
                extra_con(b, "$None", vec![]),
            ),
        ),
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "closed list-ensure codec AST template"
)]
pub(super) fn codec_list_ensure(b: &ExprBuilder) -> Expr {
    let try_first = b.if_(
        b.app(b.loc("ia"), b.loc("in")),
        b.match_(
            b.app(b.loc("id"), b.loc("in")),
            vec![
                arm(b.ok_pat("v"), b.some(b.ok(b.list(vec![b.loc("v")])))),
                arm(b.err_pat("e"), b.some(b.err(b.loc("e")))),
            ],
        ),
        b.none(),
    );
    let dec_lc = b.app(b.app(rt(b, "Helper.codec_dec"), b.loc("lc")), b.loc("in"));
    let on_err1 = b.match_(
        b.app(rt(b, "Schema.ListFromWire"), b.loc("in")),
        vec![
            arm(
                b.some_pat("_"),
                b.match_(
                    dec_lc.clone(),
                    vec![
                        arm(b.ok_pat("xs"), b.ok(b.loc("xs"))),
                        arm(
                            b.err_pat("e2"),
                            b.err(b.app2(rt(b, "Schema.CombineErrors"), b.loc("e1"), b.loc("e2"))),
                        ),
                    ],
                ),
            ),
            arm(Pat::Wild, b.err(b.loc("e1"))),
        ],
    );
    let on_no_first = b.match_(
        b.app(rt(b, "Schema.ListFromWire"), b.loc("in")),
        vec![
            arm(b.some_pat("_"), dec_lc),
            arm(Pat::Wild, expected_err(b, str_lit(b, "Union"), b.loc("in"))),
        ],
    );
    let dec_fn = b.lam(
        &["in"],
        b.block(
            vec![b.let_("first", try_first)],
            b.match_(
                b.loc("first"),
                vec![
                    arm(
                        b.some_pat("r"),
                        b.match_(
                            b.loc("r"),
                            vec![
                                arm(b.ok_pat("xs"), b.ok(b.loc("xs"))),
                                arm(b.err_pat("e1"), on_err1),
                            ],
                        ),
                    ),
                    arm(Pat::Wild, on_no_first),
                ],
            ),
        ),
    );
    let enc_fn = b.lam(
        &["xs"],
        b.if_(
            b.bin(
                BinOp::Eq,
                b.app(b.global("List.Core", "length"), b.loc("xs")),
                b.int(1),
            ),
            b.app(
                b.loc("ie"),
                b.app2(rt(b, "Helper.list_at"), b.int(0), b.loc("xs")),
            ),
            b.app(b.app(rt(b, "Helper.codec_enc"), b.loc("lc")), b.loc("xs")),
        ),
    );
    let is_list = b.match_(
        b.app(rt(b, "Schema.ListFromWire"), b.loc("in")),
        vec![
            arm(b.some_pat("_"), b.bool_(true)),
            arm(Pat::Wild, b.bool_(false)),
        ],
    );
    let acc_fn = b.lam(
        &["in"],
        b.bin(BinOp::Or, is_list, b.app(b.loc("ia"), b.loc("in"))),
    );
    b.lam(
        &["item"],
        b.block(
            vec![
                b.let_("lc", b.app(b.global("Codec", "list"), b.loc("item"))),
                b.let_("id", b.app(rt(b, "Helper.codec_dec"), b.loc("item"))),
                b.let_("ie", b.app(rt(b, "Helper.codec_enc"), b.loc("item"))),
                b.let_("ia", b.app(rt(b, "Helper.codec_acc"), b.loc("item"))),
            ],
            make_codec(
                b,
                dec_fn,
                enc_fn,
                acc_fn,
                meta_con(
                    b,
                    "$ListEnsure",
                    vec![b.app(rt(b, "Helper.codec_meta"), b.loc("item"))],
                ),
                extra_con(b, "$None", vec![]),
            ),
        ),
    )
}

pub(super) fn codec_json_string(b: &ExprBuilder) -> Expr {
    let parse_str = b.match_(
        b.app(rt(b, "Json.Parse"), b.loc("s")),
        vec![
            arm(
                b.ok_pat("parsed"),
                b.app(
                    b.app(rt(b, "Helper.codec_dec"), b.loc("inner")),
                    b.loc("parsed"),
                ),
            ),
            arm(
                Pat::Wild,
                b.err(schema_err(b, str_lit(b, "an invalid value"))),
            ),
        ],
    );
    let dec_fn = b.lam(
        &["in"],
        b.match_(
            b.app2(
                rt(b, "Schema.DecodeLeaf"),
                str_lit(b, "string"),
                b.loc("in"),
            ),
            vec![
                arm(b.ok_pat("s"), parse_str),
                arm(
                    Pat::Wild,
                    expected_err(
                        b,
                        str_lit(b, "a string that will be decoded as JSON"),
                        b.loc("in"),
                    ),
                ),
            ],
        ),
    );
    let enc_fn = b.lam(
        &["v"],
        b.match_(
            b.app(b.app(rt(b, "Helper.codec_enc"), b.loc("inner")), b.loc("v")),
            vec![
                arm(
                    b.ok_pat("w"),
                    b.ok(b.app(rt(b, "Json.Stringify"), b.loc("w"))),
                ),
                arm(b.err_pat("e"), b.err(b.loc("e"))),
            ],
        ),
    );
    b.lam(
        &["inner"],
        make_codec(
            b,
            dec_fn,
            enc_fn,
            b.app(rt(b, "Schema.AcceptsLeaf"), str_lit(b, "string")),
            meta_con(b, "$Refused", vec![str_lit(b, REFUSE_FALLIBLE)]),
            extra_con(b, "$None", vec![]),
        ),
    )
}
