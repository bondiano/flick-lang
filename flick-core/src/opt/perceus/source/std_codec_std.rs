//! Audited `flick:Schema`, `ByteSize`, `HashSet`/`HashMap`, and `JsonSchema` codec adapters.

mod extra;

use super::expr_builder::{ExprBuilder, arm};
use super::std_codec::{REFUSE_FALLIBLE, check_meta_exact, leaf_codec};
use super::std_codec_ops::{
    codec_annotate, codec_annotations, codec_catch, codec_check_effect, codec_flip,
    codec_json_string, codec_list_ensure, codec_non_empty_list,
};
use super::std_schema_rt::{
    con_app, extra_con, kind_con, make_codec, make_schema, meta_con, rt, str_lit,
};
use super::{Expr, Stmt};

pub(super) use extra::{
    bytesize_codec_template, hash_map_json, hash_set_json, json_schema_template,
};

fn checked_int_meta(b: &ExprBuilder) -> Expr {
    check_meta_exact(
        b,
        "an integer",
        vec![
            ("type", str_lit(b, "integer")),
            ("minimum", b.float(-9_007_199_254_740_991.0)),
            ("maximum", b.float(9_007_199_254_740_991.0)),
        ],
    )
}

fn codec_duration_leaves(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    let refused_f = || meta_con(b, "$Refused", vec![str_lit(b, REFUSE_FALLIBLE)]);
    match (bind, stages) {
        ("flickDurationFromMillis", 0) => {
            let non_neg = check_meta_exact(
                b,
                "a value greater than or equal to 0",
                vec![("minimum", b.float(0.0))],
            );
            let inner = meta_con(
                b,
                "$Checked",
                vec![
                    meta_con(b, "$Number", vec![]),
                    b.list([non_neg]),
                    b.none(),
                    b.none(),
                ],
            );
            Some(leaf_codec_link(
                b,
                "duration_millis",
                meta_con(b, "$Stage", vec![inner]),
                "duration_millis",
            ))
        }
        ("flickDurationFromNanos", 0) => {
            let approx = con_app(
                b,
                "Check",
                "$Meta",
                "$Approx",
                vec![str_lit(b, "a string matching the RegExp ^[0-9]+$")],
            );
            let inner = meta_con(
                b,
                "$Checked",
                vec![
                    meta_con(b, "$String", vec![]),
                    b.list([approx]),
                    b.none(),
                    b.none(),
                ],
            );
            Some(leaf_codec_link(
                b,
                "duration_nanos",
                meta_con(b, "$Stage", vec![inner]),
                "duration_nanos",
            ))
        }
        ("flickDurationFromString", 0) => Some(leaf_codec_link(
            b,
            "duration_string",
            refused_f(),
            "duration_string",
        )),
        ("flickBigDecimalFromString", 0) => Some(leaf_codec(b, "decimal", refused_f())),
        _ => None,
    }
}

fn codec_refined_leaves(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    if let Some(expr) = codec_duration_leaves(b, bind, stages) {
        return Some(expr);
    }
    let stage_str = || meta_con(b, "$Stage", vec![meta_con(b, "$String", vec![])]);
    match (bind, stages) {
        ("Trim", 0) => {
            let pat_check = check_meta_exact(
                b,
                "a string with no leading or trailing whitespace",
                vec![("pattern", str_lit(b, r"^\S[\s\S]*\S$|^\S$|^$"))],
            );
            let meta = meta_con(
                b,
                "$Checked",
                vec![stage_str(), b.list([pat_check]), b.none(), b.none()],
            );
            Some(leaf_codec(b, "trim", meta))
        }
        ("BooleanFromBit", 0) => {
            let enum_meta = meta_con(b, "$Enum", vec![b.list([b.float(0.0), b.float(1.0)])]);
            Some(leaf_codec(
                b,
                "bool_from_bit",
                meta_con(b, "$Stage", vec![enum_meta]),
            ))
        }
        ("Natural", 0) => {
            let non_neg = check_meta_exact(
                b,
                "a value greater than or equal to 0",
                vec![("minimum", b.float(0.0))],
            );
            let meta = meta_con(
                b,
                "$Checked",
                vec![
                    meta_con(b, "$Number", vec![]),
                    b.list([checked_int_meta(b), non_neg]),
                    b.none(),
                    b.none(),
                ],
            );
            Some(leaf_codec(b, "natural", meta))
        }
        ("Port", 0) => {
            let port_rng = check_meta_exact(
                b,
                "a port number",
                vec![("minimum", b.float(0.0)), ("maximum", b.float(65535.0))],
            );
            let meta = meta_con(
                b,
                "$Checked",
                vec![
                    meta_con(b, "$Number", vec![]),
                    b.list([checked_int_meta(b), port_rng]),
                    b.none(),
                    b.none(),
                ],
            );
            Some(leaf_codec(b, "port", meta))
        }
        ("Char", 0) => {
            let len_chk = con_app(
                b,
                "Check",
                "$Meta",
                "$Length",
                vec![
                    str_lit(b, "a value with a length of 1"),
                    b.some(b.int(1)),
                    b.some(b.int(1)),
                ],
            );
            let meta = meta_con(
                b,
                "$Checked",
                vec![
                    meta_con(b, "$String", vec![]),
                    b.list([len_chk]),
                    b.none(),
                    b.none(),
                ],
            );
            Some(leaf_codec(b, "char", meta))
        }
        _ => None,
    }
}

fn codec_view_ops(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickChunkJson", 1) => {
            let inner = b.app3(
                b.global("Codec", "transform"),
                b.global("Chunk.Core", "from_list"),
                b.global("Chunk.Core", "to_list"),
                b.app(b.global("Codec", "list"), b.loc("item")),
            );
            let codec = make_codec(
                b,
                b.app(rt(b, "Helper.codec_dec"), b.loc("inner")),
                b.app(rt(b, "Helper.codec_enc"), b.loc("inner")),
                b.app(rt(b, "Helper.codec_acc"), b.loc("inner")),
                b.app(rt(b, "Helper.codec_meta"), b.loc("inner")),
                extra_con(b, "$ChunkCodec", vec![b.loc("item")]),
            );
            Some(b.lam(&["item"], b.block([b.let_("inner", inner)], codec)))
        }
        ("flickToType", 1) => {
            let check_enc = b.lam(
                &["v"],
                b.match_(
                    b.app(b.app(rt(b, "Helper.codec_enc"), b.loc("c")), b.loc("v")),
                    [
                        arm(b.ok_pat("_"), b.ok(b.loc("v"))),
                        arm(b.err_pat("e"), b.err(b.loc("e"))),
                    ],
                ),
            );
            Some(b.lam(
                &["c"],
                make_schema(
                    b,
                    check_enc,
                    b.loc("c"),
                    kind_con(b, "$CodecType", vec![b.loc("c")]),
                    b.none(),
                ),
            ))
        }
        ("flickToEncoded", 1) => {
            let wdec = b.lam(
                &["in"],
                b.match_(
                    b.app(b.app(rt(b, "Helper.codec_dec"), b.loc("c")), b.loc("in")),
                    [
                        arm(b.ok_pat("_"), b.ok(b.loc("in"))),
                        arm(b.err_pat("e"), b.err(b.loc("e"))),
                    ],
                ),
            );
            let enc_codec = make_codec(
                b,
                b.loc("wdec"),
                b.loc("wdec"),
                b.app(rt(b, "Helper.codec_acc"), b.loc("c")),
                b.app(rt(b, "Helper.codec_meta"), b.loc("c")),
                extra_con(b, "$None", vec![]),
            );
            let schema = make_schema(
                b,
                b.loc("wdec"),
                enc_codec,
                kind_con(b, "$Needs", vec![str_lit(b, "a declared type")]),
                b.none(),
            );
            Some(b.lam(&["c"], b.block([b.let_("wdec", wdec)], schema)))
        }
        _ => None,
    }
}

fn codec_struct_ops(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    if let Some(expr) = codec_view_ops(b, bind, stages) {
        return Some(expr);
    }
    let stage_str = || meta_con(b, "$Stage", vec![meta_con(b, "$String", vec![])]);
    let refused_f = || meta_con(b, "$Refused", vec![str_lit(b, REFUSE_FALLIBLE)]);
    match (bind, stages) {
        ("Literal", 1) => Some(b.lam(
            &["lit"],
            b.block(
                [b.let_("vs", b.list([b.loc("lit")]))],
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
                [Stmt::Expr(b.app2(
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
        _ => None,
    }
}

pub(super) fn codec_schema_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    if let Some(expr) = codec_refined_leaves(b, bind, stages) {
        return Some(expr);
    }
    if let Some(expr) = codec_struct_ops(b, bind, stages) {
        return Some(expr);
    }
    let stage_str = || meta_con(b, "$Stage", vec![meta_con(b, "$String", vec![])]);
    let refused_f = || meta_con(b, "$Refused", vec![str_lit(b, REFUSE_FALLIBLE)]);
    match (bind, stages) {
        ("Json", 0) => Some(leaf_codec(b, "json", meta_con(b, "$Json", vec![]))),
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
        ("flickToLowerCase", 0) => Some(leaf_codec(b, "to_lower", stage_str())),
        ("flickToUpperCase", 0) => Some(leaf_codec(b, "to_upper", stage_str())),
        ("flickCapitalize", 0) => Some(leaf_codec(b, "capitalize", stage_str())),
        ("flickUncapitalize", 0) => Some(leaf_codec(b, "uncapitalize", stage_str())),
        ("flickSnakeToCamel", 0) => Some(leaf_codec(b, "snake_to_camel", stage_str())),
        ("NumberFromString", 0) => Some(leaf_codec(b, "number_from_string", stage_str())),
        ("flickFiniteFromString", 0) => Some(leaf_codec(b, "finite_from_string", refused_f())),
        ("flickStringFromBase64", 0) => Some(leaf_codec(b, "base64_str", refused_f())),
        ("flickStringFromBase64Url", 0) => Some(leaf_codec(b, "base64url_str", refused_f())),
        ("flickStringFromHex", 0) => Some(leaf_codec(b, "hex_str", refused_f())),
        ("flickBytesFromBase64", 0) => Some(leaf_codec(b, "bytes_base64", refused_f())),
        ("flickBytesFromBase64Url", 0) => Some(leaf_codec(b, "bytes_base64url", refused_f())),
        ("flickBytesFromHex", 0) => Some(leaf_codec(b, "bytes_hex", refused_f())),
        ("flickStringFromUriComponent", 0) => Some(leaf_codec(b, "uri_component", refused_f())),
        ("flickFromJsonString", 1) => Some(codec_json_string(b)),
        ("flickAnnotate", 2) => Some(codec_annotate(b)),
        ("flickResolveAnnotations", 1) => Some(codec_annotations(b)),
        ("flip", 1) => Some(codec_flip(b)),
        ("flickCatchDecoding", 2) => Some(codec_catch(b, true, false)),
        ("flickCatchDecodingWithContext", 2) => Some(codec_catch(b, true, true)),
        ("flickCatchEncoding", 2) => Some(codec_catch(b, false, false)),
        ("flickCatchEncodingWithContext", 2) => Some(codec_catch(b, false, true)),
        ("flickCheckEffect", 2) => Some(codec_check_effect(b)),
        _ => None,
    }
}

pub(super) fn leaf_codec_link(b: &ExprBuilder, kind: &str, meta: Expr, link: &str) -> Expr {
    make_codec(
        b,
        b.app(rt(b, "Schema.DecodeLeaf"), str_lit(b, kind)),
        b.app(rt(b, "Schema.EncodeLeaf"), str_lit(b, kind)),
        b.app(rt(b, "Schema.AcceptsLeaf"), str_lit(b, kind)),
        meta,
        extra_con(b, "$Link", vec![str_lit(b, link)]),
    )
}
