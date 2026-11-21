//! Audited STM core (`Tx`, `TxRef`, `TxDeferred`) pure IR adapters (Stage 4C).
//!
//! Uses fiber-local `Context.Reference` (`"flick/Tx/Current"`) + `Ref` +
//! `Deferred` for atomic multi-ref journaled commits, nested transaction
//! joining, rollback on failure/conflict, and waiter-set wakeups on `Tx.retry`.

mod atomically;
mod ops;

use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, Local, Lower, LowerError, Pat, Span};

const TX_REF_KEY: &str = "flick/Tx/Current";

impl Lower<'_> {
    pub(super) fn std_tx_foreign(
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
        let b = ExprBuilder::new(span, "$tx");
        let expr = match from {
            "flick:Effect" if bind == "tx" && stages == 1 => atomically::tx_atomically_template(&b),
            "flick:Tx" => ops::tx_ops_template(&b, bind, stages)?,
            "flick:TxRef" => ops::tx_ref_template(&b, bind, stages)?,
            "flick:TxDeferred" => ops::tx_deferred_template(&b, bind, stages)?,
            _ => return None,
        };
        Some(self.expr(&expr, &Env::new(), block))
    }
}

pub(super) fn tx_ctx_ref(b: &ExprBuilder) -> Expr {
    b.app2(
        b.global("Context", "reference"),
        b.str_(TX_REF_KEY),
        b.lam(&["_u"], b.none()),
    )
}

// TxJournal: `Ref (entries, retry, conflict, done, meta)`
// - `entries`: `List (TxRef, val, read_ver : Int, written : Bool)`
// - `meta`: `(attempt : Int, reason : Option RetryReason, start : Float, now : Float, elapsed : Float, since_prev : Float)`
// TxRef: `Ref (val, ver : Int, waiters : List (Deferred none Unit))`
fn j_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["ents", "rtry", "conf", "done", "meta"])
}

fn meta_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["att", "rsn", "st", "nw", "el", "sp"])
}

fn tref_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["rv", "rver", "rws"])
}

fn now_ms(b: &ExprBuilder) -> Expr {
    b.app(b.global("Clock", "current_time_millis"), b.unit())
}

fn with_active_tx(b: &ExprBuilder, on_journal: Expr) -> Expr {
    let missing = b.app(
        b.global("Effect.Core", "die"),
        b.str_("Service not found: effect/Effect/Transaction"),
    );
    let on_some = b.match_(
        b.rget(b.loc("j")),
        [arm(
            j_pat(b),
            b.if_(b.loc("done"), missing.clone(), on_journal),
        )],
    );
    b.block(
        [b.let_("cref", tx_ctx_ref(b))],
        b.match_(
            b.app(b.global("Context", "read"), b.loc("cref")),
            [arm(b.some_pat("j"), on_some), arm(Pat::Wild, missing)],
        ),
    )
}
