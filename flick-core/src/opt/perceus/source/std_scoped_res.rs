//! Audited `ScopedRef`, `Resource.{manual,use,refresh}`, and `Effect.resource_auto`
//! pure IR adapters + shared manual-scope/holder helpers (Stage 4B).

mod resource;
mod scoped_ref;

use flick_ir::memory::LayerIntrinsic;
use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, Intrinsic, Local, Lower, LowerError, Pat, Span, Stmt, free};

impl Lower<'_> {
    pub(super) fn lower_with_manual_scope(
        &mut self,
        expr: &Expr,
        prefix: &str,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let used = free(expr);
        let mut env = Env::new();
        for (name, op) in [
            ("owner_new", LayerIntrinsic::OwnerNew),
            ("owner_run", LayerIntrinsic::OwnerRun),
            ("owner_close", LayerIntrinsic::OwnerClose),
            ("env_capture", LayerIntrinsic::EnvCapture),
            ("env_run", LayerIntrinsic::EnvRun),
            ("ctx_set_ref", LayerIntrinsic::ContextSetFiberRef),
            (
                "metric_fibers_started",
                LayerIntrinsic::MetricRuntimeFibersStarted,
            ),
        ] {
            let full = format!("{prefix}.{name}");
            if used.contains(&full) {
                let local = self.intrinsic(Intrinsic::Layer(op), false, vec![], block, span)?;
                env.insert(full, local);
            }
        }
        let show_key = format!("{prefix}.show_val");
        if used.contains(&show_key) {
            let local = self.intrinsic(Intrinsic::Show, false, vec![], block, span)?;
            env.insert(show_key, local);
        }
        self.expr(expr, &env, block)
    }

    pub(super) fn std_scoped_res_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || value.mode != ForeignMode::Effect || !has_standard_shape(value) {
            return None;
        }
        let (from, bind) = value.binding.module_export()?;
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$sr");
        let expr = match from {
            "flick:ScopedRef" => scoped_ref::scoped_ref_template(&b, bind, stages)?,
            "flick:Resource" => resource::resource_template(&b, bind, stages)?,
            "flick:Effect" if bind == "flickResourceAuto" && stages == 2 => {
                resource::resource_auto_template(&b)
            }
            _ => return None,
        };
        Some(self.lower_with_manual_scope(&expr, "$sr", block, span))
    }
}

// Holder: `(val : a, owner : Option Owner, meta : Ref (leases : Int, retired : Bool, closed : Bool))`
pub(super) fn acquire_holder_expr(
    b: &ExprBuilder,
    acquire_task: Expr,
    env_opt: Option<Expr>,
) -> Expr {
    let run_body = match env_opt {
        Some(env) => b.app2(b.loc("env_run"), env, acquire_task),
        None => b.await_(acquire_task),
    };
    let run_in_owner = b.task(b.app2(b.loc("owner_run"), b.loc("ho"), b.task(run_body)));
    let on_ok = b.tuple([
        b.loc("hv"),
        b.some(b.loc("ho")),
        b.rmake(b.tuple([b.int(0), b.bool_(false), b.bool_(false)])),
    ]);
    let close_and_rethrow = b.block(
        [Stmt::Expr(b.app2(
            b.loc("owner_close"),
            b.loc("ho"),
            b.some(b.loc("hc")),
        ))],
        b.app(b.global("Failure", "rethrow"), b.loc("hc")),
    );
    let on_fail = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("hex")),
        [
            arm(b.some_pat("hc"), close_and_rethrow),
            arm(
                Pat::Wild,
                b.app(b.global("Effect.Core", "die"), b.str_("acquire exit")),
            ),
        ],
    );
    let tail = b.match_(
        b.app(b.global("Exit", "value"), b.loc("hex")),
        [arm(b.some_pat("hv"), on_ok), arm(Pat::Wild, on_fail)],
    );
    b.block(
        [
            b.let_("ho", b.app(b.loc("owner_new"), b.unit())),
            b.let_("hex", b.app(b.global("Effect.Core", "exit"), run_in_owner)),
        ],
        tail,
    )
}

pub(super) fn plain_holder_expr(b: &ExprBuilder, val: Expr) -> Expr {
    b.tuple([
        val,
        b.none(),
        b.rmake(b.tuple([b.int(0), b.bool_(false), b.bool_(false)])),
    ])
}

pub(super) fn holder_close_owner(b: &ExprBuilder, owner_opt: Expr) -> Expr {
    b.match_(
        owner_opt,
        [
            arm(
                b.some_pat("ho"),
                b.app2(b.loc("owner_close"), b.loc("ho"), b.none()),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    )
}

pub(super) fn retire_holder_expr(b: &ExprBuilder, holder: Expr) -> Expr {
    let close_now = b.block(
        [b.rset(
            b.loc("hmeta"),
            b.tuple([b.int(0), b.bool_(true), b.bool_(true)]),
        )],
        holder_close_owner(b, b.loc("hown")),
    );
    let mark_retired = b.block(
        [b.rset(
            b.loc("hmeta"),
            b.tuple([b.loc("ls"), b.bool_(true), b.loc("cls")]),
        )],
        b.unit(),
    );
    let should_close = b.bin(
        BinOp::And,
        b.not(b.loc("ret")),
        b.bin(BinOp::Eq, b.loc("ls"), b.int(0)),
    );
    let on_meta = b.match_(
        b.rget(b.loc("hmeta")),
        [arm(
            b.tup_pat(&["ls", "ret", "cls"]),
            b.if_(should_close, close_now, mark_retired),
        )],
    );
    b.match_(holder, [arm(b.tup_pat(&["_hv", "hown", "hmeta"]), on_meta)])
}

pub(super) fn lease_holder_expr(b: &ExprBuilder, get_holder: Expr, use_fn: Expr) -> Expr {
    let should_close = b.bin(
        BinOp::And,
        b.loc("ret2"),
        b.bin(
            BinOp::And,
            b.bin(BinOp::Eq, b.loc("nls"), b.int(0)),
            b.not(b.loc("cls2")),
        ),
    );
    let close_now = b.block(
        [b.rset(
            b.loc("hmeta"),
            b.tuple([b.int(0), b.bool_(true), b.bool_(true)]),
        )],
        holder_close_owner(b, b.loc("hown")),
    );
    let keep_open = b.block(
        [b.rset(
            b.loc("hmeta"),
            b.tuple([b.loc("nls"), b.loc("ret2"), b.loc("cls2")]),
        )],
        b.unit(),
    );
    let release_body = b.block(
        [b.let_("nls", b.sub_i(b.loc("ls2"), b.int(1)))],
        b.if_(should_close, close_now, keep_open),
    );
    let release_lease = b.match_(
        b.rget(b.loc("hmeta")),
        [arm(b.tup_pat(&["ls2", "ret2", "cls2"]), release_body)],
    );
    let inc_meta = b.rset(
        b.loc("hmeta"),
        b.tuple([b.add_i(b.loc("ls"), b.int(1)), b.loc("ret"), b.bool_(false)]),
    );
    let run_leased = b.app2(
        b.global("Effect.Errors", "ensuring"),
        b.task(release_lease),
        b.task(b.app(use_fn, b.loc("hv"))),
    );
    let acquire_lease = b.block([inc_meta], b.break_("lse", run_leased));
    let on_meta = b.match_(
        b.rget(b.loc("hmeta")),
        [arm(
            b.tup_pat(&["ls", "ret", "cls"]),
            b.if_(b.loc("cls"), b.unit(), acquire_lease),
        )],
    );
    let step = b.match_(
        get_holder,
        [arm(b.tup_pat(&["hv", "hown", "hmeta"]), on_meta)],
    );
    b.loop_ctrl("lse", step)
}
