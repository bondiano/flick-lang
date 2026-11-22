//! Audited compound `Schema` pure IR adapters (`result`, `record`, `tuple`, `adt`).

use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::{
    expected_err, extra_con, kind_con, make_codec, make_schema, meta_con, pair_pat, rt, str_lit,
    triple_pat,
};
use super::{Expr, Pat};

fn is_record_wire(b: &ExprBuilder) -> Expr {
    b.lam(
        &["in"],
        b.match_(
            b.app(rt(b, "Schema.RecordEntries"), b.loc("in")),
            vec![
                arm(b.some_pat("_"), b.bool_(true)),
                arm(Pat::Wild, b.bool_(false)),
            ],
        ),
    )
}

fn codec_part(b: &ExprBuilder, part: &str, schema_var: &str) -> Expr {
    b.app(
        rt(b, part),
        b.app(rt(b, "Helper.schema_codec"), b.loc(schema_var)),
    )
}

fn decode_single_key(b: &ExprBuilder, key: &str, dec_var: &str) -> Expr {
    b.app3(
        rt(b, "Helper.decode_object"),
        b.list(vec![b.tuple(vec![str_lit(b, key), b.loc(dec_var)])]),
        b.loc("ents"),
        b.loc("input"),
    )
}

fn encode_single_key(b: &ExprBuilder, key: &str, enc_var: &str) -> Expr {
    b.match_(
        b.app(b.loc(enc_var), b.loc("x")),
        vec![
            arm(
                b.ok_pat("w"),
                b.ok(b.app(
                    rt(b, "Schema.RecordFromEntries"),
                    b.list(vec![b.tuple(vec![str_lit(b, key), b.loc("w")])]),
                )),
            ),
            arm(
                b.err_pat("e"),
                b.err(b.app2(rt(b, "Schema.PrefixIssues"), str_lit(b, key), b.loc("e"))),
            ),
        ],
    )
}

fn unwrap_first_val(b: &ExprBuilder, wrap: impl FnOnce(Expr) -> Expr) -> Expr {
    b.match_(
        b.app2(rt(b, "Helper.list_at"), b.int(0), b.loc("kvs")),
        vec![arm(pair_pat(b, "_k", "v"), b.ok(wrap(b.loc("v"))))],
    )
}

fn schema_result_validate(b: &ExprBuilder) -> Expr {
    b.lam(
        &["input"],
        b.match_(
            b.loc("input"),
            vec![
                arm(
                    b.ok_pat("x"),
                    b.match_(
                        b.app(b.app(rt(b, "Helper.schema_val"), b.loc("os")), b.loc("x")),
                        vec![
                            arm(b.ok_pat("v"), b.ok(b.ok(b.loc("v")))),
                            arm(b.err_pat("e"), b.err(b.loc("e"))),
                        ],
                    ),
                ),
                arm(
                    b.err_pat("x"),
                    b.match_(
                        b.app(b.app(rt(b, "Helper.schema_val"), b.loc("es")), b.loc("x")),
                        vec![
                            arm(b.ok_pat("v"), b.ok(b.err(b.loc("v")))),
                            arm(b.err_pat("e"), b.err(b.loc("e"))),
                        ],
                    ),
                ),
                arm(
                    Pat::Wild,
                    expected_err(b, str_lit(b, "Result"), b.loc("input")),
                ),
            ],
        ),
    )
}

pub(super) fn schema_result(b: &ExprBuilder) -> Expr {
    b.lam(
        &["es", "os"],
        b.block(
            vec![
                b.let_("ec", b.app(rt(b, "Helper.schema_codec"), b.loc("es"))),
                b.let_("oc", b.app(rt(b, "Helper.schema_codec"), b.loc("os"))),
                b.let_("ed", b.app(rt(b, "Helper.codec_dec"), b.loc("ec"))),
                b.let_("od", b.app(rt(b, "Helper.codec_dec"), b.loc("oc"))),
                b.let_("ee", b.app(rt(b, "Helper.codec_enc"), b.loc("ec"))),
                b.let_("oe", b.app(rt(b, "Helper.codec_enc"), b.loc("oc"))),
            ],
            make_schema(
                b,
                schema_result_validate(b),
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
                                        decode_single_key(b, "ok", "od"),
                                        vec![
                                            arm(b.ok_pat("kvs"), unwrap_first_val(b, |v| b.ok(v))),
                                            arm(
                                                b.err_pat("oke"),
                                                b.match_(
                                                    decode_single_key(b, "err", "ed"),
                                                    vec![
                                                        arm(
                                                            b.ok_pat("kvs"),
                                                            unwrap_first_val(b, |v| b.err(v)),
                                                        ),
                                                        arm(
                                                            b.err_pat("ere"),
                                                            b.err(b.app2(
                                                                rt(b, "Schema.CombineErrors"),
                                                                b.loc("oke"),
                                                                b.loc("ere"),
                                                            )),
                                                        ),
                                                    ],
                                                ),
                                            ),
                                        ],
                                    ),
                                ),
                                arm(
                                    Pat::Wild,
                                    expected_err(
                                        b,
                                        str_lit(b, "an object with `ok` or `err`"),
                                        b.loc("input"),
                                    ),
                                ),
                            ],
                        ),
                    ),
                    b.lam(
                        &["v"],
                        b.match_(
                            b.loc("v"),
                            vec![
                                arm(b.ok_pat("x"), encode_single_key(b, "ok", "oe")),
                                arm(b.err_pat("x"), encode_single_key(b, "err", "ee")),
                            ],
                        ),
                    ),
                    is_record_wire(b),
                    meta_con(
                        b,
                        "$Result",
                        vec![
                            b.app(rt(b, "Helper.codec_meta"), b.loc("ec")),
                            b.app(rt(b, "Helper.codec_meta"), b.loc("oc")),
                        ],
                    ),
                    extra_con(b, "$None", vec![]),
                ),
                kind_con(b, "$Result", vec![b.loc("es"), b.loc("os")]),
                b.none(),
            ),
        ),
    )
}

pub(super) fn schema_record(b: &ExprBuilder) -> Expr {
    let map_f = |f_body: Expr| {
        b.gen_strict(b.each(
            "fld",
            b.loc("fields"),
            b.match_(
                b.loc("fld"),
                vec![arm(
                    pair_pat(b, "k", "s"),
                    b.yield_(b.tuple(vec![b.loc("k"), f_body])),
                )],
            ),
        ))
    };
    b.lam(
        &["fields"],
        b.block(
            vec![
                b.let_("decs", map_f(codec_part(b, "Helper.codec_dec", "s"))),
                b.let_("encs", map_f(codec_part(b, "Helper.codec_enc", "s"))),
                b.let_("metas", map_f(codec_part(b, "Helper.codec_meta", "s"))),
            ],
            make_schema(
                b,
                b.app(rt(b, "Helper.validate_record"), b.loc("fields")),
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
                                arm(
                                    Pat::Wild,
                                    expected_err(b, str_lit(b, "an object"), b.loc("input")),
                                ),
                            ],
                        ),
                    ),
                    b.app(rt(b, "Helper.encode_record"), b.loc("encs")),
                    is_record_wire(b),
                    meta_con(b, "$Record", vec![b.loc("metas")]),
                    extra_con(b, "$None", vec![]),
                ),
                kind_con(b, "$Record", vec![b.loc("fields")]),
                b.none(),
            ),
        ),
    )
}

pub(super) fn schema_tuple(b: &ExprBuilder) -> Expr {
    let map_i = |f_body: Expr| b.gen_strict(b.each("s", b.loc("items"), b.yield_(f_body)));
    b.lam(
        &["items"],
        b.block(
            vec![
                b.let_("vals", map_i(b.app(rt(b, "Helper.schema_val"), b.loc("s")))),
                b.let_("decs", map_i(codec_part(b, "Helper.codec_dec", "s"))),
                b.let_("encs", map_i(codec_part(b, "Helper.codec_enc", "s"))),
                b.let_("metas", map_i(codec_part(b, "Helper.codec_meta", "s"))),
            ],
            make_schema(
                b,
                b.app(rt(b, "Helper.validate_tuple"), b.loc("vals")),
                make_codec(
                    b,
                    b.app(rt(b, "Helper.decode_tuple"), b.loc("decs")),
                    b.app(rt(b, "Helper.encode_tuple"), b.loc("encs")),
                    b.lam(
                        &["in"],
                        b.match_(
                            b.app(rt(b, "Schema.ListFromWire"), b.loc("in")),
                            vec![
                                arm(b.some_pat("_"), b.bool_(true)),
                                arm(Pat::Wild, b.bool_(false)),
                            ],
                        ),
                    ),
                    meta_con(b, "$Tuple", vec![b.loc("metas")]),
                    extra_con(b, "$None", vec![]),
                ),
                kind_con(b, "$Tuple", vec![b.loc("items")]),
                b.none(),
            ),
        ),
    )
}

pub(super) fn schema_adt(b: &ExprBuilder) -> Expr {
    b.lam(
        &["name", "cases"],
        b.block(
            vec![b.let_(
                "cmetas",
                b.gen_strict(b.each(
                    "c",
                    b.loc("cases"),
                    b.match_(
                        b.loc("c"),
                        vec![arm(
                            triple_pat(b, "ft", "sn", "its"),
                            b.yield_(b.tuple(vec![
                                b.loc("ft"),
                                b.loc("sn"),
                                b.gen_strict(b.each(
                                    "fs",
                                    b.loc("its"),
                                    b.yield_(codec_part(b, "Helper.codec_meta", "fs")),
                                )),
                            ])),
                        )],
                    ),
                )),
            )],
            make_schema(
                b,
                b.app2(rt(b, "Helper.validate_adt"), b.loc("name"), b.loc("cases")),
                make_codec(
                    b,
                    b.app(rt(b, "Helper.decode_adt"), b.loc("cases")),
                    b.app(rt(b, "Helper.encode_adt"), b.loc("cases")),
                    is_record_wire(b),
                    meta_con(b, "$Adt", vec![b.loc("cmetas")]),
                    extra_con(b, "$None", vec![]),
                ),
                kind_con(b, "$Adt", vec![b.loc("name"), b.loc("cases")]),
                b.none(),
            ),
        ),
    )
}
