//! Audited `Scheduler`, `ExecutionPlan`, and `Effect.with_execution_plan`
//! pure IR adapters (Stage 4D).

mod run;

use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, ExprKind, Local, Lower, LowerError, Pat, Span};

const SCHED_CUR_KEY: &str = "flick/Scheduler/Current";
const SCHED_MAX_OPS_KEY: &str = "flick/Scheduler/MaxOpsBeforeYield";
const SCHED_PREVENT_KEY: &str = "flick/Scheduler/PreventYield";
pub(super) const PLAN_META_KEY: &str = "flick/ExecutionPlan/Metadata";

impl Lower<'_> {
    pub(super) fn std_sched_plan_foreign(
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
        let b = ExprBuilder::new(span, "$sp");
        let expr = match from {
            "flick:Scheduler" => scheduler_template(&b, bind, value.mode, stages)?,
            "flick:ExecutionPlan" => plan_template(&b, bind, value.mode, stages)?,
            "flick:Effect"
                if value.mode == ForeignMode::Effect
                    && bind == "flickWithExecutionPlan"
                    && stages == 3 =>
            {
                run::with_execution_plan_template(&b)
            }
            _ => return None,
        };
        Some(self.expr(&expr, &Env::new(), block))
    }
}

fn ctx_ref(b: &ExprBuilder, key: &str, default: Expr) -> Expr {
    b.app2(
        b.global("Context", "reference"),
        b.str_(key),
        b.lam(&["_u"], default),
    )
}

fn scheduler_template(
    b: &ExprBuilder,
    bind: &str,
    mode: ForeignMode,
    stages: usize,
) -> Option<Expr> {
    match (bind, mode, stages) {
        ("flickMakeMixed", ForeignMode::Sync | ForeignMode::Pure, 1) => {
            Some(b.lam(&["_u"], b.tuple([b.str_("mixed")])))
        }
        ("flickCurrent", ForeignMode::Effect, 1) => Some(b.lam(
            &["_u"],
            b.app(
                b.global("Context", "read"),
                ctx_ref(b, SCHED_CUR_KEY, b.tuple([b.str_("mixed")])),
            ),
        )),
        ("flickWithScheduler", ForeignMode::Effect, 2) => Some(b.lam(
            &["s", "t"],
            b.app3(
                b.global("Context", "with_reference"),
                ctx_ref(b, SCHED_CUR_KEY, b.tuple([b.str_("mixed")])),
                b.loc("s"),
                b.loc("t"),
            ),
        )),
        ("flickMaxOpsBeforeYield", ForeignMode::Effect, 1) => Some(b.lam(
            &["_u"],
            b.app(
                b.global("Context", "read"),
                ctx_ref(b, SCHED_MAX_OPS_KEY, b.int(2048)),
            ),
        )),
        ("flickWithMaxOpsBeforeYield", ForeignMode::Effect, 2) => Some(b.lam(
            &["n", "t"],
            b.app3(
                b.global("Context", "with_reference"),
                ctx_ref(b, SCHED_MAX_OPS_KEY, b.int(2048)),
                b.loc("n"),
                b.loc("t"),
            ),
        )),
        ("flickWithPreventSchedulerYield", ForeignMode::Effect, 2) => Some(b.lam(
            &["flag", "t"],
            b.app3(
                b.global("Context", "with_reference"),
                ctx_ref(b, SCHED_PREVENT_KEY, b.bool_(false)),
                b.loc("flag"),
                b.loc("t"),
            ),
        )),
        _ => None,
    }
}

// ExecutionPlan Step: `(layer, attempts : Int, opt_schedule : Option Schedule, opt_while : Option Fn)`
// ExecutionPlan: `List Step`
pub(super) fn step_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["lay", "atts", "osch", "owh"])
}

pub(super) fn plan_meta_ref(b: &ExprBuilder) -> Expr {
    ctx_ref(
        b,
        PLAN_META_KEY,
        b.at(ExprKind::Record(vec![
            ("attempt".into(), b.int(0)),
            ("stepIndex".into(), b.int(0)),
        ])),
    )
}

fn plan_template(b: &ExprBuilder, bind: &str, mode: ForeignMode, stages: usize) -> Option<Expr> {
    match (bind, mode, stages) {
        ("flickStep", ForeignMode::Pure, 1) => Some(b.lam(
            &["lay"],
            b.tuple([b.loc("lay"), b.int(0), b.none(), b.none()]),
        )),
        ("flickWithAttempts", ForeignMode::Pure, 2) => Some(b.lam(
            &["n", "st"],
            b.match_(
                b.loc("st"),
                [arm(
                    step_pat(b),
                    b.tuple([b.loc("lay"), b.loc("n"), b.loc("osch"), b.loc("owh")]),
                )],
            ),
        )),
        ("flickWithSchedule", ForeignMode::Pure, 2) => Some(b.lam(
            &["sc", "st"],
            b.match_(
                b.loc("st"),
                [arm(
                    step_pat(b),
                    b.tuple([
                        b.loc("lay"),
                        b.loc("atts"),
                        b.some(b.loc("sc")),
                        b.loc("owh"),
                    ]),
                )],
            ),
        )),
        ("flickWithWhile", ForeignMode::Pure, 2) => Some(b.lam(
            &["wh", "st"],
            b.match_(
                b.loc("st"),
                [arm(
                    step_pat(b),
                    b.tuple([
                        b.loc("lay"),
                        b.loc("atts"),
                        b.loc("osch"),
                        b.some(b.loc("wh")),
                    ]),
                )],
            ),
        )),
        ("flickMake", ForeignMode::Pure, 1) => {
            Some(b.lam(&["ne"], b.app(b.global("NonEmpty", "to_list"), b.loc("ne"))))
        }
        ("flickMerge", ForeignMode::Pure, 1) => Some(b.lam(
            &["ne"],
            b.app3(
                b.global("List.Core", "fold_left"),
                b.lam(
                    &["acc", "p"],
                    b.app2(b.global("List.Core", "append"), b.loc("acc"), b.loc("p")),
                ),
                b.list(vec![]),
                b.app(b.global("NonEmpty", "to_list"), b.loc("ne")),
            ),
        )),
        ("flickCurrentMetadata", ForeignMode::Effect, 1) => Some(b.lam(
            &["_u"],
            b.app(b.global("Context", "read"), plan_meta_ref(b)),
        )),
        _ => None,
    }
}

pub(super) fn build_retry_schedule(b: &ExprBuilder, osch: Expr, owh: Expr, times: Expr) -> Expr {
    let base = b.match_(
        osch,
        [
            arm(b.some_pat("s"), b.loc("s")),
            arm(Pat::Wild, b.global("Schedule", "forever")),
        ],
    );
    let while_sched = b.app2(
        b.global("Schedule", "while"),
        b.lam(&["m"], b.app(b.loc("wh"), b.field(b.loc("m"), "input"))),
        b.loc("s0"),
    );
    let with_wh = b.match_(
        owh,
        [
            arm(b.some_pat("wh"), while_sched),
            arm(Pat::Wild, b.loc("s0")),
        ],
    );
    let up_to_opts = b.at(ExprKind::Record(vec![
        ("duration".into(), b.none()),
        ("times".into(), b.some(b.loc("ts"))),
    ]));
    let with_t = b.if_(
        b.bin(BinOp::Ge, b.loc("ts"), b.int(0)),
        b.app2(b.global("Schedule", "up_to"), up_to_opts, b.loc("s1")),
        b.loc("s1"),
    );
    b.block(
        [
            b.let_("s0", base),
            b.let_("s1", with_wh),
            b.let_("ts", times),
        ],
        with_t,
    )
}
