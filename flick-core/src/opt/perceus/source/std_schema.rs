//! Audited `Schema`, `Checks`, `Route`, and `Differ.json_patch` pure IR adapters.

use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::std_schema_rt::{
    con_app, expected_err, extra_con, kind_con, make_codec, make_schema, meta_con, pair_pat, rt,
    schema_err, str_lit,
};
use super::{Code, Env, Expr, Local, Lower, LowerError, Pat, Span, Stmt};

impl Lower<'_> {
    pub(super) fn std_schema_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.mode != ForeignMode::Pure || value.dicts != 0 {
            return None;
        }
        let from = value.binding.from.as_deref()?;
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();

        let is_route_parse =
            from == "flick:rt" && bind == "routeParse" && value.adapter.result.nullable;
        if is_route_parse {
            if value.catch.is_some()
                || value.adapter.receiver
                || value.adapter.cancel
                || !value.adapter.optional.is_empty()
                || value.adapter.variadic
                || stages != 2
            {
                return None;
            }
            let expr =
                super::std_schema_route::route_parse_template(&ExprBuilder::new(span, "$sch"));
            return Some(self.expr(&expr, &Env::new(), block));
        }
        if !has_standard_shape(value) {
            return None;
        }
        let b = ExprBuilder::new(span, "$sch");
        let expr = match from {
            "flick:rt" => schema_rt_template(&b, bind, stages)
                .or_else(|| super::std_schema_route::route_rt_template(&b, bind, stages)),
            "flick:Schema" => schema_std_template(&b, bind, stages),
            _ => None,
        }?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

fn leaf_schema(b: &ExprBuilder, kind: &str, meta_tag: &str, sk_tag: &str) -> Expr {
    let dec = b.app(rt(b, "Schema.DecodeLeaf"), str_lit(b, kind));
    let enc = b.app(rt(b, "Schema.EncodeLeaf"), str_lit(b, kind));
    let acc = b.app(rt(b, "Schema.AcceptsLeaf"), str_lit(b, kind));
    let val = b.app(rt(b, "Schema.ValidateLeaf"), str_lit(b, kind));
    let codec = make_codec(
        b,
        dec,
        enc,
        acc,
        meta_con(b, meta_tag, vec![]),
        extra_con(b, "$None", vec![]),
    );
    make_schema(b, val, codec, kind_con(b, sk_tag, vec![]), b.none())
}

fn schema_rt_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("checkEmail", 1) => Some(rt(b, "Schema.CheckEmail")),
        ("checkNonEmpty", 1) => Some(rt(b, "Schema.CheckNonEmpty")),
        ("checkPositive", 1) => Some(rt(b, "Schema.CheckPositive")),
        ("schemaNumber", 0) => Some(leaf_schema(b, "number", "$Number", "$Number")),
        ("schemaInt", 0) => Some(leaf_schema(b, "int", "$Int", "$Int")),
        ("schemaString", 0) => Some(leaf_schema(b, "string", "$String", "$String")),
        ("schemaBool", 0) => Some(leaf_schema(b, "bool", "$Bool", "$Bool")),
        ("schemaBigint", 0) => Some(leaf_schema(b, "bigint", "$BigInt", "$BigInt")),
        ("schemaUnit", 0) => Some(leaf_schema(b, "unit", "$Unit", "$Unit")),
        ("schemaNever", 0) => Some(leaf_schema(b, "never", "$Never", "$Never")),
        ("schemaUnknown", 0) => Some(leaf_schema(b, "unknown", "$Unknown", "$Unknown")),
        ("schemaList", 1) => Some(schema_list(b)),
        ("schemaOption", 1) => Some(schema_option(b)),
        ("schemaResult", 2) => Some(super::std_schema_compound::schema_result(b)),
        ("schemaRedacted", 1) => Some(schema_redacted(b)),
        ("schemaOpaque", 2) => Some(schema_opaque(b)),
        ("schemaWithGenerator", 2) => Some(b.lam(
            &["g", "s"],
            make_schema(
                b,
                b.app(rt(b, "Helper.schema_val"), b.loc("s")),
                b.app(rt(b, "Helper.schema_codec"), b.loc("s")),
                b.app(rt(b, "Helper.schema_kind"), b.loc("s")),
                b.some(b.loc("g")),
            ),
        )),
        ("schemaValidate", 2) => Some(b.lam(
            &["s", "input"],
            b.app(
                b.app(rt(b, "Helper.schema_val"), b.loc("s")),
                b.loc("input"),
            ),
        )),
        ("schemaArbitrary", 1) => Some(b.lam(
            &["s", "seed"],
            b.app3(
                rt(b, "Helper.draw_schema"),
                b.loc("s"),
                b.int(0),
                b.loc("seed"),
            ),
        )),
        ("schemaField", 2) => Some(b.lam(&["lbl", "s"], b.tuple(vec![b.loc("lbl"), b.loc("s")]))),
        ("schemaRecordOf", 1) => Some(super::std_schema_compound::schema_record(b)),
        ("schemaItem" | "schemaUnboxed", 1) => Some(b.lam(&["s"], b.loc("s"))),
        ("schemaTupleOf", 1) => Some(super::std_schema_compound::schema_tuple(b)),
        ("schemaCase", 2) => Some(b.lam(
            &["ctor", "items"],
            b.match_(
                b.app(
                    rt(b, "Schema.TagOf"),
                    b.app2(
                        rt(b, "Helper.apply_units"),
                        b.loc("ctor"),
                        b.app(b.global("List.Core", "length"), b.loc("items")),
                    ),
                ),
                vec![arm(
                    pair_pat(b, "ft", "sn"),
                    b.tuple(vec![b.loc("ft"), b.loc("sn"), b.loc("items")]),
                )],
            ),
        )),
        ("schemaAdtOf", 2) => Some(super::std_schema_compound::schema_adt(b)),
        ("schemaLazyOf", 2) => Some(schema_lazy(b)),
        ("schemaRecursive", 2) => Some(schema_recursive(b)),
        ("schemaLiterals", 3) => Some(super::std_schema_enum::schema_literals(b)),
        ("schemaEnumTags", 2) => Some(super::std_schema_enum::schema_enum_tags(b)),
        ("schemaRefinedOf", 4) => Some(super::std_schema_enum::schema_refined(b)),
        _ => None,
    }
}

fn schema_std_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickDuration", 0) => Some(make_schema(
            b,
            b.app(rt(b, "Schema.ValidateLeaf"), str_lit(b, "duration")),
            b.global("Duration", "json_millis"),
            kind_con(b, "$Duration", vec![]),
            b.none(),
        )),
        ("flickBigDecimal", 0) => Some(make_schema(
            b,
            b.app(rt(b, "Schema.ValidateLeaf"), str_lit(b, "decimal")),
            b.global("BigDecimal", "json"),
            kind_con(b, "$BigDecimal", vec![]),
            b.none(),
        )),
        ("flickChunk", 1) => Some(b.lam(
            &["item"],
            make_schema(
                b,
                b.lam(
                    &["input"],
                    b.match_(
                        b.app(rt(b, "Schema.IsChunk"), b.loc("input")),
                        vec![
                            arm(
                                b.some_pat("xs"),
                                b.match_(
                                    b.app2(
                                        rt(b, "Helper.validate_list"),
                                        b.app(rt(b, "Helper.schema_val"), b.loc("item")),
                                        b.loc("xs"),
                                    ),
                                    vec![
                                        arm(
                                            b.ok_pat("vs"),
                                            b.ok(con_app(
                                                b,
                                                "Chunk.Core",
                                                "Chunk",
                                                "$Chunk",
                                                vec![b.loc("vs")],
                                            )),
                                        ),
                                        arm(b.err_pat("e"), b.err(b.loc("e"))),
                                    ],
                                ),
                            ),
                            arm(
                                Pat::Wild,
                                expected_err(b, str_lit(b, "Chunk"), b.loc("input")),
                            ),
                        ],
                    ),
                ),
                b.app(
                    b.global("Chunk", "json"),
                    b.app(rt(b, "Helper.schema_codec"), b.loc("item")),
                ),
                kind_con(b, "$Chunk", vec![b.loc("item")]),
                b.none(),
            ),
        )),
        ("toDifferJsonPatch", 1) => Some(super::std_schema_route::differ_json_patch(b)),
        _ => None,
    }
}

fn schema_list(b: &ExprBuilder) -> Expr {
    b.lam(
        &["item"],
        b.block(
            vec![b.let_("ic", b.app(rt(b, "Helper.schema_codec"), b.loc("item")))],
            make_schema(
                b,
                b.app(
                    rt(b, "Helper.validate_list"),
                    b.app(rt(b, "Helper.schema_val"), b.loc("item")),
                ),
                b.app(b.global("Codec", "list"), b.loc("ic")),
                kind_con(b, "$List", vec![b.loc("item")]),
                b.none(),
            ),
        ),
    )
}

fn schema_option(b: &ExprBuilder) -> Expr {
    b.lam(
        &["item"],
        b.block(
            vec![
                b.let_("ic", b.app(rt(b, "Helper.schema_codec"), b.loc("item"))),
                b.let_("iv", b.app(rt(b, "Helper.schema_val"), b.loc("item"))),
            ],
            make_schema(
                b,
                b.lam(
                    &["input"],
                    b.match_(
                        b.loc("input"),
                        vec![
                            arm(
                                b.some_pat("x"),
                                b.match_(
                                    b.app(b.loc("iv"), b.loc("x")),
                                    vec![
                                        arm(b.ok_pat("v"), b.ok(b.some(b.loc("v")))),
                                        arm(b.err_pat("e"), b.err(b.loc("e"))),
                                    ],
                                ),
                            ),
                            arm(
                                Pat::Con(ExprBuilder::con("Prelude", "Option", "None", 0), vec![]),
                                b.ok(b.none()),
                            ),
                            arm(
                                Pat::Wild,
                                expected_err(b, str_lit(b, "Option"), b.loc("input")),
                            ),
                        ],
                    ),
                ),
                b.app(b.global("Prelude", "option_as_null"), b.loc("ic")),
                kind_con(b, "$Option", vec![b.loc("item")]),
                b.none(),
            ),
        ),
    )
}

fn schema_redacted(b: &ExprBuilder) -> Expr {
    b.lam(
        &["item"],
        make_schema(
            b,
            b.lam(
                &["input"],
                b.match_(
                    b.app(rt(b, "Schema.IsRedacted"), b.loc("input")),
                    vec![
                        arm(
                            b.some_pat("inner"),
                            b.match_(
                                b.app(
                                    b.app(rt(b, "Helper.schema_val"), b.loc("item")),
                                    b.loc("inner"),
                                ),
                                vec![
                                    arm(b.ok_pat("_"), b.ok(b.loc("input"))),
                                    arm(b.err_pat("e"), b.err(b.loc("e"))),
                                ],
                            ),
                        ),
                        arm(
                            Pat::Wild,
                            expected_err(b, str_lit(b, "Redacted"), b.loc("input")),
                        ),
                    ],
                ),
            ),
            make_codec(
                b,
                b.app(rt(b, "Schema.DecodeLeaf"), str_lit(b, "redacted")),
                b.app(rt(b, "Schema.EncodeLeaf"), str_lit(b, "redacted")),
                b.lam(&["_"], b.bool_(true)),
                meta_con(b, "$Redacted", vec![]),
                extra_con(b, "$None", vec![]),
            ),
            kind_con(b, "$Redacted", vec![b.loc("item")]),
            b.none(),
        ),
    )
}

fn schema_opaque(b: &ExprBuilder) -> Expr {
    b.lam(
        &["name", "reason"],
        make_schema(
            b,
            b.lam(&["x"], b.ok(b.loc("x"))),
            make_codec(
                b,
                b.lam(&["_"], b.err(schema_err(b, b.loc("reason")))),
                b.lam(&["_"], b.err(schema_err(b, b.loc("reason")))),
                b.lam(&["_"], b.bool_(true)),
                meta_con(b, "$Refused", vec![b.loc("reason")]),
                extra_con(b, "$None", vec![]),
            ),
            kind_con(b, "$Needs", vec![b.loc("name")]),
            b.none(),
        ),
    )
}

fn schema_lazy(b: &ExprBuilder) -> Expr {
    b.lam(
        &["name", "get"],
        make_schema(
            b,
            b.lam(
                &["in"],
                b.app(
                    b.app(rt(b, "Helper.schema_val"), b.app(b.loc("get"), b.unit())),
                    b.loc("in"),
                ),
            ),
            make_codec(
                b,
                b.lam(
                    &["in"],
                    b.app(
                        b.app(
                            rt(b, "Helper.codec_dec"),
                            b.app(rt(b, "Helper.schema_codec"), b.app(b.loc("get"), b.unit())),
                        ),
                        b.loc("in"),
                    ),
                ),
                b.lam(
                    &["v"],
                    b.app(
                        b.app(
                            rt(b, "Helper.codec_enc"),
                            b.app(rt(b, "Helper.schema_codec"), b.app(b.loc("get"), b.unit())),
                        ),
                        b.loc("v"),
                    ),
                ),
                b.lam(
                    &["in"],
                    b.app(
                        b.app(
                            rt(b, "Helper.codec_acc"),
                            b.app(rt(b, "Helper.schema_codec"), b.app(b.loc("get"), b.unit())),
                        ),
                        b.loc("in"),
                    ),
                ),
                meta_con(b, "$Lazy", vec![b.loc("name"), b.loc("get")]),
                extra_con(b, "$None", vec![]),
            ),
            kind_con(b, "$Lazy", vec![b.loc("name"), b.loc("get")]),
            b.none(),
        ),
    )
}

fn schema_recursive(b: &ExprBuilder) -> Expr {
    // Pure zero-cycle suspension: `let rec self () = Schema.suspend name (fun () -> of (self ())) in of (self ())`
    b.lam(
        &["name", "of"],
        b.block(
            vec![Stmt::Fun {
                name: b.name("self"),
                rec: true,
                type_params: Vec::new(),
                dicts: vec![],
                dict_constraints: Vec::new(),
                ty: None,
                value: b.lam(
                    &["_u"],
                    b.app2(
                        b.global("Schema", "suspend"),
                        b.loc("name"),
                        b.lam(&["_u2"], b.app(b.loc("of"), b.app(b.loc("self"), b.unit()))),
                    ),
                ),
            }],
            b.app(b.loc("of"), b.app(b.loc("self"), b.unit())),
        ),
    )
}
