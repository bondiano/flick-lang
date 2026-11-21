//! `HashSet`/`HashMap`, `ByteSize`, and `JsonSchema` codec adapters.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_codec::{REFUSE_FALLIBLE, leaf_codec};
use super::super::std_hash_map::witness_from_dict;
use super::super::std_schema_rt::{extra_con, make_codec, meta_con, rt, str_lit};
use super::super::{Expr, Pat};

pub(in super::super) fn hash_set_json(b: &ExprBuilder) -> Expr {
    b.lam(
        &["hdict", "item"],
        b.block(
            [b.let_("w", witness_from_dict(b, "hdict"))],
            b.app3(
                b.global("Codec", "transform"),
                b.app(b.global("HashSet", "from_list"), b.loc("w")),
                b.global("HashSet", "to_list"),
                b.app(b.global("Codec", "list"), b.loc("item")),
            ),
        ),
    )
}

pub(in super::super) fn hash_map_json(b: &ExprBuilder) -> Expr {
    let dec = b.app(
        rt(b, "Helper.decode_tuple"),
        b.list([
            b.app(rt(b, "Helper.codec_dec"), b.loc("kc")),
            b.app(rt(b, "Helper.codec_dec"), b.loc("vc")),
        ]),
    );
    let enc = b.app(
        rt(b, "Helper.encode_tuple"),
        b.list([
            b.app(rt(b, "Helper.codec_enc"), b.loc("kc")),
            b.app(rt(b, "Helper.codec_enc"), b.loc("vc")),
        ]),
    );
    let acc = b.lam(
        &["in"],
        b.match_(
            b.app(rt(b, "Schema.ListFromWire"), b.loc("in")),
            [
                arm(b.some_pat("_"), b.bool_(true)),
                arm(Pat::Wild, b.bool_(false)),
            ],
        ),
    );
    let meta = meta_con(
        b,
        "$Tuple",
        vec![b.list([
            b.app(rt(b, "Helper.codec_meta"), b.loc("kc")),
            b.app(rt(b, "Helper.codec_meta"), b.loc("vc")),
        ])],
    );
    let pair_c = make_codec(b, dec, enc, acc, meta, extra_con(b, "$None", vec![]));
    b.lam(
        &["hdict", "kc", "vc"],
        b.block(
            [
                b.let_("w", witness_from_dict(b, "hdict")),
                b.let_("pair_c", pair_c),
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

pub(in super::super) fn bytesize_codec_template(
    b: &ExprBuilder,
    bind: &str,
    stages: usize,
) -> Option<Expr> {
    let rf = || meta_con(b, "$Refused", vec![str_lit(b, REFUSE_FALLIBLE)]);
    match (bind, stages) {
        ("flickJson", 0) => Some(leaf_codec(b, "bytesize", rf())),
        ("flickJsonNumber", 0) => Some(leaf_codec(b, "bytesize_number", rf())),
        ("flickJsonBigint", 0) => Some(leaf_codec(b, "bytesize_bigint", rf())),
        _ => None,
    }
}

pub(in super::super) fn json_schema_template(
    b: &ExprBuilder,
    bind: &str,
    stages: usize,
) -> Option<Expr> {
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
