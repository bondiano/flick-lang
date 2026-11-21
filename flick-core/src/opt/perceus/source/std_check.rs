//! Audited `Check` pure IR adapters (`packages/std/src/Check.fl`).

use flick_ir::{BinOp, Dict, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::std_schema_rt::{cat, con_app, make_codec, meta_con, rt, str_lit};
use super::{Code, Env, Expr, ExprKind, Local, Lower, LowerError, Pat, Span};

impl Lower<'_> {
    pub(super) fn std_check_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.mode != ForeignMode::Pure || value.binding.from.as_deref() != Some("flick:Schema")
        {
            return None;
        }
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$chk");

        // `Check.uuid` carries `optional 0`.
        if bind == "isUUID"
            && value.dicts == 0
            && stages == 1
            && value.catch.is_none()
            && !value.adapter.receiver
            && !value.adapter.cancel
            && value.adapter.optional == [0]
            && !value.adapter.variadic
            && !value.adapter.result.nullable
        {
            let expr = b.lam(&["ver"], builtin_check(&b, "uuid", b.loc("ver")));
            return Some(self.expr(&expr, &Env::new(), block));
        }

        if !has_standard_shape(value) {
            return None;
        }

        // `Check.*_by` carries 1 `Ord` dictionary.
        if value.dicts == 1 {
            let expr = ord_check_template(&b, bind, stages)?;
            return Some(self.expr(&expr, &Env::new(), block));
        }
        if value.dicts != 0 {
            return None;
        }
        let expr = check_template(&b, bind, stages)?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

pub(super) fn atom(b: &ExprBuilder, run_fn: Expr, meta: Expr) -> Expr {
    con_app(b, "Check", "$Atom", "$Atom", vec![run_fn, meta])
}

fn builtin_check(b: &ExprBuilder, kind: &str, param: Expr) -> Expr {
    b.block(
        vec![b.let_(
            "sp",
            b.app2(rt(b, "Schema.MakeCheck"), str_lit(b, kind), param),
        )],
        b.match_(
            b.loc("sp"),
            vec![arm(
                Pat::Con(
                    ExprBuilder::con("Check", "$Spec", "$Spec", 3),
                    vec![Pat::Wild, Pat::Wild, Pat::Bind(b.name("m"))],
                ),
                b.list(vec![atom(
                    b,
                    b.app(rt(b, "Schema.RunCheck"), b.loc("sp")),
                    b.loc("m"),
                )]),
            )],
        ),
    )
}

pub(super) fn expected_fail(b: &ExprBuilder, desc: Expr, val: Expr) -> Expr {
    b.some(cat(
        b,
        str_lit(b, "expected "),
        cat(
            b,
            desc,
            cat(
                b,
                str_lit(b, ", got "),
                b.app(rt(b, "Schema.DescribeInput"), val),
            ),
        ),
    ))
}

#[expect(
    clippy::too_many_lines,
    reason = "central Check combinator template dispatch"
)]
fn check_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickCheck", 2) => Some(check_apply(b)),
        ("makeFilterGroup", 1) => Some(b.lam(
            &["chks"],
            b.gen_strict(b.each(
                "grp",
                b.loc("chks"),
                b.each("a", b.loc("grp"), b.yield_(b.loc("a"))),
            )),
        )),
        ("flickMakeFilter", 2) => Some(b.lam(
            &["pred", "msg"],
            b.list(vec![atom(
                b,
                b.lam(
                    &["v"],
                    b.if_(
                        b.app(b.loc("pred"), b.loc("v")),
                        b.none(),
                        b.some(b.loc("msg")),
                    ),
                ),
                con_app(b, "Check", "$Meta", "$None", vec![b.loc("msg")]),
            )]),
        )),
        ("isMinLength", 1) => Some(b.lam(&["n"], builtin_check(b, "min_length", b.loc("n")))),
        ("isMaxLength", 1) => Some(b.lam(&["n"], builtin_check(b, "max_length", b.loc("n")))),
        ("isBetweenLength", 2) => Some(b.lam(
            &["lo", "hi"],
            builtin_check(b, "length_between", b.tuple(vec![b.loc("lo"), b.loc("hi")])),
        )),
        ("flickIsNonEmpty", 0) => Some(builtin_check(b, "min_length", b.int(1))),
        ("isMinCodePoints", 1) => {
            Some(b.lam(&["n"], builtin_check(b, "min_code_points", b.loc("n"))))
        }
        ("isMaxCodePoints", 1) => {
            Some(b.lam(&["n"], builtin_check(b, "max_code_points", b.loc("n"))))
        }
        ("isBetweenCodePoints", 2) => Some(b.lam(
            &["lo", "hi"],
            builtin_check(
                b,
                "code_points_between",
                b.tuple(vec![b.loc("lo"), b.loc("hi")]),
            ),
        )),
        ("flickIsPattern", 1) => Some(b.lam(&["src"], builtin_check(b, "pattern", b.loc("src")))),
        ("isStartingWith", 1) => Some(b.lam(&["s"], builtin_check(b, "starts_with", b.loc("s")))),
        ("isEndingWith", 1) => Some(b.lam(&["s"], builtin_check(b, "ends_with", b.loc("s")))),
        ("isIncluding", 1) => Some(b.lam(&["s"], builtin_check(b, "includes", b.loc("s")))),
        ("flickIsTrimmed", 0) => Some(builtin_check(b, "trimmed", b.unit())),
        ("flickIsLowercased", 0) => Some(builtin_check(b, "lowercased", b.unit())),
        ("flickIsUppercased", 0) => Some(builtin_check(b, "uppercased", b.unit())),
        ("flickIsCapitalized", 0) => Some(builtin_check(b, "capitalized", b.unit())),
        ("flickIsUncapitalized", 0) => Some(builtin_check(b, "uncapitalized", b.unit())),
        ("flickIsULID", 0) => Some(builtin_check(b, "ulid", b.unit())),
        ("flickIsGUID", 0) => Some(builtin_check(b, "guid", b.unit())),
        ("flickIsBase64", 0) => Some(builtin_check(b, "base64", b.unit())),
        ("flickIsBase64Url", 0) => Some(builtin_check(b, "base64url", b.unit())),
        ("flickIsStringFinite", 0) => Some(builtin_check(b, "string_finite", b.unit())),
        ("flickIsStringBigInt", 0) => Some(builtin_check(b, "string_bigint", b.unit())),
        ("isGreaterThan", 1) => Some(b.lam(&["x"], builtin_check(b, "gt", b.loc("x")))),
        ("isGreaterThanOrEqualTo", 1) => Some(b.lam(&["x"], builtin_check(b, "gte", b.loc("x")))),
        ("isLessThan", 1) => Some(b.lam(&["x"], builtin_check(b, "lt", b.loc("x")))),
        ("isLessThanOrEqualTo", 1) => Some(b.lam(&["x"], builtin_check(b, "lte", b.loc("x")))),
        ("isBetween", 1) => Some(b.lam(&["opt"], builtin_check(b, "between", b.loc("opt")))),
        ("isMultipleOf", 1) => Some(b.lam(&["d"], builtin_check(b, "multiple_of", b.loc("d")))),
        ("flickIsInt", 0) => Some(builtin_check(b, "int", b.unit())),
        ("flickIsFinite", 0) => Some(builtin_check(b, "finite", b.unit())),
        ("flickIsInt32", 0) => Some(int_range_check(b, -2_147_483_648.0, 2_147_483_647.0)),
        ("flickIsUint32", 0) => Some(int_range_check(b, 0.0, 4_294_967_295.0)),
        ("isGreaterThanBigInt", 1) => {
            Some(b.lam(&["x"], builtin_check(b, "gt_bigint", b.loc("x"))))
        }
        ("isGreaterThanOrEqualToBigInt", 1) => {
            Some(b.lam(&["x"], builtin_check(b, "gte_bigint", b.loc("x"))))
        }
        ("isLessThanBigInt", 1) => Some(b.lam(&["x"], builtin_check(b, "lt_bigint", b.loc("x")))),
        ("isLessThanOrEqualToBigInt", 1) => {
            Some(b.lam(&["x"], builtin_check(b, "lte_bigint", b.loc("x"))))
        }
        ("isBetweenBigInt", 1) => {
            Some(b.lam(&["opt"], builtin_check(b, "between_bigint", b.loc("opt"))))
        }
        ("isGreaterThanBigDecimal", 1) => {
            Some(b.lam(&["x"], builtin_check(b, "gt_decimal", b.loc("x"))))
        }
        ("isGreaterThanOrEqualToBigDecimal", 1) => {
            Some(b.lam(&["x"], builtin_check(b, "gte_decimal", b.loc("x"))))
        }
        ("isLessThanBigDecimal", 1) => {
            Some(b.lam(&["x"], builtin_check(b, "lt_decimal", b.loc("x"))))
        }
        ("isLessThanOrEqualToBigDecimal", 1) => {
            Some(b.lam(&["x"], builtin_check(b, "lte_decimal", b.loc("x"))))
        }
        ("isBetweenBigDecimal", 1) => {
            Some(b.lam(&["opt"], builtin_check(b, "between_decimal", b.loc("opt"))))
        }
        ("flickIsUnique", 0) => Some(builtin_check(b, "unique", b.unit())),
        ("flickIsUniqueKey", 0) => Some(builtin_check(b, "unique_keys", b.unit())),
        ("flickMakeIsMultipleOf", 3) => Some(check_multiple_of_by(b)),
        ("flickIsMinSize", 1) => Some(super::std_check_props::check_size_one(b, true)),
        ("flickIsMaxSize", 1) => Some(super::std_check_props::check_size_one(b, false)),
        ("flickIsBetweenSize", 2) => Some(super::std_check_props::check_size_between(b)),
        ("flickIsMinProperties", 2) => Some(super::std_check_props::check_props_one(b, true)),
        ("flickIsMaxProperties", 2) => Some(super::std_check_props::check_props_one(b, false)),
        ("flickIsBetweenProperties", 3) => Some(super::std_check_props::check_props_between(b)),
        ("flickIsPropertyNames", 2) => Some(super::std_check_props::check_property_names(b)),
        _ => None,
    }
}

fn check_apply(b: &ExprBuilder) -> Expr {
    b.lam(
        &["chk", "c"],
        b.block(
            vec![
                b.let_(
                    "new_ms",
                    b.gen_strict(b.each(
                        "a",
                        b.loc("chk"),
                        b.match_(
                            b.loc("a"),
                            vec![arm(
                                Pat::Con(
                                    ExprBuilder::con("Check", "$Atom", "$Atom", 2),
                                    vec![Pat::Wild, Pat::Bind(b.name("m"))],
                                ),
                                b.yield_(b.loc("m")),
                            )],
                        ),
                    )),
                ),
                b.let_("om", b.app(rt(b, "Helper.codec_meta"), b.loc("c"))),
                b.let_(
                    "nm",
                    b.match_(
                        b.loc("om"),
                        vec![
                            arm(
                                Pat::Con(
                                    ExprBuilder::con("Codec", "$Meta", "$Checked", 4),
                                    vec![
                                        Pat::Bind(b.name("bm")),
                                        Pat::Bind(b.name("cs")),
                                        Pat::Wild,
                                        Pat::Wild,
                                    ],
                                ),
                                meta_con(
                                    b,
                                    "$Checked",
                                    vec![
                                        b.loc("bm"),
                                        b.app2(
                                            b.global("List.Core", "append"),
                                            b.loc("cs"),
                                            b.loc("new_ms"),
                                        ),
                                        b.none(),
                                        b.none(),
                                    ],
                                ),
                            ),
                            arm(
                                Pat::Wild,
                                meta_con(
                                    b,
                                    "$Checked",
                                    vec![b.loc("om"), b.loc("new_ms"), b.none(), b.none()],
                                ),
                            ),
                        ],
                    ),
                ),
            ],
            make_codec(
                b,
                b.lam(
                    &["in"],
                    b.match_(
                        b.app(b.app(rt(b, "Helper.codec_dec"), b.loc("c")), b.loc("in")),
                        vec![
                            arm(
                                b.ok_pat("v"),
                                b.app2(rt(b, "Helper.run_checks"), b.loc("chk"), b.loc("v")),
                            ),
                            arm(b.err_pat("e"), b.err(b.loc("e"))),
                        ],
                    ),
                ),
                b.lam(
                    &["v"],
                    b.match_(
                        b.app2(rt(b, "Helper.run_checks"), b.loc("chk"), b.loc("v")),
                        vec![
                            arm(
                                b.ok_pat("_"),
                                b.app(b.app(rt(b, "Helper.codec_enc"), b.loc("c")), b.loc("v")),
                            ),
                            arm(b.err_pat("e"), b.err(b.loc("e"))),
                        ],
                    ),
                ),
                b.app(rt(b, "Helper.codec_acc"), b.loc("c")),
                b.loc("nm"),
                b.app(rt(b, "Helper.codec_extra"), b.loc("c")),
            ),
        ),
    )
}

fn int_range_check(b: &ExprBuilder, lo: f64, hi: f64) -> Expr {
    let range = b.at(ExprKind::Record(vec![
        ("exclusiveMaximum".into(), b.bool_(false)),
        ("exclusiveMinimum".into(), b.bool_(false)),
        ("maximum".into(), b.float(hi)),
        ("minimum".into(), b.float(lo)),
    ]));
    b.app2(
        b.global("List.Core", "append"),
        builtin_check(b, "int", b.unit()),
        builtin_check(b, "between", range),
    )
}

#[expect(clippy::too_many_lines, reason = "central Ord check template dispatch")]
fn ord_check_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    let cmp_fn = || {
        b.at(ExprKind::Method {
            dict: Dict::Field(b.name("ord")),
            name: "compare".into(),
        })
    };
    let less_than = |e: Expr| {
        b.match_(
            e,
            vec![
                arm(
                    Pat::Con(ExprBuilder::con("Prelude", "Ordering", "LT", 0), vec![]),
                    b.bool_(true),
                ),
                arm(Pat::Wild, b.bool_(false)),
            ],
        )
    };
    let greater_than = |e: Expr| {
        b.match_(
            e,
            vec![
                arm(
                    Pat::Con(ExprBuilder::con("Prelude", "Ordering", "GT", 0), vec![]),
                    b.bool_(true),
                ),
                arm(Pat::Wild, b.bool_(false)),
            ],
        )
    };
    let at_most = |e: Expr| b.not(greater_than(e));
    let at_least = |e: Expr| b.not(less_than(e));

    match (bind, stages) {
        (
            "flickMakeIsGreaterThan"
            | "flickMakeIsGreaterThanOrEqualTo"
            | "flickMakeIsLessThan"
            | "flickMakeIsLessThanOrEqualTo",
            1,
        ) => {
            let word = match bind {
                "flickMakeIsGreaterThan" => "a value greater than ",
                "flickMakeIsGreaterThanOrEqualTo" => "a value greater than or equal to ",
                "flickMakeIsLessThan" => "a value less than ",
                _ => "a value less than or equal to ",
            };
            let cmp_call = b.app2(cmp_fn(), b.loc("v"), b.loc("bound"));
            let pass = match bind {
                "flickMakeIsGreaterThan" => greater_than(cmp_call),
                "flickMakeIsGreaterThanOrEqualTo" => at_least(cmp_call),
                "flickMakeIsLessThan" => less_than(cmp_call),
                _ => at_most(cmp_call),
            };
            Some(b.lam(
                &["ord", "bound"],
                b.block(
                    vec![b.let_(
                        "desc",
                        cat(
                            b,
                            str_lit(b, word),
                            b.app(rt(b, "Schema.FormatValue"), b.loc("bound")),
                        ),
                    )],
                    b.list(vec![atom(
                        b,
                        b.lam(
                            &["v"],
                            b.if_(pass, b.none(), expected_fail(b, b.loc("desc"), b.loc("v"))),
                        ),
                        con_app(b, "Check", "$Meta", "$None", vec![b.loc("desc")]),
                    )]),
                ),
            ))
        }
        ("flickMakeIsBetween", 2) => Some(b.lam(
            &["ord", "lo", "hi"],
            b.block(
                vec![b.let_(
                    "desc",
                    cat(
                        b,
                        str_lit(b, "a value between "),
                        cat(
                            b,
                            b.app(rt(b, "Schema.FormatValue"), b.loc("lo")),
                            cat(
                                b,
                                str_lit(b, " and "),
                                b.app(rt(b, "Schema.FormatValue"), b.loc("hi")),
                            ),
                        ),
                    ),
                )],
                b.list(vec![atom(
                    b,
                    b.lam(
                        &["v"],
                        b.if_(
                            b.bin(
                                BinOp::And,
                                at_least(b.app2(cmp_fn(), b.loc("v"), b.loc("lo"))),
                                at_most(b.app2(cmp_fn(), b.loc("v"), b.loc("hi"))),
                            ),
                            b.none(),
                            expected_fail(b, b.loc("desc"), b.loc("v")),
                        ),
                    ),
                    con_app(b, "Check", "$Meta", "$None", vec![b.loc("desc")]),
                )]),
            ),
        )),
        _ => None,
    }
}

fn check_multiple_of_by(b: &ExprBuilder) -> Expr {
    b.lam(
        &["rem", "zero", "div"],
        b.block(
            vec![b.let_(
                "desc",
                cat(
                    b,
                    str_lit(b, "a value that is a multiple of "),
                    b.app(rt(b, "Schema.FormatValue"), b.loc("div")),
                ),
            )],
            b.list(vec![atom(
                b,
                b.lam(
                    &["v"],
                    b.if_(
                        b.app2(
                            rt(b, "Schema.SameValue"),
                            b.app2(b.loc("rem"), b.loc("v"), b.loc("div")),
                            b.loc("zero"),
                        ),
                        b.none(),
                        expected_fail(b, b.loc("desc"), b.loc("v")),
                    ),
                ),
                con_app(b, "Check", "$Meta", "$None", vec![b.loc("desc")]),
            )]),
        ),
    )
}
