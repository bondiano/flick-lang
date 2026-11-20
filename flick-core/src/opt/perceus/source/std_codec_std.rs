//! Audited `flick:Schema`, `ByteSize`, `HashSet`/`HashMap`, and `JsonSchema` codec adapters.

use super::expr_builder::{ExprBuilder, arm};
use super::std_codec::{REFUSE_FALLIBLE, check_meta_exact, leaf_codec};
use super::std_codec_ops::{
    codec_annotate, codec_annotations, codec_catch, codec_check_effect, codec_flip,
    codec_json_string, codec_list_ensure, codec_non_empty_list,
};
use super::std_hash_map::witness_from_dict;
use super::std_schema_rt::{
    con_app, extra_con, kind_con, make_codec, make_schema, meta_con, rt, str_lit,
};
use super::{Expr, Pat, Stmt};

#[expect(clippy::too_many_lines, reason = "central flick:Schema codec dispatch")]
pub(super) fn codec_schema_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    let stage_str = || meta_con(b, "$Stage", vec![meta_con(b, "$String", vec![])]);
    let refused_f = || meta_con(b, "$Refused", vec![str_lit(b, REFUSE_FALLIBLE)]);
    match (bind, stages) {
        ("Json", 0) => Some(leaf_codec(b, "json", meta_con(b, "$Json", vec![]))),
        ("Literal", 1) => Some(b.lam(
            &["lit"],
            b.block(
                vec![b.let_("vs", b.list(vec![b.loc("lit")]))],
                make_codec(
                    b,
                    b.app2(rt(b, "Schema.LiteralDecode"), b.loc("vs"), b.bool_(false)),
                    b.app2(rt(b, "Schema.LiteralDecode"), b.loc("vs"), b.bool_(false)),
                    b.app(rt(b, "Schema.LiteralAccepts"), b.loc("vs")),
                    meta_con(b, "$Literal", vec![b.loc("lit")]),
                    extra_con(b, "$None", vec![]),
                ),
            ),
        )),
        ("Literals", 1) => Some(b.lam(
            &["vs"],
            make_codec(
                b,
                b.app2(rt(b, "Schema.LiteralDecode"), b.loc("vs"), b.bool_(true)),
                b.app2(rt(b, "Schema.LiteralDecode"), b.loc("vs"), b.bool_(true)),
                b.app(rt(b, "Schema.LiteralAccepts"), b.loc("vs")),
                meta_con(b, "$Enum", vec![b.loc("vs")]),
                extra_con(b, "$None", vec![]),
            ),
        )),
        ("flickEntries", 1) => Some(b.lam(
            &["vc"],
            b.app2(
                rt(b, "Helper.make_entries_codec"),
                b.loc("vc"),
                b.list(vec![]),
            ),
        )),
        ("NonEmptyArray", 1) => Some(codec_non_empty_list(b)),
        ("ArrayEnsure", 1) => Some(codec_list_ensure(b)),
        ("Trim", 0) => Some(leaf_codec(
            b,
            "trim",
            meta_con(
                b,
                "$Checked",
                vec![
                    stage_str(),
                    b.list(vec![check_meta_exact(
                        b,
                        "a string with no leading or trailing whitespace",
                        vec![("pattern", str_lit(b, r"^\S[\s\S]*\S$|^\S$|^$"))],
                    )]),
                    b.none(),
                    b.none(),
                ],
            ),
        )),
        ("flickToLowerCase", 0) => Some(leaf_codec(b, "to_lower", stage_str())),
        ("flickToUpperCase", 0) => Some(leaf_codec(b, "to_upper", stage_str())),
        ("flickCapitalize", 0) => Some(leaf_codec(b, "capitalize", stage_str())),
        ("flickUncapitalize", 0) => Some(leaf_codec(b, "uncapitalize", stage_str())),
        ("flickSnakeToCamel", 0) => Some(leaf_codec(b, "snake_to_camel", stage_str())),
        ("NumberFromString", 0) => Some(leaf_codec(b, "number_from_string", stage_str())),
        ("flickFiniteFromString", 0) => Some(leaf_codec(b, "finite_from_string", refused_f())),
        ("BooleanFromBit", 0) => Some(leaf_codec(
            b,
            "bool_from_bit",
            meta_con(
                b,
                "$Stage",
                vec![meta_con(
                    b,
                    "$Enum",
                    vec![b.list(vec![b.float(0.0), b.float(1.0)])],
                )],
            ),
        )),
        ("flickStringFromBase64", 0) => Some(leaf_codec(b, "base64_str", refused_f())),
        ("flickStringFromBase64Url", 0) => Some(leaf_codec(b, "base64url_str", refused_f())),
        ("flickStringFromHex", 0) => Some(leaf_codec(b, "hex_str", refused_f())),
        ("flickBytesFromBase64", 0) => Some(leaf_codec(b, "bytes_base64", refused_f())),
        ("flickBytesFromBase64Url", 0) => Some(leaf_codec(b, "bytes_base64url", refused_f())),
        ("flickBytesFromHex", 0) => Some(leaf_codec(b, "bytes_hex", refused_f())),
        ("flickStringFromUriComponent", 0) => Some(leaf_codec(b, "uri_component", refused_f())),
        ("flickFromJsonString", 1) => Some(codec_json_string(b)),
        ("flickSplit", 1) => Some(b.lam(
            &["sep"],
            make_codec(
                b,
                b.app(rt(b, "Schema.SplitDecode"), b.loc("sep")),
                b.app(rt(b, "Schema.SplitEncode"), b.loc("sep")),
                b.app(rt(b, "Schema.AcceptsLeaf"), str_lit(b, "string")),
                stage_str(),
                extra_con(b, "$None", vec![]),
            ),
        )),
        ("flickSplitKeyValue", 2) => Some(b.lam(
            &["sep", "kv"],
            make_codec(
                b,
                b.app2(rt(b, "Schema.SplitKvDecode"), b.loc("sep"), b.loc("kv")),
                b.app2(rt(b, "Schema.SplitKvEncode"), b.loc("sep"), b.loc("kv")),
                b.app(rt(b, "Schema.AcceptsLeaf"), str_lit(b, "string")),
                refused_f(),
                extra_con(b, "$None", vec![]),
            ),
        )),
        ("flickStringTree", 1) => Some(b.lam(
            &["s"],
            b.block(
                vec![Stmt::Expr(b.app2(
                    rt(b, "Helper.check_tree"),
                    b.list(vec![]),
                    b.loc("s"),
                ))],
                make_codec(
                    b,
                    b.app(rt(b, "Helper.decode_tree"), b.loc("s")),
                    b.app(rt(b, "Helper.encode_tree"), b.loc("s")),
                    b.lam(&["_"], b.bool_(true)),
                    refused_f(),
                    extra_con(b, "$None", vec![]),
                ),
            ),
        )),
        ("Natural", 0) => Some(leaf_codec(
            b,
            "natural",
            meta_con(
                b,
                "$Checked",
                vec![
                    meta_con(b, "$Number", vec![]),
                    b.list(vec![
                        check_meta_exact(
                            b,
                            "an integer",
                            vec![
                                ("type", str_lit(b, "integer")),
                                ("minimum", b.float(-9_007_199_254_740_991.0)),
                                ("maximum", b.float(9_007_199_254_740_991.0)),
                            ],
                        ),
                        check_meta_exact(
                            b,
                            "a value greater than or equal to 0",
                            vec![("minimum", b.float(0.0))],
                        ),
                    ]),
                    b.none(),
                    b.none(),
                ],
            ),
        )),
        ("Port", 0) => Some(leaf_codec(
            b,
            "port",
            meta_con(
                b,
                "$Checked",
                vec![
                    meta_con(b, "$Number", vec![]),
                    b.list(vec![
                        check_meta_exact(
                            b,
                            "an integer",
                            vec![
                                ("type", str_lit(b, "integer")),
                                ("minimum", b.float(-9_007_199_254_740_991.0)),
                                ("maximum", b.float(9_007_199_254_740_991.0)),
                            ],
                        ),
                        check_meta_exact(
                            b,
                            "a port number",
                            vec![("minimum", b.float(0.0)), ("maximum", b.float(65535.0))],
                        ),
                    ]),
                    b.none(),
                    b.none(),
                ],
            ),
        )),
        ("Char", 0) => Some(leaf_codec(
            b,
            "char",
            meta_con(
                b,
                "$Checked",
                vec![
                    meta_con(b, "$String", vec![]),
                    b.list(vec![con_app(
                        b,
                        "Check",
                        "$Meta",
                        "$Length",
                        vec![
                            str_lit(b, "a value with a length of 1"),
                            b.some(b.int(1)),
                            b.some(b.int(1)),
                        ],
                    )]),
                    b.none(),
                    b.none(),
                ],
            ),
        )),
        ("flickDurationFromMillis", 0) => Some(leaf_codec(
            b,
            "duration_millis",
            meta_con(
                b,
                "$Stage",
                vec![meta_con(
                    b,
                    "$Checked",
                    vec![
                        meta_con(b, "$Number", vec![]),
                        b.list(vec![check_meta_exact(
                            b,
                            "a value greater than or equal to 0",
                            vec![("minimum", b.float(0.0))],
                        )]),
                        b.none(),
                        b.none(),
                    ],
                )],
            ),
        )),
        ("flickDurationFromNanos", 0) => Some(leaf_codec(
            b,
            "duration_nanos",
            meta_con(
                b,
                "$Stage",
                vec![meta_con(
                    b,
                    "$Checked",
                    vec![
                        meta_con(b, "$String", vec![]),
                        b.list(vec![con_app(
                            b,
                            "Check",
                            "$Meta",
                            "$Approx",
                            vec![str_lit(b, "a string matching the RegExp ^[0-9]+$")],
                        )]),
                        b.none(),
                        b.none(),
                    ],
                )],
            ),
        )),
        ("flickDurationFromString", 0) => Some(leaf_codec(b, "duration_string", refused_f())),
        ("flickBigDecimalFromString", 0) => Some(leaf_codec(b, "decimal", refused_f())),
        ("flickChunkJson", 1) => Some(b.lam(
            &["item"],
            b.app3(
                b.global("Codec", "transform"),
                b.global("Chunk.Core", "from_list"),
                b.global("Chunk.Core", "to_list"),
                b.app(b.global("Codec", "list"), b.loc("item")),
            ),
        )),
        ("flickAnnotate", 2) => Some(codec_annotate(b)),
        ("flickResolveAnnotations", 1) => Some(codec_annotations(b)),
        ("flip", 1) => Some(codec_flip(b)),
        ("flickToType", 1) => Some(b.lam(
            &["c"],
            make_schema(
                b,
                b.lam(
                    &["v"],
                    b.match_(
                        b.app(b.app(rt(b, "Helper.codec_enc"), b.loc("c")), b.loc("v")),
                        vec![
                            arm(b.ok_pat("_"), b.ok(b.loc("v"))),
                            arm(b.err_pat("e"), b.err(b.loc("e"))),
                        ],
                    ),
                ),
                b.loc("c"),
                kind_con(b, "$Needs", vec![str_lit(b, "a declared type")]),
                b.none(),
            ),
        )),
        ("flickToEncoded", 1) => Some(b.lam(
            &["c"],
            b.block(
                vec![b.let_(
                    "wdec",
                    b.lam(
                        &["in"],
                        b.match_(
                            b.app(b.app(rt(b, "Helper.codec_dec"), b.loc("c")), b.loc("in")),
                            vec![
                                arm(b.ok_pat("_"), b.ok(b.loc("in"))),
                                arm(b.err_pat("e"), b.err(b.loc("e"))),
                            ],
                        ),
                    ),
                )],
                make_schema(
                    b,
                    b.loc("wdec"),
                    make_codec(
                        b,
                        b.loc("wdec"),
                        b.loc("wdec"),
                        b.app(rt(b, "Helper.codec_acc"), b.loc("c")),
                        b.app(rt(b, "Helper.codec_meta"), b.loc("c")),
                        extra_con(b, "$None", vec![]),
                    ),
                    kind_con(b, "$Needs", vec![str_lit(b, "a declared type")]),
                    b.none(),
                ),
            ),
        )),
        ("flickCatchDecoding", 2) => Some(codec_catch(b, true, false)),
        ("flickCatchDecodingWithContext", 2) => Some(codec_catch(b, true, true)),
        ("flickCatchEncoding", 2) => Some(codec_catch(b, false, false)),
        ("flickCatchEncodingWithContext", 2) => Some(codec_catch(b, false, true)),
        ("flickCheckEffect", 2) => Some(codec_check_effect(b)),
        _ => None,
    }
}

pub(super) fn hash_set_json(b: &ExprBuilder) -> Expr {
    b.lam(
        &["hdict", "item"],
        b.block(
            vec![b.let_("w", witness_from_dict(b, "hdict"))],
            b.app3(
                b.global("Codec", "transform"),
                b.app(b.global("HashSet", "from_list"), b.loc("w")),
                b.global("HashSet", "to_list"),
                b.app(b.global("Codec", "list"), b.loc("item")),
            ),
        ),
    )
}

pub(super) fn hash_map_json(b: &ExprBuilder) -> Expr {
    b.lam(
        &["hdict", "kc", "vc"],
        b.block(
            vec![
                b.let_("w", witness_from_dict(b, "hdict")),
                b.let_(
                    "pair_c",
                    make_codec(
                        b,
                        b.app(
                            rt(b, "Helper.decode_tuple"),
                            b.list(vec![
                                b.app(rt(b, "Helper.codec_dec"), b.loc("kc")),
                                b.app(rt(b, "Helper.codec_dec"), b.loc("vc")),
                            ]),
                        ),
                        b.app(
                            rt(b, "Helper.encode_tuple"),
                            b.list(vec![
                                b.app(rt(b, "Helper.codec_enc"), b.loc("kc")),
                                b.app(rt(b, "Helper.codec_enc"), b.loc("vc")),
                            ]),
                        ),
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
                        meta_con(
                            b,
                            "$Tuple",
                            vec![b.list(vec![
                                b.app(rt(b, "Helper.codec_meta"), b.loc("kc")),
                                b.app(rt(b, "Helper.codec_meta"), b.loc("vc")),
                            ])],
                        ),
                        extra_con(b, "$None", vec![]),
                    ),
                ),
            ],
            b.app3(
                b.global("Codec", "transform"),
                b.app(b.global("HashMap", "from_list"), b.loc("w")),
                b.global("HashMap", "to_list"),
                b.app(b.global("Codec", "list"), b.loc("pair_c")),
            ),
        ),
    )
}

pub(super) fn bytesize_codec_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    let rf = || meta_con(b, "$Refused", vec![str_lit(b, REFUSE_FALLIBLE)]);
    match (bind, stages) {
        ("flickJson", 0) => Some(leaf_codec(b, "bytesize", rf())),
        ("flickJsonNumber", 0) => Some(leaf_codec(b, "bytesize_number", rf())),
        ("flickJsonBigint", 0) => Some(leaf_codec(b, "bytesize_bigint", rf())),
        _ => None,
    }
}

pub(super) fn json_schema_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("META_SCHEMA_URI_DRAFT_04", 0) => {
            Some(str_lit(b, "http://json-schema.org/draft-04/schema#"))
        }
        ("META_SCHEMA_URI_DRAFT_07", 0) => {
            Some(str_lit(b, "http://json-schema.org/draft-07/schema#"))
        }
        ("META_SCHEMA_URI_DRAFT_2020_12", 0) => {
            Some(str_lit(b, "https://json-schema.org/draft/2020-12/schema"))
        }
        ("flickFromSchemaDraft07", 1) => Some(rt(b, "JsonSchema.FromDraft07")),
        ("flickFromSchemaDraft2020_12", 1) => Some(rt(b, "JsonSchema.FromDraft202012")),
        ("flickFromSchemaOpenApi3_1", 1) => Some(rt(b, "JsonSchema.FromOpenApi31")),
        ("flickFromSchemaOpenApi3_0", 1) => Some(rt(b, "JsonSchema.FromOpenApi30")),
        ("flickToDocumentDraft07", 1) => Some(rt(b, "JsonSchema.ToDraft07")),
        ("flickToDocumentDraft04", 1) => Some(rt(b, "JsonSchema.ToDraft04")),
        ("toMultiDocumentOpenApi3_1", 1) => Some(rt(b, "JsonSchema.ToOpenApi31")),
        _ => None,
    }
}
