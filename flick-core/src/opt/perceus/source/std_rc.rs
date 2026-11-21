//! Audited `RcRef` and `RcMap` pure IR adapters (Stage 4B).
//!
//! Reference-counted scope-shared resources with optional `idleTimeToLive`,
//! capacity checks (`RcMap.ExceededCapacity`), and `invalidate`/`touch`.

mod map_get;
mod map_ops;
mod rc_ref;

use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Expr, Local, Lower, LowerError, Pat, Span, Stmt};

impl Lower<'_> {
    pub(super) fn std_rc_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || value.mode != ForeignMode::Effect || !has_standard_shape(value) {
            return None;
        }
        let from = value.binding.from.as_deref()?;
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$rc");
        let expr = match from {
            "flick:RcRef" => rc_ref::rc_ref_template(&b, bind, stages)?,
            "flick:RcMap" => map_ops::rc_map_template(&b, bind, stages)?,
            _ => return None,
        };
        Some(self.lower_with_manual_scope(&expr, "$rc", block, span))
    }
}

// RcEntry: `Ref (val, owner, holders : Int, retired : Bool, closed : Bool, epoch : Int)`
fn schedule_idle_expr(
    b: &ExprBuilder,
    entry_var: Expr,
    parent_owner: Expr,
    on_evict: Expr,
    close_now: Expr,
) -> Expr {
    let still_idle = b.bin(
        BinOp::And,
        b.not(b.loc("c3")),
        b.bin(
            BinOp::And,
            b.bin(BinOp::Eq, b.loc("h3"), b.int(0)),
            b.bin(BinOp::Eq, b.loc("ep3"), b.loc("my_ep")),
        ),
    );
    let closed_entry = b.tuple([
        b.loc("ev3"),
        b.loc("eo3"),
        b.int(0),
        b.bool_(true),
        b.bool_(true),
        b.loc("ep3"),
    ]);
    let do_evict = b.block(
        [
            b.rset(entry_var.clone(), closed_entry),
            Stmt::Expr(on_evict),
        ],
        b.app2(b.loc("owner_close"), b.loc("eo3"), b.none()),
    );
    let check_entry = b.match_(
        b.rget(entry_var),
        [arm(
            b.tup_pat(&["ev3", "eo3", "h3", "_r3", "c3", "ep3"]),
            b.if_(still_idle, do_evict, b.unit()),
        )],
    );
    let timer_task = b.task(b.block(
        [Stmt::Expr(
            b.app(b.global("Effect.Core", "sleep"), b.loc("dur")),
        )],
        check_entry,
    ));
    let spawn_timer = b.block(
        [
            b.let_("my_ep", b.loc("ep")),
            b.let_(
                "_tf",
                b.app3(
                    b.global("Fiber", "spawn"),
                    parent_owner,
                    b.lam(&["_c"], b.unit()),
                    timer_task,
                ),
            ),
        ],
        b.unit(),
    );
    let non_zero = b.if_(
        b.not(b.app(b.global("Duration", "is_finite"), b.loc("dur"))),
        b.unit(),
        spawn_timer,
    );
    b.lam(
        &["dur"],
        b.if_(
            b.app(b.global("Duration", "is_zero"), b.loc("dur")),
            close_now,
            non_zero,
        ),
    )
}

fn rc_entry_release(
    b: &ExprBuilder,
    entry_var: Expr,
    idle_opt: Expr,
    parent_owner: Expr,
    on_evict: Expr,
) -> Expr {
    let close_now = b.block(
        [
            b.rset(
                entry_var.clone(),
                b.tuple([
                    b.loc("ev"),
                    b.loc("eo"),
                    b.int(0),
                    b.bool_(true),
                    b.bool_(true),
                    b.loc("ep"),
                ]),
            ),
            Stmt::Expr(on_evict.clone()),
        ],
        b.app2(b.loc("owner_close"), b.loc("eo"), b.none()),
    );
    let schedule_idle = schedule_idle_expr(
        b,
        entry_var.clone(),
        parent_owner,
        on_evict,
        close_now.clone(),
    );
    let on_unretired = b.match_(
        idle_opt,
        [
            arm(b.some_pat("d"), b.app(schedule_idle, b.loc("d"))),
            arm(Pat::Wild, close_now.clone()),
        ],
    );
    let should_close = b.bin(
        BinOp::And,
        b.not(b.loc("cls")),
        b.bin(BinOp::Eq, b.loc("nh"), b.int(0)),
    );
    let on_zero = b.if_(b.loc("ret"), close_now, on_unretired);
    let updated = b.tuple([
        b.loc("ev"),
        b.loc("eo"),
        b.loc("nh"),
        b.loc("ret"),
        b.loc("cls"),
        b.loc("ep"),
    ]);
    let on_entry = b.block(
        [
            b.let_("nh", b.sub_i(b.loc("h"), b.int(1))),
            b.rset(entry_var.clone(), updated),
        ],
        b.if_(should_close, on_zero, b.unit()),
    );
    b.match_(
        b.rget(entry_var),
        [arm(
            b.tup_pat(&["ev", "eo", "h", "ret", "cls", "ep"]),
            on_entry,
        )],
    )
}

fn rc_retire_entry(b: &ExprBuilder, entry_var: Expr) -> Expr {
    let closed_now = b.block(
        [b.rset(
            entry_var.clone(),
            b.tuple([
                b.loc("ev"),
                b.loc("eo"),
                b.int(0),
                b.bool_(true),
                b.bool_(true),
                b.loc("ep"),
            ]),
        )],
        b.app2(b.loc("owner_close"), b.loc("eo"), b.none()),
    );
    let mark_retired = b.block(
        [b.rset(
            entry_var.clone(),
            b.tuple([
                b.loc("ev"),
                b.loc("eo"),
                b.loc("h"),
                b.bool_(true),
                b.bool_(false),
                b.loc("ep"),
            ]),
        )],
        b.unit(),
    );
    let on_open = b.if_(
        b.bin(BinOp::Eq, b.loc("h"), b.int(0)),
        closed_now,
        mark_retired,
    );
    b.match_(
        b.rget(entry_var),
        [arm(
            b.tup_pat(&["ev", "eo", "h", "_ret", "cls", "ep"]),
            b.if_(b.loc("cls"), b.unit(), on_open),
        )],
    )
}
