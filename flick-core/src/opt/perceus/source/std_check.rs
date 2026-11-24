//! Audited `Check` pure IR adapters (`packages/std/src/Check.fl`).

mod ops;

use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::std_check_props::{
    check_property_names, check_props_between, check_props_one, check_size_between, check_size_one,
};
use super::std_schema_rt::{cat, con_app, rt, str_lit};
use super::{Code, Env, Expr, Local, Lower, LowerError, Pat, Span};

impl Lower<'_> {
    pub(super) fn std_check_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.mode != ForeignMode::Pure || value.binding.from() != Some("flick:Schema") {
            return None;
        }
        let bind = value.binding.export_name().unwrap_or("");
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$chk");

        // `Check.uuid` carries `optional 0`.
        if bind == "isUUID"
            && value.dicts == 0
            && stages == 1
            && value.catch.is_none()
            && value.adapter.words.is_empty()
            && value.adapter.optional == [0]
            && value.adapter.owned.is_empty()
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
            let expr = ops::ord_check_template(&b, bind, stages)?;
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
    atom_with_spec(b, run_fn, meta, b.none())
}

fn atom_with_spec(b: &ExprBuilder, run_fn: Expr, meta: Expr, spec_opt: Expr) -> Expr {
    con_app(b, "Check", "$Atom", "$Atom", vec![run_fn, meta, spec_opt])
}

fn builtin_check(b: &ExprBuilder, kind: &str, param: Expr) -> Expr {
    let spec_pat = Pat::Con(
        ExprBuilder::con("Check", "$Spec", "$Spec", 3),
        vec![Pat::Wild, Pat::Wild, Pat::Bind(b.name("m"))],
    );
    let item = atom_with_spec(
        b,
        b.app(rt(b, "Schema.RunCheck"), b.loc("sp")),
        b.loc("m"),
        b.some(b.loc("sp")),
    );
    b.block(
        [b.let_(
            "sp",
            b.app2(rt(b, "Schema.MakeCheck"), str_lit(b, kind), param),
        )],
        b.match_(b.loc("sp"), [arm(spec_pat, b.list([item]))]),
    )
}

pub(super) fn expected_fail(b: &ExprBuilder, desc: Expr, val: Expr) -> Expr {
    let tail = cat(
        b,
        str_lit(b, ", got "),
        b.app(rt(b, "Schema.DescribeInput"), val),
    );
    b.some(cat(b, str_lit(b, "expected "), cat(b, desc, tail)))
}

fn string_and_num_check_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("isMinLength", 1) => Some(b.lam(&["n"], builtin_check(b, "min_length", b.loc("n")))),
        ("isMaxLength", 1) => Some(b.lam(&["n"], builtin_check(b, "max_length", b.loc("n")))),
        ("isBetweenLength", 2) => Some(b.lam(
            &["lo", "hi"],
            builtin_check(b, "length_between", b.tuple([b.loc("lo"), b.loc("hi")])),
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
                b.tuple([b.loc("lo"), b.loc("hi")]),
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
        ("flickIsInt32", 0) => Some(ops::int_range_check(b, -2_147_483_648.0, 2_147_483_647.0)),
        ("flickIsUint32", 0) => Some(ops::int_range_check(b, 0.0, 4_294_967_295.0)),
        _ => None,
    }
}

fn check_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    if let Some(expr) = string_and_num_check_template(b, bind, stages) {
        return Some(expr);
    }
    match (bind, stages) {
        ("flickCheck", 2) => Some(ops::check_apply(b)),
        ("makeFilterGroup", 1) => Some(b.lam(
            &["chks"],
            b.gen_strict(b.each(
                "grp",
                b.loc("chks"),
                b.each("a", b.loc("grp"), b.yield_(b.loc("a"))),
            )),
        )),
        ("flickMakeFilter", 2) => {
            let run_fn = b.lam(
                &["v"],
                b.if_(
                    b.app(b.loc("pred"), b.loc("v")),
                    b.none(),
                    b.some(b.loc("msg")),
                ),
            );
            let meta = con_app(b, "Check", "$Meta", "$None", vec![b.loc("msg")]);
            Some(b.lam(&["pred", "msg"], b.list([atom(b, run_fn, meta)])))
        }
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
        ("flickMakeIsMultipleOf", 3) => Some(ops::check_multiple_of_by(b)),
        ("flickIsMinSize", 1) => Some(check_size_one(b, true)),
        ("flickIsMaxSize", 1) => Some(check_size_one(b, false)),
        ("flickIsBetweenSize", 2) => Some(check_size_between(b)),
        ("flickIsMinProperties", 2) => Some(check_props_one(b, true)),
        ("flickIsMaxProperties", 2) => Some(check_props_one(b, false)),
        ("flickIsBetweenProperties", 3) => Some(check_props_between(b)),
        ("flickIsPropertyNames", 2) => Some(check_property_names(b)),
        _ => None,
    }
}
