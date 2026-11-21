//! Audited `TxQueue`, `TxPubSub`, and `TxSubscriptionRef` pure IR adapters (Stage 4C).

mod pubsub;
mod queue;
mod queue_ops;
mod subref;

use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::ExprBuilder;
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Local, Lower, LowerError, Span};

impl Lower<'_> {
    pub(super) fn std_tx_queue_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        let from = value.binding.from.as_deref()?;
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$txq");
        let expr = match from {
            "flick:TxQueue" if value.mode == ForeignMode::Effect => {
                queue::tx_queue_template(&b, bind, stages)?
            }
            "flick:TxPubSub" => pubsub::tx_pubsub_template(&b, bind, value.mode, stages)?,
            "flick:TxSubscriptionRef" if value.mode == ForeignMode::Effect => {
                subref::tx_subref_template(&b, bind, stages)?
            }
            "flick:TxSubscriptionRef"
                if value.mode == ForeignMode::Pure && bind == "changesStream" && stages == 1 =>
            {
                subref::tx_subref_changes_stream(&b)
            }
            _ => return None,
        };
        Some(self.expr(&expr, &Env::new(), block))
    }
}
