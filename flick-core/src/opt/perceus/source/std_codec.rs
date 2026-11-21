//! Audited `Codec`, `Codec.Derive`, `Prelude` DSL, `JsonSchema`, and stdlib JSON codec adapters.

use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::std_codec_meta::{
    codec_check_whole, codec_option_null, codec_transform_effect, codec_transform_pure,
    codec_transform_result, dsl_on_missing,
};
use super::std_schema_rt::{con_app, extra_con, make_codec, meta_con, rt, str_lit};
use super::{Code, Env, Expr, ExprKind, Local, Lower, LowerError, Pat, Span};

pub(super) const REFUSE_FALLIBLE: &str = "a fallible transformation has no JSON Schema form";
pub(super) const REFUSE_FALLBACK: &str =
    "a decoding fallback accepts input the wire schema does not state";
pub(super) const REFUSE_EFFECT: &str = "an effectful transformation decides acceptance at run time";

impl Lower<'_> {
    pub(super) fn std_codec_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if !has_standard_shape(value) {
            return None;
        }
        let from = value.binding.from.as_deref()?;
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$cdc");

        if value.mode == ForeignMode::Effect && value.dicts == 0 {
            let expr = codec_effect_template(&b, from, bind, stages)?;
            return Some(self.expr(&expr, &Env::new(), block));
        }
        if value.mode != ForeignMode::Pure {
            return None;
        }
        if value.dicts == 1 && from == "flick:Schema" {
            let expr = match (bind, stages) {
                ("flickHashSetJson", 1) => Some(super::std_codec_std::hash_set_json(&b)),
                ("flickHashMapJson", 2) => Some(super::std_codec_std::hash_map_json(&b)),
                _ => None,
            }?;
            return Some(self.expr(&expr, &Env::new(), block));
        }
        if value.dicts != 0 {
            return None;
        }
        let expr = match from {
            "flick:rt" => codec_rt_template(&b, bind, stages),
            "flick:Schema" => super::std_codec_std::codec_schema_template(&b, bind, stages),
            "flick:ByteSize" => super::std_codec_std::bytesize_codec_template(&b, bind, stages),
            "flick:JsonSchema" => super::std_codec_std::json_schema_template(&b, bind, stages),
            _ => None,
        }?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

fn codec_effect_template(b: &ExprBuilder, from: &str, bind: &str, stages: usize) -> Option<Expr> {
    match (from, bind, stages) {
        ("flick:rt", "codecDecode", 2) => Some(b.lam(
            &["c", "in"],
            b.app(
                b.global("Prelude", "from_result"),
                b.app2(b.global("Codec", "decode_result"), b.loc("c"), b.loc("in")),
            ),
        )),
        ("flick:rt", "codecEncode", 2) => Some(b.lam(
            &["c", "v"],
            b.app(
                b.global("Prelude", "from_result"),
                b.app2(b.global("Codec", "encode_result"), b.loc("c"), b.loc("v")),
            ),
        )),
        ("flick:Schema", "flickToEncoderXml", 2) => Some(b.lam(
            &["c", "v"],
            b.block(
                vec![
                    b.let_(
                        "wire",
                        b.app2(b.global("Codec", "encode"), b.loc("c"), b.loc("v")),
                    ),
                    b.let_(
                        "root",
                        b.match_(
                            b.field(b.app(b.global("Codec", "annotations"), b.loc("c")), "title"),
                            vec![
                                arm(b.some_pat("t"), b.loc("t")),
                                arm(Pat::Wild, str_lit(b, "root")),
                            ],
                        ),
                    ),
                ],
                b.app2(rt(b, "Schema.EncodeXml"), b.loc("root"), b.loc("wire")),
            ),
        )),
        ("flick:rt", "codecStandardAsync", 1) => Some(b.lam(
            &["c"],
            con_app(b, "Codec", "$Standard", "$Standard", vec![b.loc("c")]),
        )),
        _ => None,
    }
}

pub(super) fn leaf_codec(b: &ExprBuilder, kind: &str, meta: Expr) -> Expr {
    make_codec(
        b,
        b.app(rt(b, "Schema.DecodeLeaf"), str_lit(b, kind)),
        b.app(rt(b, "Schema.EncodeLeaf"), str_lit(b, kind)),
        b.app(rt(b, "Schema.AcceptsLeaf"), str_lit(b, kind)),
        meta,
        extra_con(b, "$None", vec![]),
    )
}

pub(super) fn check_meta_exact(b: &ExprBuilder, desc: &str, fields: Vec<(&str, Expr)>) -> Expr {
    let rec = b.at(ExprKind::Record(
        fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    ));
    con_app(b, "Check", "$Meta", "$Exact", vec![str_lit(b, desc), rec])
}

#[expect(clippy::too_many_lines, reason = "central flick:rt codec dispatch")]
fn codec_rt_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("codecDecodeResult", 2) => Some(b.lam(
            &["c", "in"],
            b.match_(
                b.app(rt(b, "Schema.CheckInert"), b.loc("in")),
                vec![
                    arm(b.some_pat("e"), b.err(b.loc("e"))),
                    arm(
                        Pat::Wild,
                        b.app(b.app(rt(b, "Helper.codec_dec"), b.loc("c")), b.loc("in")),
                    ),
                ],
            ),
        )),
        ("codecEncodeResult", 2) => Some(b.lam(
            &["c", "v"],
            b.app(b.app(rt(b, "Helper.codec_enc"), b.loc("c")), b.loc("v")),
        )),
        ("schemaJsonCodec", 1) => Some(rt(b, "Helper.schema_codec")),
        ("codecBool", 0) => Some(leaf_codec(b, "bool", meta_con(b, "$Bool", vec![]))),
        ("codecInt", 0) => Some(leaf_codec(b, "int", meta_con(b, "$Int", vec![]))),
        ("codecString", 0) => Some(leaf_codec(b, "string", meta_con(b, "$String", vec![]))),
        ("codecNumberFinite", 0) => Some(leaf_codec(b, "number", meta_con(b, "$Number", vec![]))),
        ("codecNumberLossless", 0) => Some(leaf_codec(
            b,
            "lossless_number",
            meta_con(b, "$LosslessNumber", vec![]),
        )),
        ("codecBigint", 0) => Some(leaf_codec(b, "bigint", meta_con(b, "$BigInt", vec![]))),
        ("codecRedactedMasked", 0) => Some(leaf_codec(
            b,
            "masked_redacted",
            meta_con(b, "$MaskedRedacted", vec![]),
        )),
        ("codecRedactedReveal", 1) => Some(b.lam(
            &["item"],
            make_codec(
                b,
                b.lam(
                    &["in"],
                    b.match_(
                        b.app(b.app(rt(b, "Helper.codec_dec"), b.loc("item")), b.loc("in")),
                        vec![
                            arm(
                                b.ok_pat("v"),
                                b.ok(con_app(
                                    b,
                                    "Redacted",
                                    "Redacted",
                                    "$Redacted",
                                    vec![b.loc("v")],
                                )),
                            ),
                            arm(b.err_pat("e"), b.err(b.loc("e"))),
                        ],
                    ),
                ),
                b.lam(
                    &["r"],
                    b.match_(
                        b.loc("r"),
                        vec![arm(
                            Pat::Con(
                                ExprBuilder::con("Redacted", "Redacted", "$Redacted", 1),
                                vec![Pat::Bind(b.name("v"))],
                            ),
                            b.app(b.app(rt(b, "Helper.codec_enc"), b.loc("item")), b.loc("v")),
                        )],
                    ),
                ),
                b.app(rt(b, "Helper.codec_acc"), b.loc("item")),
                meta_con(
                    b,
                    "$Stage",
                    vec![b.app(rt(b, "Helper.codec_meta"), b.loc("item"))],
                ),
                extra_con(b, "$None", vec![]),
            ),
        )),
        ("codecList", 1) => Some(b.lam(
            &["item"],
            make_codec(
                b,
                b.app(
                    rt(b, "Helper.decode_list"),
                    b.app(rt(b, "Helper.codec_dec"), b.loc("item")),
                ),
                b.app(
                    rt(b, "Helper.encode_list"),
                    b.app(rt(b, "Helper.codec_enc"), b.loc("item")),
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
                    "$List",
                    vec![b.app(rt(b, "Helper.codec_meta"), b.loc("item"))],
                ),
                extra_con(b, "$None", vec![]),
            ),
        )),
        ("codecOptionNull", 1) => Some(codec_option_null(b, false)),
        ("codecOptionNullish", 1) => Some(codec_option_null(b, true)),
        ("codecTaggedOption", 1) => Some(super::std_codec_ops::codec_tagged_option(b)),
        ("codecTransformPure", 3) => Some(codec_transform_pure(b)),
        ("codecTransformAs", 4) => Some(b.lam(
            &["_s", "dec", "enc", "from"],
            b.app3(
                codec_transform_pure(b),
                b.loc("dec"),
                b.loc("enc"),
                b.loc("from"),
            ),
        )),
        ("codecTransformResult", 3) => Some(codec_transform_result(b)),
        ("codecTransformEffect", 3) => Some(codec_transform_effect(b)),
        ("codecCheckWhole", 3) => Some(codec_check_whole(b)),
        ("codecJsonSchema", 2) => Some(b.lam(
            &["root", "c"],
            b.app2(
                rt(b, "Schema.ExportJsonSchema"),
                b.loc("root"),
                b.app2(
                    rt(b, "Helper.resolve_meta"),
                    b.list(vec![]),
                    b.app(rt(b, "Helper.codec_meta"), b.loc("c")),
                ),
            ),
        )),
        ("codecStandard", 1) => Some(b.lam(
            &["c"],
            con_app(b, "Codec", "$Standard", "$Standard", vec![b.loc("c")]),
        )),
        ("dslField" | "dslFieldEffect", 2) => Some(b.lam(
            &["w", "c"],
            con_app(
                b,
                "Codec",
                "$Field",
                "$Field",
                vec![b.loc("w"), b.loc("c"), b.bool_(false), b.none()],
            ),
        )),
        ("dslOptionalField", 2) => Some(b.lam(
            &["w", "c"],
            con_app(
                b,
                "Codec",
                "$Field",
                "$Field",
                vec![b.loc("w"), b.loc("c"), b.bool_(true), b.none()],
            ),
        )),
        ("dslOnMissing", 2) => Some(dsl_on_missing(b, false)),
        ("dslOnMissingEffect", 2) => Some(dsl_on_missing(b, true)),
        ("codecFieldsEmpty", 0) => Some(b.list(vec![])),
        ("codecField", 3) => Some(b.lam(
            &["lbl", "spec", "rest"],
            b.app2(
                rt(b, "Helper.list_cons"),
                b.tuple(vec![b.loc("lbl"), b.loc("spec")]),
                b.loc("rest"),
            ),
        )),
        ("codecRecord", 3) => Some(b.lam(
            &["name", "_schema", "rev_fields"],
            b.app3(
                rt(b, "Helper.make_block_codec"),
                b.loc("name"),
                b.app(b.global("List.Core", "reverse"), b.loc("rev_fields")),
                b.bool_(false),
            ),
        )),
        ("codecIgnoreExtra", 1) => Some(b.lam(
            &["c"],
            b.match_(
                b.app(rt(b, "Helper.codec_extra"), b.loc("c")),
                vec![
                    arm(
                        Pat::Con(
                            ExprBuilder::con("Codec", "$Extra", "$Block", 2),
                            vec![Pat::Bind(b.name("nm")), Pat::Bind(b.name("flds"))],
                        ),
                        b.app3(
                            rt(b, "Helper.make_block_codec"),
                            b.loc("nm"),
                            b.loc("flds"),
                            b.bool_(true),
                        ),
                    ),
                    arm(Pat::Wild, b.loc("c")),
                ],
            ),
        )),
        _ => None,
    }
}
