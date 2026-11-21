//! Audited `Queue` pure IR adapter (Stage 4B).
//!
//! Full bounded/unbounded/dropping/sliding async queue with typed failure
//! propagation, graceful `end`/`interrupt`, immediate `shutdown`, and
//! interruptible offer/take waiters — zero extra VM tables.

mod offer;
mod shared;
mod take;
mod take_batch;

use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, Local, Lower, LowerError, Pat, Span};
use shared::{q_finish_expr, q_make_expr, q_state_pat, q_wake_takes};

impl Lower<'_> {
    pub(super) fn std_queue_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || value.mode != ForeignMode::Effect || !has_standard_shape(value) {
            return None;
        }
        if value.binding.from.as_deref()? != "flick:Queue" {
            return None;
        }
        let b = ExprBuilder::new(span, "$q");
        let expr = queue_template(&b, value.binding.name.as_str(), value.stages.len())?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

fn queue_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickMake", 1) => Some(make_template(b)),
        ("bounded", 1) => Some(b.lam(&["c"], q_make_expr(b, b.loc("c"), b.int(0)))),
        ("dropping", 1) => Some(b.lam(&["c"], q_make_expr(b, b.loc("c"), b.int(1)))),
        ("sliding", 1) => Some(b.lam(&["c"], q_make_expr(b, b.loc("c"), b.int(2)))),
        ("unbounded", 1) => Some(b.lam(&["_u"], q_make_expr(b, b.int(-1), b.int(0)))),
        ("offer", 2) => Some(offer::offer_template(b)),
        ("offerAll", 2) => Some(offer::offer_all_template(b)),
        ("fail", 2) => Some(b.lam(
            &["e", "q"],
            b.app(
                q_finish_expr(
                    b,
                    false,
                    b.some(b.app(b.global("Cause", "fail"), b.loc("e"))),
                ),
                b.loc("q"),
            ),
        )),
        ("flickFailCause", 2) => Some(b.lam(
            &["c", "q"],
            b.app(q_finish_expr(b, false, b.some(b.loc("c"))), b.loc("q")),
        )),
        ("end", 1) => Some(q_finish_expr(b, false, b.none())),
        ("interrupt", 1) => Some(q_finish_expr(
            b,
            false,
            b.some(b.app(b.global("Cause", "interrupt"), b.none())),
        )),
        ("shutdown", 1) => Some(q_finish_expr(
            b,
            true,
            b.some(b.app(b.global("Cause", "interrupt"), b.none())),
        )),
        ("clear", 1) => Some(take::clear_template(b)),
        ("flickTakeAll", 1) => Some(take_batch::take_all_template(b)),
        ("collect", 1) => Some(take_batch::collect_template(b)),
        ("flickTakeN", 2) => Some(b.lam(
            &["n", "q"],
            b.app3(
                b.global("Queue", "take_between"),
                b.loc("n"),
                b.loc("n"),
                b.loc("q"),
            ),
        )),
        ("flickTakeBetween", 3) => Some(take_batch::take_between_template(b)),
        ("flickTake", 1) => Some(take::take_one_template(b)),
        ("poll", 1) => Some(take::poll_template(b)),
        ("flickPeek", 1) => Some(take::peek_template(b)),
        ("flush", 1) => Some(flush_template(b)),
        ("await", 1) => Some(await_template(b)),
        ("size", 1) => Some(size_template(b)),
        ("isFull", 1) => Some(is_full_template(b)),
        ("into", 2) => Some(into_template(b)),
        _ => None,
    }
}

fn make_template(b: &ExprBuilder) -> Expr {
    let strat_pat = |name: &str| Pat::Con(ExprBuilder::con("Queue", "Strategy", name, 0), vec![]);
    let cap = b.match_(
        b.field(b.loc("opts"), "capacity"),
        [arm(b.some_pat("c"), b.loc("c")), arm(Pat::Wild, b.int(-1))],
    );
    let decode_strat = b.match_(
        b.loc("st"),
        [
            arm(strat_pat("Dropping"), b.int(1)),
            arm(strat_pat("Sliding"), b.int(2)),
            arm(Pat::Wild, b.int(0)),
        ],
    );
    let strat = b.match_(
        b.field(b.loc("opts"), "strategy"),
        [
            arm(b.some_pat("st"), decode_strat),
            arm(Pat::Wild, b.int(0)),
        ],
    );
    b.lam(
        &["opts"],
        b.block(
            [b.let_("cap", cap), b.let_("strat", strat)],
            q_make_expr(b, b.loc("cap"), b.loc("strat")),
        ),
    )
}

fn flush_template(b: &ExprBuilder) -> Expr {
    let updated = b.tuple([
        b.loc("cap"),
        b.loc("strat"),
        b.loc("items"),
        b.loc("status"),
        b.loc("offers"),
        b.list(vec![]),
        b.loc("done_d"),
    ]);
    let body = b.block(
        [b.rset(b.loc("q"), updated), q_wake_takes(b, b.loc("takes"))],
        b.unit(),
    );
    b.lam(
        &["q"],
        b.no_preempt(b.match_(b.rget(b.loc("q")), [arm(q_state_pat(b), body)])),
    )
}

fn await_template(b: &ExprBuilder) -> Expr {
    let on_done = b.match_(
        b.dawait(b.loc("done_d")),
        [
            arm(
                b.some_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    );
    b.lam(
        &["q"],
        b.match_(b.rget(b.loc("q")), [arm(q_state_pat(b), on_done)]),
    )
}

fn size_template(b: &ExprBuilder) -> Expr {
    let len = b.app(b.global("List.Core", "length"), b.loc("items"));
    b.lam(
        &["q"],
        b.match_(b.rget(b.loc("q")), [arm(q_state_pat(b), len)]),
    )
}

fn is_full_template(b: &ExprBuilder) -> Expr {
    let full = b.bin(
        BinOp::And,
        b.bin(BinOp::Ge, b.loc("cap"), b.int(0)),
        b.bin(
            BinOp::Ge,
            b.app(b.global("List.Core", "length"), b.loc("items")),
            b.loc("cap"),
        ),
    );
    b.lam(
        &["q"],
        b.match_(b.rget(b.loc("q")), [arm(q_state_pat(b), full)]),
    )
}

fn into_template(b: &ExprBuilder) -> Expr {
    let exit_fail = b.app(
        b.global("Exit", "failure"),
        b.app(b.global("Effect.Core", "exit"), b.loc("t")),
    );
    let body = b.match_(
        exit_fail,
        [
            arm(
                b.some_pat("c"),
                b.app2(b.global("Queue", "fail_cause"), b.loc("c"), b.loc("q")),
            ),
            arm(Pat::Wild, b.app(b.global("Queue", "end"), b.loc("q"))),
        ],
    );
    b.lam(&["q", "t"], body)
}
