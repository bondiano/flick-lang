//! Audited `PubSub` pure IR adapter (Stage 4B).
//!
//! Broadcast hub with bounded/dropping/sliding/unbounded strategies, replay
//! buffer, graceful `end`, immediate `shutdown`, and scope-owned subscriptions.

mod publish;
mod shared;
mod subscribe;

use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, Local, Lower, LowerError, Span};
use shared::{hub_pat, make_hub, max_sub_len_expr, state_pat, sub_pat};

impl Lower<'_> {
    pub(super) fn std_pubsub_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        if value.binding.from()? != "flick:PubSub" {
            return None;
        }
        let b = ExprBuilder::new(span, "$ps");
        let expr = pubsub_template(
            &b,
            value.binding.export_name().unwrap_or(""),
            value.mode,
            value.stages.len(),
        )?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

fn pubsub_template(b: &ExprBuilder, bind: &str, mode: ForeignMode, stages: usize) -> Option<Expr> {
    if mode == ForeignMode::Pure && bind == "capacity" && stages == 1 {
        let cap = b.if_(
            b.bin(BinOp::Lt, b.loc("cap"), b.int(0)),
            b.int(i32::MAX),
            b.loc("cap"),
        );
        return Some(b.lam(&["hub"], b.match_(b.loc("hub"), [arm(hub_pat(b), cap)])));
    }
    if mode != ForeignMode::Effect {
        return None;
    }
    match (bind, stages) {
        ("flickBounded", 1) => Some(make_hub(b, 0, true)),
        ("flickDropping", 1) => Some(make_hub(b, 1, true)),
        ("flickSliding", 1) => Some(make_hub(b, 2, true)),
        ("flickUnbounded", 1) => Some(make_hub(b, 0, false)),
        ("size", 1) => Some(read_state_tpl(b, max_sub_len_expr(b, b.loc("subs")))),
        ("isFull", 1) => Some(is_full_tpl(b)),
        ("isEmpty", 1) => Some(b.lam(
            &["hub"],
            b.bin(
                BinOp::Eq,
                b.app(b.global("PubSub", "size"), b.loc("hub")),
                b.int(0),
            ),
        )),
        ("isShutdown", 1) => Some(read_state_tpl(b, b.loc("shut"))),
        ("awaitShutdown", 1) => Some(read_state_tpl(b, b.dawait(b.loc("sdef")))),
        ("shutdown", 1) => Some(publish::shutdown_template(b)),
        ("end", 2) => Some(publish::end_template(b)),
        ("publish", 2) => Some(publish::publish_template(b)),
        ("publishAll", 2) => Some(publish::publish_all_template(b)),
        ("subscribe", 1) => Some(subscribe::subscribe_template(b)),
        ("remaining", 1) => Some(b.lam(
            &["sub"],
            b.match_(
                b.loc("sub"),
                [arm(
                    sub_pat(b),
                    b.app(b.global("List.Core", "length"), b.rget(b.loc("sbuf"))),
                )],
            ),
        )),
        ("take", 1) => Some(subscribe::sub_take_template(b)),
        ("takeAll", 1) => Some(subscribe::sub_take_all_template(b)),
        ("takeUpTo", 2) => Some(subscribe::sub_take_up_to_template(b)),
        ("takeBetween", 3) => Some(subscribe::sub_take_between_template(b)),
        _ => None,
    }
}

fn read_state_tpl(b: &ExprBuilder, body: Expr) -> Expr {
    let from_ref = b.match_(b.rget(b.loc("r")), [arm(state_pat(b), body)]);
    b.lam(
        &["hub"],
        b.match_(b.loc("hub"), [arm(hub_pat(b), from_ref)]),
    )
}

fn is_full_tpl(b: &ExprBuilder) -> Expr {
    let full = b.bin(
        BinOp::And,
        b.bin(BinOp::Ge, b.loc("cap"), b.int(0)),
        b.bin(
            BinOp::Ge,
            b.app(b.global("PubSub", "size"), b.loc("hub")),
            b.loc("cap"),
        ),
    );
    b.lam(&["hub"], b.match_(b.loc("hub"), [arm(hub_pat(b), full)]))
}
