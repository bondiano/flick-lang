//! Audited `Exit`, `Cause`, `Failure`, and `Effect` outcome adapters.

use flick_ir::memory::OutcomeIntrinsic;
use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, ExprKind, Intrinsic, Local, Lower, LowerError, Operation, Pat, Span};

impl Lower<'_> {
    pub(super) fn std_outcome_foreign(
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

        if value.mode == ForeignMode::Effect {
            return self.effect_outcome_foreign(from, bind, stages, span, block);
        }
        if value.mode == ForeignMode::Sync {
            if from == "flick:Cause" && bind == "flickFrameStack" && stages == 1 {
                let b = ExprBuilder::new(span, "$fs");
                let expr = b.lam(
                    &["fr"],
                    b.match_(
                        b.loc("fr"),
                        vec![arm(
                            b.tup_pat(&["_nm", "st", "_par"]),
                            b.if_(
                                b.bin(flick_ir::BinOp::Eq, b.loc("st"), b.str_("")),
                                b.none(),
                                b.some(b.loc("st")),
                            ),
                        )],
                    ),
                );
                return Some(self.expr(&expr, &Env::new(), block));
            }
            let kind = match (from, bind, stages) {
                ("flick:Cause", "flickPretty", 1) => Some(OutcomeIntrinsic::CausePretty),
                ("flick:Cause", "flickPrettyErrors", 1) => {
                    Some(OutcomeIntrinsic::CausePrettyErrors)
                }
                _ => None,
            }?;
            return Some(self.intrinsic(Intrinsic::Outcome(kind), false, vec![], block, span));
        }
        if value.mode != ForeignMode::Pure {
            return None;
        }
        if stages == 0
            && let Some(kind) = zero_arg_intrinsic(from, bind)
        {
            return Some(block.emit(Operation::Intrinsic { kind, args: vec![] }, span));
        }
        if let Some(kind) = direct_pure_intrinsic(from, bind, stages) {
            return Some(self.intrinsic(kind, false, vec![], block, span));
        }
        if from == "flick:Cause" && bind == "flickMap" && stages == 2 {
            let expr = cause_map_template(&ExprBuilder::new(span, "$cm"));
            return Some(self.expr(&expr, &Env::new(), block).and_then(|function| {
                let argument = self.intrinsic(
                    Intrinsic::Outcome(OutcomeIntrinsic::CauseReplaceFirstFail),
                    false,
                    vec![],
                    block,
                    span,
                )?;
                block.emit(Operation::Apply { function, argument }, span)
            }));
        }
        let b = ExprBuilder::new(span, "$out");
        let expr = pure_template(&b, from, bind, stages)?;
        Some(self.expr(&expr, &Env::new(), block))
    }

    fn effect_outcome_foreign(
        &mut self,
        from: &str,
        bind: &str,
        stages: usize,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        match (from, bind, stages) {
            ("flick:Effect", "flickExit", 1) => {
                Some(self.intrinsic(Intrinsic::EffectExit, false, vec![], block, span))
            }
            ("flick:Effect", "die", 1) => {
                Some(self.intrinsic(Intrinsic::EffectDie, false, vec![], block, span))
            }
            ("flick:Effect", "flickCatchDefect", 2) => {
                Some(self.intrinsic(Intrinsic::EffectCatchDefect, false, vec![], block, span))
            }
            ("flick:rt", "failureRethrow", 1) => {
                Some(self.intrinsic(Intrinsic::FailureRethrow, false, vec![], block, span))
            }
            ("flick:Effect", "flickFailCauseSync", 1) => {
                let b = ExprBuilder::new(span, "$fcs");
                let expr = b.lam(
                    &["f"],
                    b.app(b.global("Failure", "rethrow"), b.app(b.loc("f"), b.unit())),
                );
                Some(self.expr(&expr, &Env::new(), block))
            }
            ("flick:Effect", "zip", 2) => {
                let b = ExprBuilder::new(span, "$ez");
                let expr = b.lam(
                    &["other", "self"],
                    b.tuple(vec![
                        b.at(ExprKind::Await(Box::new(b.loc("self")))),
                        b.at(ExprKind::Await(Box::new(b.loc("other")))),
                    ]),
                );
                Some(self.expr(&expr, &Env::new(), block))
            }
            ("flick:Effect", "zipWith", 3) => {
                let b = ExprBuilder::new(span, "$ezw");
                let expr = b.lam(
                    &["other", "f", "self"],
                    b.app2(
                        b.loc("f"),
                        b.at(ExprKind::Await(Box::new(b.loc("self")))),
                        b.at(ExprKind::Await(Box::new(b.loc("other")))),
                    ),
                );
                Some(self.expr(&expr, &Env::new(), block))
            }
            ("flick:Effect", "when", 2) => {
                let b = ExprBuilder::new(span, "$ew");
                let expr = b.lam(
                    &["cond", "body"],
                    b.if_(
                        b.at(ExprKind::Await(Box::new(b.loc("cond")))),
                        b.some(b.at(ExprKind::Await(Box::new(b.loc("body"))))),
                        b.none(),
                    ),
                );
                Some(self.expr(&expr, &Env::new(), block))
            }
            _ => None,
        }
    }
}

fn zero_arg_intrinsic(from: &str, bind: &str) -> Option<Intrinsic> {
    match (from, bind) {
        ("flick:Exit", "flickVoid") => Some(Intrinsic::Outcome(OutcomeIntrinsic::ExitVoid)),
        ("flick:Cause", "flickEmpty") => Some(Intrinsic::Outcome(OutcomeIntrinsic::CauseEmpty)),
        _ => None,
    }
}

fn direct_pure_intrinsic(from: &str, bind: &str, stages: usize) -> Option<Intrinsic> {
    let kind = match (from, bind) {
        ("flick:rt", "exitValue") => Intrinsic::Outcome(OutcomeIntrinsic::ExitValue),
        ("flick:rt", "exitFailure") => Intrinsic::Outcome(OutcomeIntrinsic::ExitFailure),
        ("flick:Exit", "flickSucceed") => Intrinsic::Outcome(OutcomeIntrinsic::ExitSucceed),
        ("flick:Exit", "flickFail") => Intrinsic::Outcome(OutcomeIntrinsic::ExitFail),
        ("flick:Exit", "flickFailCause") => Intrinsic::Outcome(OutcomeIntrinsic::ExitFailCause),
        ("flick:Exit", "flickDie") => Intrinsic::Outcome(OutcomeIntrinsic::ExitDie),
        ("flick:Exit", "flickInterrupt") => Intrinsic::Outcome(OutcomeIntrinsic::ExitInterrupt),
        ("flick:Exit", "flickHasFails") => Intrinsic::Outcome(OutcomeIntrinsic::ExitHasFails),
        ("flick:Exit", "flickHasDies") => Intrinsic::Outcome(OutcomeIntrinsic::ExitHasDies),
        ("flick:Exit", "flickFilterSuccess") => {
            Intrinsic::Outcome(OutcomeIntrinsic::ExitFilterSuccess)
        }
        ("flick:Exit", "flickFilterValue") => Intrinsic::Outcome(OutcomeIntrinsic::ExitFilterValue),
        ("flick:Exit", "flickFilterCause") => Intrinsic::Outcome(OutcomeIntrinsic::ExitFilterCause),
        ("flick:Exit", "flickFindError") => Intrinsic::Outcome(OutcomeIntrinsic::ExitFindError),
        ("flick:Exit", "flickFindErrorOption") => {
            Intrinsic::Outcome(OutcomeIntrinsic::ExitFindErrorOption)
        }
        ("flick:Exit", "flickFindDefect") => Intrinsic::Outcome(OutcomeIntrinsic::ExitFindDefect),
        ("flick:Exit", "flickAsVoid") => Intrinsic::Outcome(OutcomeIntrinsic::ExitAsVoid),
        ("flick:Exit", "flickAsVoidAll") => Intrinsic::Outcome(OutcomeIntrinsic::ExitAsVoidAll),
        ("flick:Cause", "flickFail") => Intrinsic::Outcome(OutcomeIntrinsic::CauseFail),
        ("flick:Cause", "flickDie") => Intrinsic::Outcome(OutcomeIntrinsic::CauseDie),
        ("flick:Cause", "flickInterrupt") => Intrinsic::Outcome(OutcomeIntrinsic::CauseInterrupt),
        ("flick:Cause", "flickFromReasons") => {
            Intrinsic::Outcome(OutcomeIntrinsic::CauseFromReasons)
        }
        ("flick:Cause", "flickReasons") => Intrinsic::Outcome(OutcomeIntrinsic::CauseReasons),
        ("flick:Cause", "makeFailReason") => {
            Intrinsic::Outcome(OutcomeIntrinsic::CauseMakeFailReason)
        }
        ("flick:Cause", "makeDieReason") => {
            Intrinsic::Outcome(OutcomeIntrinsic::CauseMakeDieReason)
        }
        ("flick:Cause", "flickMakeInterruptReason") => {
            Intrinsic::Outcome(OutcomeIntrinsic::CauseMakeInterruptReason)
        }
        ("flick:Cause", "isFailReason") => Intrinsic::Outcome(OutcomeIntrinsic::CauseIsFailReason),
        ("flick:Cause", "isDieReason") => Intrinsic::Outcome(OutcomeIntrinsic::CauseIsDieReason),
        ("flick:Cause", "isInterruptReason") => {
            Intrinsic::Outcome(OutcomeIntrinsic::CauseIsInterruptReason)
        }
        ("flick:Cause", "flickHasInterruptsOnly") => {
            Intrinsic::Outcome(OutcomeIntrinsic::CauseHasInterruptsOnly)
        }
        ("flick:Cause", "flickHasFails") => Intrinsic::Outcome(OutcomeIntrinsic::CauseHasFails),
        ("flick:Cause", "flickHasDies") => Intrinsic::Outcome(OutcomeIntrinsic::CauseHasDies),
        ("flick:Cause", "flickHasInterrupts") => {
            Intrinsic::Outcome(OutcomeIntrinsic::CauseHasInterrupts)
        }
        ("flick:Cause", "flickCombine") => Intrinsic::Outcome(OutcomeIntrinsic::CauseCombine),
        ("flick:Cause", "flickFindFail") => Intrinsic::Outcome(OutcomeIntrinsic::CauseFindFail),
        ("flick:Cause", "flickFindError") => Intrinsic::Outcome(OutcomeIntrinsic::CauseFindError),
        ("flick:Cause", "flickFindErrorOption") | ("flick:rt", "failurePrimary") => {
            Intrinsic::Outcome(OutcomeIntrinsic::CauseFindErrorOption)
        }
        ("flick:Cause", "flickFindDie") => Intrinsic::Outcome(OutcomeIntrinsic::CauseFindDie),
        ("flick:Cause", "flickFindDefect") => Intrinsic::Outcome(OutcomeIntrinsic::CauseFindDefect),
        ("flick:Cause", "flickFindInterrupt") => {
            Intrinsic::Outcome(OutcomeIntrinsic::CauseFindInterrupt)
        }
        ("flick:Cause", "flickInterruptors") => {
            Intrinsic::Outcome(OutcomeIntrinsic::CauseInterruptors)
        }
        ("flick:Cause", "flickFilterInterruptors") => {
            Intrinsic::Outcome(OutcomeIntrinsic::CauseFilterInterruptors)
        }
        ("flick:Cause", "flickStackTrace") => Intrinsic::Outcome(OutcomeIntrinsic::CauseStackTrace),
        ("flick:Cause", "flickAnnotate") => Intrinsic::Outcome(OutcomeIntrinsic::CauseAnnotate),
        ("flick:Cause", "flickInterruptorStackTrace" | "flickReasonStackTrace") => {
            Intrinsic::Outcome(OutcomeIntrinsic::CauseNoneFrame)
        }
        _ => return None,
    };
    (kind.arity() == stages).then_some(kind)
}

fn pure_template(b: &ExprBuilder, from: &str, bind: &str, stages: usize) -> Option<Expr> {
    match (from, bind, stages) {
        ("flick:Cause", "flickFrameName", 1) => Some(b.lam(
            &["fr"],
            b.match_(
                b.loc("fr"),
                vec![arm(b.tup_pat(&["nm", "_st", "_par"]), b.loc("nm"))],
            ),
        )),
        ("flick:Cause", "flickFrameParent", 1) => Some(b.lam(
            &["fr"],
            b.match_(
                b.loc("fr"),
                vec![arm(b.tup_pat(&["_nm", "_st", "par"]), b.loc("par"))],
            ),
        )),
        ("flick:rt", "failureRawCause", 1) => Some(b.lam(&["f"], b.loc("f"))),
        ("flick:Effect", "void", 0) => Some(b.at(ExprKind::Task(Box::new(b.unit())))),
        ("flick:Exit", "flickFilterFailure", 1) => Some(b.lam(
            &["e"],
            b.if_(
                b.app(b.global("Exit", "is_failure"), b.loc("e")),
                b.ok(b.loc("e")),
                b.err(b.loc("e")),
            ),
        )),
        ("flick:Exit", "flickMatch", 3) => Some(b.lam(
            &["on_ok", "on_fail", "e"],
            b.match_(
                b.app(b.global("Exit", "value"), b.loc("e")),
                vec![
                    arm(b.some_pat("v"), b.app(b.loc("on_ok"), b.loc("v"))),
                    arm(
                        Pat::Wild,
                        b.match_(
                            b.app(b.global("Exit", "failure"), b.loc("e")),
                            vec![
                                arm(b.some_pat("f"), b.app(b.loc("on_fail"), b.loc("f"))),
                                arm(
                                    Pat::Wild,
                                    b.app(b.loc("on_fail"), b.global("Cause", "empty")),
                                ),
                            ],
                        ),
                    ),
                ],
            ),
        )),
        ("flick:Exit", "flickMap", 2) => Some(b.lam(
            &["f", "e"],
            b.match_(
                b.app(b.global("Exit", "value"), b.loc("e")),
                vec![
                    arm(
                        b.some_pat("v"),
                        b.app(b.global("Exit", "succeed"), b.app(b.loc("f"), b.loc("v"))),
                    ),
                    arm(Pat::Wild, b.loc("e")),
                ],
            ),
        )),
        ("flick:Exit", "flickMapError", 2) => Some(b.lam(
            &["f", "e"],
            b.match_(
                b.app(b.global("Exit", "failure"), b.loc("e")),
                vec![
                    arm(
                        b.some_pat("c"),
                        b.app(
                            b.global("Exit", "fail_cause"),
                            b.app2(b.global("Cause", "map"), b.loc("f"), b.loc("c")),
                        ),
                    ),
                    arm(Pat::Wild, b.loc("e")),
                ],
            ),
        )),
        ("flick:Exit", "flickMapBoth", 3) => Some(b.lam(
            &["on_fail", "on_ok", "e"],
            b.app2(
                b.global("Exit", "map"),
                b.loc("on_ok"),
                b.app2(b.global("Exit", "map_error"), b.loc("on_fail"), b.loc("e")),
            ),
        )),
        _ => None,
    }
}

fn cause_map_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["replace_fn", "f", "c"],
        b.match_(
            b.app(b.global("Cause", "find_error_option"), b.loc("c")),
            vec![
                arm(
                    b.some_pat("e"),
                    b.app2(
                        b.loc("replace_fn"),
                        b.app(b.loc("f"), b.loc("e")),
                        b.loc("c"),
                    ),
                ),
                arm(Pat::Wild, b.loc("c")),
            ],
        ),
    )
}
