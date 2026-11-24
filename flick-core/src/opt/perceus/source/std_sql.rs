//! Audited pure `flick:sql` query/template/raw-SQL builders (`Sql`, `Sql.Derive`).

use flick_ir::{Arith, BinOp, Foreign, ForeignMode, Num};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::std_codec::REFUSE_FALLIBLE;
use super::std_schema_rt::{cat, extra_con, make_codec, meta_con, rt, str_lit};
use super::{Code, Env, Expr, ExprKind, Local, Lower, LowerError, Pat, Span, Stmt};

impl Lower<'_> {
    pub(super) fn std_sql_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.binding.from() != Some("flick:sql")
            || value.mode != ForeignMode::Pure
            || value.dicts != 0
        {
            return None;
        }
        let bind = value.binding.export_name().unwrap_or("");
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$sql");

        // `Sql.identifier` uses `nullable` result adaptation (`String -> Option Identifier`).
        if bind == "sqlIdentifier"
            && stages == 1
            && value.catch.is_none()
            && value.adapter.words.is_empty()
            && value.adapter.optional.is_empty()
            && value.adapter.owned.is_empty()
            && value.adapter.result.nullable
        {
            let expr = sql_identifier(&b);
            return Some(self.expr(&expr, &Env::new(), block));
        }

        if !has_standard_shape(value) {
            return None;
        }
        let expr = sql_template_expr(&b, bind, stages)?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

pub(super) fn sql_helper_expr(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$sqh");
    match name {
        "sql_renumber" => Some(sql_renumber_helper(&b)),
        "sql_shift_param" => Some(sql_shift_param_helper(&b)),
        _ => None,
    }
}

fn record(b: &ExprBuilder, fields: Vec<(&str, Expr)>) -> Expr {
    b.at(ExprKind::Record(
        fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    ))
}

fn sql_template_expr(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("sqlTable", 1) => Some(b.lam(&["t"], b.loc("t"))),
        ("sqlColumn", 4) => Some(b.lam(
            &["name", "kind", "nullable", "codec"],
            record(
                b,
                vec![
                    ("name", b.loc("name")),
                    ("kind", b.loc("kind")),
                    ("nullable", b.loc("nullable")),
                    ("codec", b.loc("codec")),
                ],
            ),
        )),
        ("sqlCodec", 2) => Some(sql_codec(b)),
        ("sqlBind", 2) => Some(b.lam(
            &["template", "values"],
            record(
                b,
                vec![
                    ("_tag", str_lit(b, "Typed")),
                    ("template", b.loc("template")),
                    ("values", b.loc("values")),
                ],
            ),
        )),
        ("sqlText", 1) => Some(b.lam(
            &["q"],
            b.if_(
                b.bin(BinOp::Eq, b.field(b.loc("q"), "_tag"), str_lit(b, "Typed")),
                b.field(b.field(b.loc("q"), "template"), "text"),
                b.field(b.loc("q"), "text"),
            ),
        )),
        ("sqlTemplateText", 1) => Some(b.lam(&["t"], b.field(b.loc("t"), "text"))),
        ("sqlParameterCount", 1) => Some(b.lam(
            &["q"],
            b.if_(
                b.bin(BinOp::Eq, b.field(b.loc("q"), "_tag"), str_lit(b, "Typed")),
                b.app(
                    b.global("List.Core", "length"),
                    b.field(b.field(b.loc("q"), "template"), "params"),
                ),
                b.app(
                    b.global("List.Core", "length"),
                    b.field(b.loc("q"), "values"),
                ),
            ),
        )),
        ("sqlRawText", 1) => Some(b.lam(
            &["text"],
            record(
                b,
                vec![("_tag", str_lit(b, "Text")), ("text", b.loc("text"))],
            ),
        )),
        ("sqlRawIdent", 1) => Some(b.lam(
            &["name"],
            record(
                b,
                vec![
                    ("_tag", str_lit(b, "Text")),
                    (
                        "text",
                        cat(
                            b,
                            str_lit(b, "\""),
                            cat(
                                b,
                                b.app3(
                                    rt(b, "String.ReplaceAll"),
                                    str_lit(b, "\""),
                                    str_lit(b, "\"\""),
                                    b.loc("name"),
                                ),
                                str_lit(b, "\""),
                            ),
                        ),
                    ),
                ],
            ),
        )),
        ("sqlRawParam", 2) => Some(b.lam(
            &["codec", "value"],
            record(
                b,
                vec![
                    ("_tag", str_lit(b, "Param")),
                    ("codec", b.loc("codec")),
                    ("value", b.loc("value")),
                ],
            ),
        )),
        ("sqlRawQuery", 2) => Some(sql_raw_query(b)),
        ("sqlTemplate", 1) => Some(sql_template_splice(b)),
        _ => None,
    }
}

fn sql_codec(b: &ExprBuilder) -> Expr {
    let dec_body = b.match_(
        b.app(b.loc("dec"), b.loc("in")),
        [
            arm(b.ok_pat("w"), b.ok(b.loc("w"))),
            arm(
                b.err_pat("e"),
                b.err(b.app(rt(b, "Schema.JoinErrorMessages"), b.loc("e"))),
            ),
        ],
    );
    let dec_fn = b.lam(&["in"], dec_body);
    let enc_fn = b.lam(&["v"], b.ok(b.app(b.loc("enc"), b.loc("v"))));
    let acc_fn = b.lam(&["_"], b.bool_(true));
    let meta = meta_con(b, "$Refused", vec![str_lit(b, REFUSE_FALLIBLE)]);
    let extra = extra_con(b, "$None", vec![]);
    b.lam(
        &["dec", "enc"],
        make_codec(b, dec_fn, enc_fn, acc_fn, meta, extra),
    )
}

fn sql_identifier(b: &ExprBuilder) -> Expr {
    let is_upper = b.bin(
        BinOp::And,
        b.bin(BinOp::Ge, b.loc("c"), b.int(65)),
        b.bin(BinOp::Le, b.loc("c"), b.int(90)),
    );
    let is_lower = b.bin(
        BinOp::And,
        b.bin(BinOp::Ge, b.loc("c"), b.int(97)),
        b.bin(BinOp::Le, b.loc("c"), b.int(122)),
    );
    let is_under = b.bin(BinOp::Eq, b.loc("c"), b.int(95));
    let is_alpha = b.bin(BinOp::Or, b.bin(BinOp::Or, is_upper, is_lower), is_under);
    let is_digit = b.bin(
        BinOp::And,
        b.bin(BinOp::Ge, b.loc("c"), b.int(48)),
        b.bin(BinOp::Le, b.loc("c"), b.int(57)),
    );
    let check_first = b.block(
        [
            b.assign("first", b.bool_(false)),
            Stmt::Expr(b.if_(is_alpha.clone(), b.unit(), b.assign_e("ok", b.bool_(false)))),
        ],
        b.unit(),
    );
    let check_rest = b.if_(
        b.bin(BinOp::Or, is_alpha, is_digit),
        b.unit(),
        b.assign_e("ok", b.bool_(false)),
    );
    let step = b.if_(b.var("first"), check_first, check_rest);
    let check_bytes = b.block(
        [
            b.var_decl("ok", b.bool_(true)),
            b.var_decl("first", b.bool_(true)),
            Stmt::Expr(b.each("c", b.loc("bs"), step)),
        ],
        b.if_(b.var("ok"), b.some(b.loc("name")), b.none()),
    );
    let len_ok = b.bin(
        BinOp::And,
        b.bin(BinOp::Ge, b.loc("n"), b.int(1)),
        b.bin(BinOp::Le, b.loc("n"), b.int(63)),
    );
    let encoded = b.app(rt(b, "Bytes.EncodeUtf8"), b.loc("name"));
    let bs = b.app(rt(b, "Bytes.ToList"), encoded);
    let body = b.block(
        [
            b.let_("bs", bs),
            b.let_("n", b.app(b.global("List.Core", "length"), b.loc("bs"))),
        ],
        b.if_(len_ok, check_bytes, b.none()),
    );
    b.lam(&["name"], body)
}

fn sql_raw_query(b: &ExprBuilder) -> Expr {
    let on_text = b.assign_e(
        "text",
        cat(b, b.var("text"), b.field(b.loc("piece"), "text")),
    );
    let on_param = b.block(
        [
            b.assign("count", b.add_i(b.var("count"), b.int(1))),
            b.assign(
                "text",
                cat(
                    b,
                    b.var("text"),
                    cat(b, str_lit(b, "$"), b.app(rt(b, "Show"), b.var("count"))),
                ),
            ),
        ],
        b.yield_(b.loc("piece")),
    );
    let is_text = b.bin(
        BinOp::Eq,
        b.field(b.loc("piece"), "_tag"),
        str_lit(b, "Text"),
    );
    let values = b.gen_strict(b.each("piece", b.loc("pieces"), b.if_(is_text, on_text, on_param)));
    let result = record(
        b,
        vec![
            ("_tag", str_lit(b, "Raw")),
            ("text", b.var("text")),
            ("values", b.loc("values")),
            ("decoder", b.loc("decoder")),
        ],
    );
    b.lam(
        &["pieces", "decoder"],
        b.block(
            [
                b.var_decl("text", str_lit(b, "")),
                b.var_decl("count", b.int(0)),
                b.let_("values", values),
            ],
            result,
        ),
    )
}

fn char_code_or_neg(b: &ExprBuilder, idx: Expr, text: Expr) -> Expr {
    b.match_(
        b.app2(rt(b, "String.CharCodeAt"), idx, text),
        [arm(b.some_pat("c"), b.loc("c")), arm(Pat::Wild, b.int(-1))],
    )
}

fn is_ascii_digit(b: &ExprBuilder, code: Expr) -> Expr {
    b.bin(
        BinOp::And,
        b.bin(BinOp::Ge, code.clone(), b.int(48)),
        b.bin(BinOp::Le, code, b.int(57)),
    )
}

fn sql_renumber_quote_branch(b: &ExprBuilder) -> Expr {
    let next_q = char_code_or_neg(b, b.add_i(b.var("i"), b.int(1)), b.loc("text"));
    let on_quote_char = b.if_(
        b.bin(BinOp::Eq, next_q, b.int(34)),
        b.assign_e("i", b.add_i(b.var("i"), b.int(2))),
        b.block(
            [b.assign("i", b.add_i(b.var("i"), b.int(1)))],
            b.break_("$sqq", b.unit()),
        ),
    );
    let q_step = b.if_(
        b.bin(BinOp::Lt, b.var("i"), b.loc("len")),
        b.block(
            [b.let_("c2", char_code_or_neg(b, b.var("i"), b.loc("text")))],
            b.if_(
                b.bin(BinOp::Eq, b.loc("c2"), b.int(34)),
                on_quote_char,
                b.assign_e("i", b.add_i(b.var("i"), b.int(1))),
            ),
        ),
        b.break_("$sqq", b.unit()),
    );
    b.block(
        [
            b.let_("start", b.var("i")),
            b.assign("i", b.add_i(b.var("i"), b.int(1))),
            Stmt::Expr(b.loop_ctrl("$sqq", q_step)),
            b.assign(
                "out",
                cat(
                    b,
                    b.var("out"),
                    b.app3(
                        rt(b, "String.Slice"),
                        b.loc("start"),
                        b.var("i"),
                        b.loc("text"),
                    ),
                ),
            ),
        ],
        b.unit(),
    )
}

fn sql_renumber_dollar_branch(b: &ExprBuilder) -> Expr {
    let mul10 = b.bin(BinOp::Arith(Arith::Mul, Num::Int), b.var("num"), b.int(10));
    let digit_add = b.add_i(mul10, b.sub_i(b.loc("d"), b.int(48)));
    let d_step = b.if_(
        b.bin(BinOp::Lt, b.var("i"), b.loc("len")),
        b.block(
            [b.let_("d", char_code_or_neg(b, b.var("i"), b.loc("text")))],
            b.if_(
                is_ascii_digit(b, b.loc("d")),
                b.block(
                    [
                        b.assign("num", digit_add),
                        b.assign("i", b.add_i(b.var("i"), b.int(1))),
                    ],
                    b.unit(),
                ),
                b.break_("$sqd", b.unit()),
            ),
        ),
        b.break_("$sqd", b.unit()),
    );
    let dollar_digits = b.block(
        [
            b.assign("i", b.add_i(b.var("i"), b.int(1))),
            b.var_decl("num", b.int(0)),
            Stmt::Expr(b.loop_ctrl("$sqd", d_step)),
            b.assign(
                "out",
                cat(b, b.var("out"), b.app(b.loc("slot"), b.var("num"))),
            ),
        ],
        b.unit(),
    );
    let literal_dollar = b.block(
        [
            b.assign("out", cat(b, b.var("out"), str_lit(b, "$"))),
            b.assign("i", b.add_i(b.var("i"), b.int(1))),
        ],
        b.unit(),
    );
    b.block(
        [b.let_(
            "d0",
            char_code_or_neg(b, b.add_i(b.var("i"), b.int(1)), b.loc("text")),
        )],
        b.if_(
            is_ascii_digit(b, b.loc("d0")),
            dollar_digits,
            literal_dollar,
        ),
    )
}

fn sql_renumber_helper(b: &ExprBuilder) -> Expr {
    let plain_branch = b.block(
        [
            b.assign(
                "out",
                cat(
                    b,
                    b.var("out"),
                    b.app3(
                        rt(b, "String.Slice"),
                        b.var("i"),
                        b.add_i(b.var("i"), b.int(1)),
                        b.loc("text"),
                    ),
                ),
            ),
            b.assign("i", b.add_i(b.var("i"), b.int(1))),
        ],
        b.unit(),
    );
    let main_step = b.block(
        [b.let_("ch", char_code_or_neg(b, b.var("i"), b.loc("text")))],
        b.if_(
            b.bin(BinOp::Eq, b.loc("ch"), b.int(34)),
            sql_renumber_quote_branch(b),
            b.if_(
                b.bin(BinOp::Eq, b.loc("ch"), b.int(36)),
                sql_renumber_dollar_branch(b),
                plain_branch,
            ),
        ),
    );
    let body = b.block(
        [
            b.let_("len", b.app(rt(b, "String.Length"), b.loc("text"))),
            b.var_decl("i", b.int(0)),
            b.var_decl("out", str_lit(b, "")),
            Stmt::Expr(b.while_(b.bin(BinOp::Lt, b.var("i"), b.loc("len")), main_step)),
        ],
        b.var("out"),
    );
    b.lam(&["text", "slot"], body)
}

fn sql_shift_param_helper(b: &ExprBuilder) -> Expr {
    let kv_pat = b.tup_pat(&["k", "v"]);
    let updated_pair = b.if_(
        b.bin(BinOp::Eq, b.loc("k"), str_lit(b, "table")),
        b.yield_(b.tuple([b.loc("k"), b.add_i(b.loc("t"), b.loc("shift"))])),
        b.yield_(b.tuple([b.loc("k"), b.loc("v")])),
    );
    let rebuilt = b.app(
        rt(b, "Schema.RecordFromEntries"),
        b.gen_strict(b.each(
            "pair",
            b.loc("ents"),
            b.match_(b.loc("pair"), [arm(kv_pat, updated_pair)]),
        )),
    );
    let on_table = b.match_(
        b.app(rt(b, "Schema.RecordEntries"), b.loc("q")),
        [arm(b.some_pat("ents"), rebuilt), arm(Pat::Wild, b.loc("q"))],
    );
    b.lam(
        &["shift", "q"],
        b.match_(
            b.app2(
                rt(b, "Schema.RecordLookup"),
                b.loc("q"),
                str_lit(b, "table"),
            ),
            [arm(b.some_pat("t"), on_table), arm(Pat::Wild, b.loc("q"))],
        ),
    )
}

fn is_template_param(b: &ExprBuilder, p: Expr) -> Expr {
    let lookup = b.app2(rt(b, "Schema.RecordLookup"), p, str_lit(b, "sql_kind"));
    b.match_(
        lookup,
        [
            arm(
                b.some_pat("sk"),
                b.bin(BinOp::Eq, b.loc("sk"), str_lit(b, "template")),
            ),
            arm(Pat::Wild, b.bool_(false)),
        ],
    )
}

fn sql_splice_sub_branch(b: &ExprBuilder) -> Expr {
    let shifted = b.gen_strict(b.each(
        "q",
        b.field(b.loc("sub"), "params"),
        b.yield_(b.app2(rt(b, "Helper.sql_shift_param"), b.loc("shift"), b.loc("q"))),
    ));
    let sub_slot = b.lam(
        &["n"],
        cat(
            b,
            str_lit(b, "$"),
            b.app(rt(b, "Show"), b.add_i(b.loc("base"), b.loc("n"))),
        ),
    );
    let sub_text = b.app2(
        rt(b, "Helper.sql_renumber"),
        b.field(b.loc("sub"), "text"),
        sub_slot,
    );
    b.block(
        [
            b.let_("sub", b.field(b.loc("p"), "value")),
            b.let_(
                "base",
                b.app(b.global("List.Core", "length"), b.var("params")),
            ),
            b.let_(
                "shift",
                b.app(b.global("List.Core", "length"), b.var("tables")),
            ),
            b.assign(
                "tables",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("tables"),
                    b.field(b.loc("sub"), "tables"),
                ),
            ),
            b.assign(
                "params",
                b.app2(b.global("List.Core", "append"), b.var("params"), shifted),
            ),
            b.assign(
                "texts",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("texts"),
                    b.list([sub_text]),
                ),
            ),
        ],
        b.unit(),
    )
}

fn sql_splice_plain_branch(b: &ExprBuilder) -> Expr {
    b.block(
        [
            b.assign(
                "params",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("params"),
                    b.list([b.loc("p")]),
                ),
            ),
            b.let_(
                "idx",
                b.app(b.global("List.Core", "length"), b.var("params")),
            ),
            b.assign(
                "texts",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("texts"),
                    b.list([cat(b, str_lit(b, "$"), b.app(rt(b, "Show"), b.loc("idx")))]),
                ),
            ),
        ],
        b.unit(),
    )
}

fn sql_template_splice(b: &ExprBuilder) -> Expr {
    let check_each = b.each(
        "p0",
        b.field(b.loc("tpl"), "params"),
        b.if_(
            is_template_param(b, b.loc("p0")),
            b.assign_e("has_sub", b.bool_(true)),
            b.unit(),
        ),
    );
    let outer_slot = b.lam(
        &["n"],
        b.match_(
            b.app2(
                b.global("List.Core", "get"),
                b.sub_i(b.loc("n"), b.int(1)),
                b.var("texts"),
            ),
            [
                arm(b.some_pat("s"), b.loc("s")),
                arm(
                    Pat::Wild,
                    cat(b, str_lit(b, "$"), b.app(rt(b, "Show"), b.loc("n"))),
                ),
            ],
        ),
    );
    let final_text = b.app2(
        rt(b, "Helper.sql_renumber"),
        b.field(b.loc("tpl"), "text"),
        outer_slot,
    );
    let spliced = b.block(
        [
            b.var_decl("tables", b.field(b.loc("tpl"), "tables")),
            b.var_decl("params", b.list([])),
            b.var_decl("texts", b.list([])),
            Stmt::Expr(b.each(
                "p",
                b.field(b.loc("tpl"), "params"),
                b.if_(
                    is_template_param(b, b.loc("p")),
                    sql_splice_sub_branch(b),
                    sql_splice_plain_branch(b),
                ),
            )),
            b.let_("new_text", final_text),
        ],
        record(
            b,
            vec![
                ("text", b.loc("new_text")),
                ("tables", b.var("tables")),
                ("params", b.var("params")),
                ("columns", b.field(b.loc("tpl"), "columns")),
            ],
        ),
    );
    let body = b.block(
        [
            b.var_decl("has_sub", b.bool_(false)),
            Stmt::Expr(check_each),
        ],
        b.if_(b.var("has_sub"), spliced, b.loc("tpl")),
    );
    b.lam(&["tpl"], body)
}
