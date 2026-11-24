//! Audited `Effect.Core`, `Effect.Concurrency`, and `Effect.Errors` adapters (Stage 4A).

mod errors;

use flick_ir::memory::AsyncIntrinsic;
use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, ExprKind, Intrinsic, Local, Lower, LowerError, Pat, Span};

impl Lower<'_> {
    pub(super) fn std_effect_ops_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        let from = value.binding.from()?;
        if from != "flick:Effect" {
            return None;
        }
        let bind = value.binding.export_name().unwrap_or("");
        let stages = value.stages.len();

        if value.mode == ForeignMode::Pure && bind == "replicate" && stages == 2 {
            let b = ExprBuilder::new(span, "$erep");
            let expr = replicate_template(&b);
            return Some(self.expr(&expr, &Env::new(), block));
        }
        if value.mode != ForeignMode::Effect {
            return None;
        }
        if let Some(op) = direct_effect_intrinsic(bind, stages) {
            return Some(self.intrinsic(Intrinsic::Async(op), false, vec![], block, span));
        }
        let b = ExprBuilder::new(span, "$eff");
        let expr = effect_ops_template(&b, bind, stages)?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

fn direct_effect_intrinsic(bind: &str, stages: usize) -> Option<AsyncIntrinsic> {
    let op = match bind {
        "sleep" => AsyncIntrinsic::EffectSleep,
        "flickNever" => AsyncIntrinsic::EffectNever,
        "flickYieldNow" => AsyncIntrinsic::EffectYieldNow,
        "yieldNowWith" => AsyncIntrinsic::EffectYieldNowWith,
        "flickInterrupt" => AsyncIntrinsic::EffectInterrupt,
        "interruptible" => AsyncIntrinsic::EffectInterruptible,
        "uninterruptibleMask" => AsyncIntrinsic::EffectUninterruptibleMask,
        "interruptibleMask" => AsyncIntrinsic::EffectInterruptibleMask,
        "raceAll" => AsyncIntrinsic::RaceAll,
        "raceAllFirst" => AsyncIntrinsic::RaceAllFirst,
        "flickFiberId" => AsyncIntrinsic::EffectFiberId,
        "ensuring" => AsyncIntrinsic::EffectEnsuring,
        "flickOnExitPrimitive" => AsyncIntrinsic::EffectOnExitPrimitive,
        "flickCatchCause" => AsyncIntrinsic::EffectCatchCause,
        "orDie" => AsyncIntrinsic::EffectOrDie,
        "timed" => AsyncIntrinsic::EffectTimed,
        "flickTimeoutOrElse" => AsyncIntrinsic::TimeoutOrElse,
        "cached" => AsyncIntrinsic::EffectCached,
        "cachedWithTTL" => AsyncIntrinsic::EffectCachedWithTtl,
        "cachedInvalidateWithTTL" => AsyncIntrinsic::EffectCachedInvalidateWithTtl,
        _ => return None,
    };
    (op.arity() == stages).then_some(op)
}

fn replicate_template(b: &ExprBuilder) -> Expr {
    let body = b.while_(
        b.bin(BinOp::Lt, b.var("i"), b.loc("n")),
        b.block(
            vec![b.assign("i", b.add_i(b.var("i"), b.int(1)))],
            b.yield_(b.loc("t")),
        ),
    );
    b.lam(
        &["n", "t"],
        b.block(vec![b.var_decl("i", b.int(0))], b.gen_strict(body)),
    )
}

fn effect_ops_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("delay", 2) => Some(b.lam(
            &["d", "t"],
            b.block(
                vec![b.let_("_s", b.app(b.global("Effect.Core", "sleep"), b.loc("d")))],
                b.at(ExprKind::Await(Box::new(b.loc("t")))),
            ),
        )),
        ("forever", 1) => Some(b.lam(
            &["t"],
            b.while_(b.bool_(true), b.at(ExprKind::Await(Box::new(b.loc("t"))))),
        )),
        ("eventually", 1) => Some(eventually_template(b)),
        ("flickReplicateEffect", 3) => Some(b.lam(
            &["conc", "n", "t"],
            b.app2(
                b.global("Effect.Collections", "all"),
                b.loc("conc"),
                b.app2(b.global("Effect.Core", "replicate"), b.loc("n"), b.loc("t")),
            ),
        )),
        _ => errors::error_ops_template(b, bind, stages),
    }
}

fn eventually_template(b: &ExprBuilder) -> Expr {
    let check_fail = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("ex")),
        vec![
            arm(
                b.some_pat("c"),
                b.if_(
                    b.app(b.global("Cause", "has_fails"), b.loc("c")),
                    b.unit(),
                    b.app(b.global("Failure", "rethrow"), b.loc("c")),
                ),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let step = b.block(
        vec![b.let_("ex", b.app(b.global("Effect.Core", "exit"), b.loc("t")))],
        b.match_(
            b.app(b.global("Exit", "value"), b.loc("ex")),
            vec![
                arm(b.some_pat("v"), b.break_("eventually", b.loc("v"))),
                arm(Pat::Wild, check_fail),
            ],
        ),
    );
    b.lam(
        &["t"],
        b.at(ExprKind::ControlLoop {
            id: "eventually".into(),
            condition: None,
            body: Box::new(step),
            iterable: false,
        }),
    )
}
