//! Audited `Arbitrary` foreign adapters (`packages/std/src/Arbitrary.fl`, Stage 4D).

mod combinators;
mod format;
mod generators;

use flick_ir::{Arith, BinOp, Foreign, ForeignMode, Num};

use super::expr_builder::ExprBuilder;
use super::lazy_scope::has_standard_shape;
use super::std_schema_rt::rt;
use super::{Code, Env, Expr, Local, Lower, LowerError, Pat, Span};
use combinators::{
    filter_gen_template, filter_map_gen_template, flat_map_gen_template, map_gen_template,
};
use format::format_failure_template;
use generators::{all_gen_template, array_gen_template, schema_with_template};

impl Lower<'_> {
    pub(super) fn std_arbitrary_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        if value.binding.from()? != "flick:Arbitrary" {
            return None;
        }
        let bind = value.binding.export_name().unwrap_or("");
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$arb");
        let expr = arb_template(&b, bind, value.mode, stages)?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

fn add_f(b: &ExprBuilder, x: Expr, y: Expr) -> Expr {
    b.bin(BinOp::Arith(Arith::Add, Num::Float), x, y)
}

fn mul_f(b: &ExprBuilder, x: Expr, y: Expr) -> Expr {
    b.bin(BinOp::Arith(Arith::Mul, Num::Float), x, y)
}

fn div_f(b: &ExprBuilder, x: Expr, y: Expr) -> Expr {
    b.bin(BinOp::Arith(Arith::Div, Num::Float), x, y)
}

fn st_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["sz", "shk", "bias", "rr", "br"])
}

fn constant_gen_template(b: &ExprBuilder) -> Expr {
    let sample = b.some(b.tuple([b.loc("v"), b.none()]));
    let gen_fn = b.lam(&["_st"], sample);
    b.lam(&["v"], b.tuple([b.float(0.0), gen_fn]))
}

fn zip_gen_template(b: &ExprBuilder, names: &[&str]) -> Expr {
    let items: Vec<Expr> = names.iter().map(|n| b.loc(n)).collect();
    let all = b.app(b.global("Arbitrary", "all"), b.list(items));
    let mapped = b.app2(
        b.global("Arbitrary", "map"),
        rt(b, "Schema.TupleFromList"),
        all,
    );
    b.lam(names, mapped)
}

fn arb_template(b: &ExprBuilder, bind: &str, mode: ForeignMode, stages: usize) -> Option<Expr> {
    if mode == ForeignMode::Effect {
        return match (bind, stages) {
            ("flickSampleEffect", 2) => Some(rt(b, "Helper.arb_sample")),
            ("flickCheckEffect", 3) => Some(rt(b, "Helper.arb_check")),
            _ => None,
        };
    }
    if mode != ForeignMode::Pure {
        return None;
    }
    match (bind, stages) {
        ("schema", 1) => Some(rt(b, "Helper.arb_compile_schema")),
        ("flickSchema", 2) => Some(schema_with_template(b)),
        ("Constant", 1) => Some(constant_gen_template(b)),
        ("flickArray", 2) => Some(array_gen_template(b)),
        ("all", 1) => Some(all_gen_template(b)),
        ("flickZip2", 2) => Some(zip_gen_template(b, &["a", "c"])),
        ("flickZip3", 3) => Some(zip_gen_template(b, &["a", "c", "d"])),
        ("flickZip4", 4) => Some(zip_gen_template(b, &["a", "c", "d", "e"])),
        ("map", 2) => Some(map_gen_template(b)),
        ("filter", 2) => Some(filter_gen_template(b)),
        ("filterMap", 2) => Some(filter_map_gen_template(b)),
        ("flatMap", 2) => Some(flat_map_gen_template(b)),
        ("flickFormatCheckFailure", 1) => Some(format_failure_template(b)),
        _ => None,
    }
}
