//! Audited `Pool` pure IR adapter (Stage 4B).
//!
//! Fixed-size and TTL-bounded resource pool with FIFO lending, concurrency per
//! item, `reserve`, `invalidate`, defect/interruption invalidation via
//! `Scope.defer`, and automatic refill to `min`.

mod get;
mod manage;
mod slot;

use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Expr, Local, Lower, LowerError, Pat, Span, Stmt};

impl Lower<'_> {
    pub(super) fn std_pool_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || value.mode != ForeignMode::Effect || !has_standard_shape(value) {
            return None;
        }
        if value.binding.from.as_deref()? != "flick:Pool" {
            return None;
        }
        let b = ExprBuilder::new(span, "$pl");
        let expr = pool_template(&b, value.binding.name.as_str(), value.stages.len())?;
        Some(self.lower_with_manual_scope(&expr, "$pl", block, span))
    }
}

// Pool: `(acq, min, max, conc, ttl_opt, by_creation, env, state)`
// state: `Ref (slots : List Slot, waiters : List (Deferred none Unit), closed : Bool)`
// Slot: `Ref (val, owner, in_use : Int, reserved : Bool, invalid : Bool, created_ms : Float, used_ms : Float)`
fn pool_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["acq", "mn", "mx", "conc", "ttl", "byc", "env", "st"])
}

fn st_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["slots", "waits", "closed"])
}

fn slot_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["sv", "so", "su", "sr", "si", "sc", "su_ms"])
}

fn now_ms(b: &ExprBuilder) -> Expr {
    b.app(b.global("Clock", "current_time_millis"), b.unit())
}

fn wake_waiters_stmt(b: &ExprBuilder, waits: Expr) -> Stmt {
    let wake_one = b.block([b.dsucceed(b.unit(), b.loc("d"))], b.unit());
    b.let_("_ww", b.for_ctrl("pww", "d", waits, wake_one))
}

fn pool_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickMake", 1) => Some(slot::make_pool_expr(
            b,
            b.field(b.loc("opts"), "size"),
            b.field(b.loc("opts"), "size"),
            b.none(),
            b.bool_(false),
        )),
        ("flickMakeWithTtl", 1) => {
            let creation_pat = Pat::Con(
                ExprBuilder::con("Pool", "TimeToLiveStrategy", "Creation", 0),
                vec![],
            );
            let is_creation = b.match_(
                b.loc("s"),
                [
                    arm(creation_pat, b.bool_(true)),
                    arm(Pat::Wild, b.bool_(false)),
                ],
            );
            let byc = b.match_(
                b.field(b.loc("opts"), "timeToLiveStrategy"),
                [
                    arm(b.some_pat("s"), is_creation),
                    arm(Pat::Wild, b.bool_(false)),
                ],
            );
            Some(slot::make_pool_expr(
                b,
                b.field(b.loc("opts"), "min"),
                b.field(b.loc("opts"), "max"),
                b.some(b.field(b.loc("opts"), "timeToLive")),
                byc,
            ))
        }
        ("get", 1) => Some(get::pool_get_expr(b)),
        ("use", 2) => {
            let body = b.app(b.loc("f"), b.app(b.global("Pool", "get"), b.loc("pool")));
            Some(b.lam(
                &["f", "pool"],
                b.app(b.global("Scope", "run"), b.lam(&["_ow"], body)),
            ))
        }
        ("invalidate", 2) => Some(manage::pool_invalidate_expr(b)),
        ("reserve", 2) => Some(manage::pool_reserve_expr(b)),
        _ => None,
    }
}
