//! Synthetic `$rt` global lowering for `Schema`, `Codec`, `Check`, `Route`, and `JsonSchema`.

use flick_ir::memory::{GenIntrinsic, JsonIntrinsic, JsonSchemaIntrinsic, SchemaIntrinsic};
use flick_ir::{BinOp, Handler};

use super::expr_builder::{ExprBuilder, arm};
use super::{
    Code, Env, Expr, ExprKind, Global, Intrinsic, Lit, Lower, LowerError, Name, Pat, Span,
};
use crate::source::FileId;

impl Lower<'_> {
    pub(super) fn synth_rt_global(
        &mut self,
        name: &Name,
        span: Option<Span>,
    ) -> Result<(), LowerError> {
        let span = span.unwrap_or_else(|| Span::new(FileId::new(0), 0, 0));
        let code = self.reserve(span)?;
        self.initializers.insert(name.clone(), code);
        self.program.globals.push(Global {
            name: name.clone(),
            initializer: code,
        });
        let mut block = Code::new(vec![]);
        let value = if let Some(op_name) = name.name.strip_prefix("Schema.") {
            let op =
                schema_intrinsic_by_name(op_name).ok_or_else(|| LowerError::UnknownGlobal {
                    name: name.clone(),
                    span: Some(span),
                })?;
            self.intrinsic(Intrinsic::Schema(op), false, vec![], &mut block, span)?
        } else if let Some(op_name) = name.name.strip_prefix("JsonSchema.") {
            let op = json_schema_intrinsic_by_name(op_name).ok_or_else(|| {
                LowerError::UnknownGlobal {
                    name: name.clone(),
                    span: Some(span),
                }
            })?;
            self.intrinsic(Intrinsic::JsonSchema(op), false, vec![], &mut block, span)?
        } else if name.name == "Gen.Step" {
            self.intrinsic(
                Intrinsic::Gen(GenIntrinsic::Step),
                false,
                vec![],
                &mut block,
                span,
            )?
        } else if name.name == "Json.Parse" {
            self.intrinsic(
                Intrinsic::Json(JsonIntrinsic::Parse),
                false,
                vec![],
                &mut block,
                span,
            )?
        } else if name.name == "Json.Stringify" {
            self.intrinsic(
                Intrinsic::Json(JsonIntrinsic::Stringify),
                false,
                vec![],
                &mut block,
                span,
            )?
        } else if name.name == "Int.ToBigInt" {
            self.intrinsic(Intrinsic::ToBigInt, false, vec![], &mut block, span)?
        } else if name.name == "Metric.Registry" {
            let b = ExprBuilder::new(span, "$mr");
            let init = b.rmake(b.list(vec![]));
            self.expr(&init, &Env::new(), &mut block)?
        } else if let Some(kind) = super::std_ext_rt::ext_rt_intrinsic(&name.name) {
            self.intrinsic(kind, false, vec![], &mut block, span)?
        } else if let Some(helper) = name.name.strip_prefix("Helper.") {
            let expr = helper_expr(helper, span)
                .or_else(|| super::std_schema_rt_more::helper_expr(helper, span))
                .or_else(|| super::std_schema_rt_route::helper_expr(helper, span))
                .or_else(|| super::std_config::config_helper_expr(helper, span))
                .or_else(|| super::std_config_prov::prov_helper_expr(helper, span))
                .or_else(|| super::std_arbitrary_model::arb_model_helper(helper, span))
                .or_else(|| super::std_arbitrary_entries::arb_entries_helper(helper, span))
                .or_else(|| super::std_arbitrary_run::arb_run_helper(helper, span))
                .or_else(|| super::std_arbitrary_node::arb_node_helper(helper, span))
                .or_else(|| super::std_arbitrary_schema::arb_schema_helper(helper, span))
                .or_else(|| super::std_channel_core::channel_core_helper(helper, span))
                .or_else(|| super::std_channel_transform::channel_transform_helper(helper, span))
                .or_else(|| super::std_channel_text::channel_text_helper(helper, span))
                .or_else(|| super::std_channel_control::channel_control_helper(helper, span))
                .or_else(|| super::std_channel_run::channel_run_helper(helper, span))
                .or_else(|| super::std_sink_core::sink_core_helper(helper, span))
                .or_else(|| super::std_sink_folds::sink_folds_helper(helper, span))
                .or_else(|| super::std_stream_core::stream_core_helper(helper, span))
                .or_else(|| super::std_stream_transform::stream_transform_helper(helper, span))
                .or_else(|| super::std_stream_window::stream_window_helper(helper, span))
                .or_else(|| super::std_stream_combine::stream_combine_helper(helper, span))
                .or_else(|| super::std_stream_split::stream_split_helper(helper, span))
                .or_else(|| super::std_stream_pipe::stream_pipe_helper(helper, span))
                .or_else(|| super::std_sql::sql_helper_expr(helper, span))
                .ok_or_else(|| LowerError::UnknownGlobal {
                    name: name.clone(),
                    span: Some(span),
                })?;
            self.expr(&expr, &Env::new(), &mut block)?
        } else {
            return Err(LowerError::UnknownGlobal {
                name: name.clone(),
                span: Some(span),
            });
        };
        self.finish(code, &block.finish(value))
    }
}

fn schema_intrinsic_by_name(name: &str) -> Option<SchemaIntrinsic> {
    use SchemaIntrinsic as S;
    Some(match name {
        "JsonNull" => S::JsonNull,
        "CheckInert" => S::CheckInert,
        "DescribeInput" => S::DescribeInput,
        "JoinErrorMessages" => S::JoinErrorMessages,
        "TagOf" => S::TagOf,
        "DeconstructAdt" => S::DeconstructAdt,
        "ConstructAdt" => S::ConstructAdt,
        "RecordEntries" => S::RecordEntries,
        "RecordFromEntries" => S::RecordFromEntries,
        "RecordFromSortedEntries" => S::RecordFromSortedEntries,
        "RecordLookup" => S::RecordLookup,
        "TupleToList" => S::TupleToList,
        "ListFromWire" => S::ListFromWire,
        "TupleFromList" => S::TupleFromList,
        "IsJsonNull" => S::IsJsonNull,
        "IsRedacted" => S::IsRedacted,
        "IsChunk" => S::IsChunk,
        "CheckEmail" => S::CheckEmail,
        "CheckNonEmpty" => S::CheckNonEmpty,
        "CheckPositive" => S::CheckPositive,
        "FormatValue" => S::FormatValue,
        "RouteParseUrl" => S::RouteParseUrl,
        "RouteCoerceCandidates" => S::RouteCoerceCandidates,
        "RouteJsString" => S::RouteJsString,
        "RouteFormatUrl" => S::RouteFormatUrl,
        "Defect" => S::Defect,
        "PrefixIssues" => S::PrefixIssues,
        "CombineErrors" => S::CombineErrors,
        "DecodeLeaf" => S::DecodeLeaf,
        "EncodeLeaf" => S::EncodeLeaf,
        "ValidateLeaf" => S::ValidateLeaf,
        "AcceptsLeaf" => S::AcceptsLeaf,
        "LiteralAccepts" => S::LiteralAccepts,
        "LiteralDecode" => S::LiteralDecode,
        "SplitDecode" => S::SplitDecode,
        "SplitEncode" => S::SplitEncode,
        "SplitKvDecode" => S::SplitKvDecode,
        "SplitKvEncode" => S::SplitKvEncode,
        "MakeCheck" => S::MakeCheck,
        "RunCheck" => S::RunCheck,
        "SameValue" => S::SameValue,
        "EncodeXml" => S::EncodeXml,
        "ExportJsonSchema" => S::ExportJsonSchema,
        "DifferJsonPatchOps" => S::DifferJsonPatchOps,
        "DifferApplyJsonPatch" => S::DifferApplyJsonPatch,
        _ => return None,
    })
}

fn json_schema_intrinsic_by_name(name: &str) -> Option<JsonSchemaIntrinsic> {
    use JsonSchemaIntrinsic as J;
    Some(match name {
        "FromDraft07" => J::FromDraft07,
        "FromDraft202012" => J::FromDraft202012,
        "FromOpenApi31" => J::FromOpenApi31,
        "FromOpenApi30" => J::FromOpenApi30,
        "ToDraft07" => J::ToDraft07,
        "ToDraft04" => J::ToDraft04,
        "ToOpenApi31" => J::ToOpenApi31,
        _ => return None,
    })
}

pub(super) fn str_lit(b: &ExprBuilder, s: &str) -> Expr {
    b.at(ExprKind::Lit(Lit::String(s.into())))
}

pub(super) fn rt(b: &ExprBuilder, name: &str) -> Expr {
    b.global("$rt", name)
}

pub(super) fn cat(b: &ExprBuilder, a: Expr, c: Expr) -> Expr {
    b.bin(BinOp::Concat, a, c)
}

pub(super) fn con_app(b: &ExprBuilder, m: &str, ty: &str, c: &str, args: Vec<Expr>) -> Expr {
    let head = b.at(ExprKind::Con(ExprBuilder::con(m, ty, c, args.len())));
    args.into_iter().fold(head, |acc, arg| b.app(acc, arg))
}

pub(super) fn pair_pat(b: &ExprBuilder, a: &str, c: &str) -> Pat {
    b.tup_pat(&[a, c])
}

pub(super) fn triple_pat(b: &ExprBuilder, a: &str, c: &str, d: &str) -> Pat {
    b.tup_pat(&[a, c, d])
}

pub(super) fn make_codec(
    b: &ExprBuilder,
    dec: Expr,
    enc: Expr,
    acc: Expr,
    meta: Expr,
    extra: Expr,
) -> Expr {
    con_app(
        b,
        "Codec",
        "Codec",
        "$Codec",
        vec![dec, enc, acc, meta, extra],
    )
}

pub(super) fn make_schema(
    b: &ExprBuilder,
    val: Expr,
    codec: Expr,
    kind: Expr,
    gen_opt: Expr,
) -> Expr {
    con_app(
        b,
        "Schema",
        "Schema",
        "$Schema",
        vec![val, codec, kind, gen_opt],
    )
}

pub(super) fn meta_con(b: &ExprBuilder, name: &str, args: Vec<Expr>) -> Expr {
    con_app(b, "Codec", "$Meta", name, args)
}

pub(super) fn extra_con(b: &ExprBuilder, name: &str, args: Vec<Expr>) -> Expr {
    con_app(b, "Codec", "$Extra", name, args)
}

pub(super) fn kind_con(b: &ExprBuilder, name: &str, args: Vec<Expr>) -> Expr {
    con_app(b, "Schema", "$Kind", name, args)
}

pub(super) fn schema_err(b: &ExprBuilder, msg: Expr) -> Expr {
    let issue = b.at(ExprKind::Record(vec![
        ("message".into(), msg),
        ("path".into(), str_lit(b, "(root)")),
    ]));
    con_app(
        b,
        "Prelude",
        "SchemaError",
        "SchemaError",
        vec![b.list(vec![issue])],
    )
}

pub(super) fn expected_err(b: &ExprBuilder, what: Expr, input: Expr) -> Expr {
    let desc = b.app(rt(b, "Schema.DescribeInput"), input);
    let msg = cat(
        b,
        str_lit(b, "expected "),
        cat(b, what, cat(b, str_lit(b, ", got "), desc)),
    );
    b.err(schema_err(b, msg))
}

pub(super) fn map_result_list(
    b: &ExprBuilder,
    loop_id: &str,
    src: Expr,
    item_var: &str,
    idx_var: &str,
    step_res: Expr,
    on_err: impl FnOnce(Expr, Expr) -> Expr,
) -> Expr {
    let err_expr = on_err(b.var(idx_var), b.loc("$e"));
    b.block(
        vec![
            b.var_decl("$err", b.none()),
            b.var_decl(idx_var, b.int(0)),
            b.let_(
                "$out",
                b.gen_strict(b.for_ctrl(
                    loop_id,
                    item_var,
                    src,
                    b.match_(
                        step_res,
                        vec![
                            arm(
                                b.ok_pat("$v"),
                                b.block(
                                    vec![b.assign(idx_var, b.add_i(b.var(idx_var), b.int(1)))],
                                    b.yield_(b.loc("$v")),
                                ),
                            ),
                            arm(
                                b.err_pat("$e"),
                                b.block(
                                    vec![b.assign("$err", b.some(err_expr))],
                                    b.break_(loop_id, b.unit()),
                                ),
                            ),
                        ],
                    ),
                )),
            ),
        ],
        b.match_(
            b.var("$err"),
            vec![
                arm(b.some_pat("$fe"), b.err(b.loc("$fe"))),
                arm(Pat::Wild, b.ok(b.loc("$out"))),
            ],
        ),
    )
}

fn codec_accessor(b: &ExprBuilder, idx: usize) -> Expr {
    let mut pats = vec![Pat::Wild; 5];
    pats[idx] = Pat::Bind(b.name("f"));
    b.lam(
        &["c"],
        b.match_(
            b.loc("c"),
            vec![arm(
                Pat::Con(ExprBuilder::con("Codec", "Codec", "$Codec", 5), pats),
                b.loc("f"),
            )],
        ),
    )
}

fn schema_accessor(b: &ExprBuilder, idx: usize) -> Expr {
    let mut pats = vec![Pat::Wild; 4];
    pats[idx] = Pat::Bind(b.name("f"));
    b.lam(
        &["s"],
        b.match_(
            b.loc("s"),
            vec![arm(
                Pat::Con(ExprBuilder::con("Schema", "Schema", "$Schema", 4), pats),
                b.loc("f"),
            )],
        ),
    )
}

fn helper_expr(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$srt");
    match name {
        "codec_dec" => Some(codec_accessor(&b, 0)),
        "codec_enc" => Some(codec_accessor(&b, 1)),
        "codec_acc" => Some(codec_accessor(&b, 2)),
        "codec_meta" => Some(codec_accessor(&b, 3)),
        "codec_extra" => Some(codec_accessor(&b, 4)),
        "schema_val" => Some(schema_accessor(&b, 0)),
        "schema_codec" => Some(schema_accessor(&b, 1)),
        "schema_kind" => Some(schema_accessor(&b, 2)),
        "schema_gen" => Some(schema_accessor(&b, 3)),
        "schema_task" => Some(schema_task_helper(&b)),
        "apply_units" => Some(apply_units_helper(&b)),
        "list_at" => Some(b.lam(
            &["i", "xs"],
            b.match_(
                b.app2(b.global("List.Core", "get"), b.loc("i"), b.loc("xs")),
                vec![
                    arm(b.some_pat("v"), b.loc("v")),
                    arm(
                        Pat::Wild,
                        b.app(rt(&b, "Schema.Defect"), str_lit(&b, "index out of bounds")),
                    ),
                ],
            ),
        )),
        "list_cons" => Some(b.lam(
            &["h", "t"],
            b.app2(
                b.global("List.Core", "append"),
                b.list(vec![b.loc("h")]),
                b.loc("t"),
            ),
        )),
        "decode_list" => Some(decode_list_helper(&b)),
        "encode_list" => Some(encode_list_helper(&b)),
        "decode_tuple" => Some(super::std_schema_rt_obj::decode_tuple_helper(&b)),
        "encode_tuple" => Some(super::std_schema_rt_obj::encode_tuple_helper(&b)),
        "decode_object" => Some(super::std_schema_rt_obj::decode_object_helper(&b)),
        "encode_record" => Some(super::std_schema_rt_obj::encode_record_helper(&b)),
        "decode_adt" => Some(super::std_schema_rt_adt::decode_adt_helper(&b)),
        "encode_adt" => Some(super::std_schema_rt_adt::encode_adt_helper(&b)),
        "validate_list" => Some(super::std_schema_rt_val::validate_list_helper(&b)),
        "validate_tuple" => Some(super::std_schema_rt_val::validate_tuple_helper(&b)),
        "validate_record" => Some(super::std_schema_rt_val::validate_record_helper(&b)),
        "validate_adt" => Some(super::std_schema_rt_val::validate_adt_helper(&b)),
        _ => None,
    }
}

fn schema_task_helper(b: &ExprBuilder) -> Expr {
    let se_con = ExprBuilder::con("Prelude", "SchemaError", "SchemaError", 1);
    let body = b.ok(b.app(b.loc("thunk"), b.unit()));
    let caught = b.err(b.app(
        rt(b, "Schema.JoinErrorMessages"),
        con_app(
            b,
            "Prelude",
            "SchemaError",
            "SchemaError",
            vec![b.loc("issues")],
        ),
    ));
    let handler = Handler {
        body,
        effects: vec![],
        ret: None,
        errors: vec![arm(
            Pat::Con(se_con, vec![Pat::Bind(b.name("issues"))]),
            caught,
        )],
    };
    b.lam(&["thunk"], b.at(ExprKind::Handle(Box::new(handler))))
}

fn apply_units_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["ctor", "n"],
        b.if_(
            b.bin(BinOp::Le, b.loc("n"), b.int(0)),
            b.loc("ctor"),
            b.app2(
                rt(b, "Helper.apply_units"),
                b.app(b.loc("ctor"), b.unit()),
                b.sub_i(b.loc("n"), b.int(1)),
            ),
        ),
    )
}

fn decode_list_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["dec", "input"],
        b.match_(
            b.app(rt(b, "Schema.ListFromWire"), b.loc("input")),
            vec![
                arm(
                    b.some_pat("xs"),
                    map_result_list(
                        b,
                        "$dl",
                        b.loc("xs"),
                        "x",
                        "i",
                        b.app(b.loc("dec"), b.loc("x")),
                        |idx, err| b.app2(rt(b, "Schema.PrefixIssues"), idx, err),
                    ),
                ),
                arm(
                    Pat::Wild,
                    expected_err(b, str_lit(b, "an array"), b.loc("input")),
                ),
            ],
        ),
    )
}

fn encode_list_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["enc", "xs"],
        map_result_list(
            b,
            "$el",
            b.loc("xs"),
            "x",
            "i",
            b.app(b.loc("enc"), b.loc("x")),
            |idx, err| b.app2(rt(b, "Schema.PrefixIssues"), idx, err),
        ),
    )
}
