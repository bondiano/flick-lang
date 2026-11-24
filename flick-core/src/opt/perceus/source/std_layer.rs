//! Audited dynamic `Layer`, `MemoMap`, and `Context` / `Context.Reference` adapters (Stage 4A).

mod templates;

use flick_ir::memory::LayerIntrinsic;
use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, Intrinsic, Local, Lower, LowerError, Operation, Pat, Span};

impl Lower<'_> {
    pub(super) fn std_layer_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        let (from, bind) = value.binding.module_export()?;
        let stages = value.stages.len();

        if from == "flick:Context" {
            return self.std_context_foreign(bind, value.mode, stages, span, block);
        }
        if from != "flick:Layer" {
            return None;
        }
        if value.mode == ForeignMode::Pure && bind == "empty" && stages == 0 {
            return Some(block.emit(
                Operation::Intrinsic {
                    kind: Intrinsic::Layer(LayerIntrinsic::LayerEmpty),
                    args: vec![],
                },
                span,
            ));
        }
        if let Some(op) = direct_layer_intrinsic(bind, value.mode, stages) {
            return Some(self.intrinsic(Intrinsic::Layer(op), false, vec![], block, span));
        }
        self.layer_template_foreign(bind, value.mode, stages, span, block)
    }

    fn std_context_foreign(
        &mut self,
        bind: &str,
        mode: ForeignMode,
        stages: usize,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if bind == "getOrElse" && mode == ForeignMode::Pure && stages == 3 {
            let b = ExprBuilder::new(span, "$cge");
            let expr = b.lam(
                &["tag", "fb", "ctx"],
                b.match_(
                    b.app2(
                        b.global("Context", "get_option"),
                        b.loc("tag"),
                        b.loc("ctx"),
                    ),
                    [
                        arm(b.some_pat("v"), b.loc("v")),
                        arm(Pat::Wild, b.app(b.loc("fb"), b.unit())),
                    ],
                ),
            );
            return Some(self.expr(&expr, &Env::new(), block));
        }
        let op = match (bind, mode) {
            ("empty", ForeignMode::Pure) => LayerIntrinsic::ContextEmpty,
            ("make", ForeignMode::Pure) => LayerIntrinsic::ContextMake,
            ("add", ForeignMode::Pure) => LayerIntrinsic::ContextAdd,
            ("addOrOmit", ForeignMode::Pure) => LayerIntrinsic::ContextAddOrOmit,
            ("get", ForeignMode::Pure) => LayerIntrinsic::ContextGet,
            ("getOption", ForeignMode::Pure) => LayerIntrinsic::ContextGetOption,
            ("merge", ForeignMode::Pure) => LayerIntrinsic::ContextMerge,
            ("flickMergeAll", ForeignMode::Pure) => LayerIntrinsic::ContextMergeAll,
            ("pick", ForeignMode::Pure) => LayerIntrinsic::ContextPick,
            ("omit", ForeignMode::Pure) => LayerIntrinsic::ContextOmit,
            ("flickReference", ForeignMode::Pure) => LayerIntrinsic::ContextReference,
            ("flickRead", ForeignMode::Effect) => LayerIntrinsic::ContextRead,
            ("flickWithReference", ForeignMode::Effect) => LayerIntrinsic::ContextWithReference,
            _ => return None,
        };
        (op.arity() == stages)
            .then(|| self.intrinsic(Intrinsic::Layer(op), false, vec![], block, span))
    }

    fn layer_template_foreign(
        &mut self,
        bind: &str,
        mode: ForeignMode,
        stages: usize,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        let b = ExprBuilder::new(span, "$lay");
        if mode == ForeignMode::Effect && bind == "launch" && stages == 1 {
            let body = b.lam(
                &["_owner"],
                b.block(
                    [b.let_("_ctx", b.app(b.global("Layer", "build"), b.loc("self")))],
                    b.app(b.global("Effect.Core", "never"), b.unit()),
                ),
            );
            let expr = b.lam(&["self"], b.app(b.global("Scope", "run"), body));
            return Some(self.expr(&expr, &Env::new(), block));
        }
        if mode != ForeignMode::Pure {
            return None;
        }
        match (bind, stages) {
            ("flickFromBuild", 1) => {
                let expr = b.lam(
                    &["mk", "f"],
                    b.app2(b.loc("mk"), b.bool_(false), b.loc("f")),
                );
                Some(self.apply_layer_intrinsic(&expr, LayerIntrinsic::LayerFromBuild, block, span))
            }
            ("flickFromBuildMemo", 1) => {
                let expr = b.lam(&["mk", "f"], b.app2(b.loc("mk"), b.bool_(true), b.loc("f")));
                Some(self.apply_layer_intrinsic(&expr, LayerIntrinsic::LayerFromBuild, block, span))
            }
            ("provide", 2) => {
                let expr = templates::layer_provide_template(&b, false);
                Some(self.apply_layer_intrinsic(
                    &expr,
                    LayerIntrinsic::LayerProvideContext,
                    block,
                    span,
                ))
            }
            ("provideMerge", 2) => {
                let expr = templates::layer_provide_template(&b, true);
                Some(self.apply_layer_intrinsic(
                    &expr,
                    LayerIntrinsic::LayerProvideContext,
                    block,
                    span,
                ))
            }
            ("updateService", 3) => {
                let expr = templates::layer_update_service_template(&b);
                Some(self.apply_update_service(&expr, block, span))
            }
            _ => {
                let expr = templates::pure_layer_template(&b, bind, stages)?;
                Some(self.expr(&expr, &Env::new(), block))
            }
        }
    }

    fn apply_layer_intrinsic(
        &mut self,
        expr: &Expr,
        op: LayerIntrinsic,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let function = self.expr(expr, &Env::new(), block)?;
        let argument = self.intrinsic(Intrinsic::Layer(op), false, vec![], block, span)?;
        block.emit(Operation::Apply { function, argument }, span)
    }

    fn apply_update_service(
        &mut self,
        expr: &Expr,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let f1 = self.expr(expr, &Env::new(), block)?;
        let prov = self.intrinsic(
            Intrinsic::Layer(LayerIntrinsic::LayerProvideContext),
            false,
            vec![],
            block,
            span,
        )?;
        let f2 = block.emit(
            Operation::Apply {
                function: f1,
                argument: prov,
            },
            span,
        )?;
        let lookup = self.intrinsic(
            Intrinsic::Layer(LayerIntrinsic::LayerLookupService),
            false,
            vec![],
            block,
            span,
        )?;
        block.emit(
            Operation::Apply {
                function: f2,
                argument: lookup,
            },
            span,
        )
    }
}

fn direct_layer_intrinsic(bind: &str, mode: ForeignMode, stages: usize) -> Option<LayerIntrinsic> {
    let op = match (bind, mode) {
        ("succeedContext", ForeignMode::Pure) => LayerIntrinsic::LayerSucceedContext,
        ("fresh", ForeignMode::Pure) => LayerIntrinsic::LayerFresh,
        ("flickMergeAll", ForeignMode::Pure) => LayerIntrinsic::LayerMergeAll,
        ("orDie", ForeignMode::Pure) => LayerIntrinsic::LayerOrDie,
        ("build", ForeignMode::Effect) => LayerIntrinsic::LayerBuild,
        ("flickBuildWithMemoMap", ForeignMode::Effect) => LayerIntrinsic::LayerBuildWithMemoMap,
        ("flickMakeMemoMap", ForeignMode::Effect) => LayerIntrinsic::LayerMakeMemoMap,
        ("forkMemoMap", ForeignMode::Effect) => LayerIntrinsic::LayerForkMemoMap,
        ("flickCurrentMemoMap", ForeignMode::Effect) => LayerIntrinsic::LayerCurrentMemoMap,
        _ => return None,
    };
    (op.arity() == stages).then_some(op)
}
