//! Audited `ScopedCache` pure IR adapter (Stage 4B).
//!
//! Resource-owning per-key cache whose entries are leased inside `use`,
//! retired cleanly on eviction/invalidation/refresh/parent-scope-close, and
//! looked up with maker-captured services and fiber refs.

mod entry;
mod ops;

use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Expr, Local, Lower, LowerError, Pat, Span};

impl Lower<'_> {
    pub(super) fn std_scoped_cache_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || value.mode != ForeignMode::Effect || !has_standard_shape(value) {
            return None;
        }
        if value.binding.from.as_deref()? != "flick:ScopedCache" {
            return None;
        }
        let b = ExprBuilder::new(span, "$sc");
        let expr = scoped_cache_template(&b, value.binding.name.as_str(), value.stages.len())?;
        Some(self.lower_with_manual_scope(&expr, "$sc", block, span))
    }
}

// ScopedCache: `(lookup, cap : Int, ttl_fn, env, ents : Ref (List (k, Entry)))`
// Entry: `Ref (state : Option (Result Cause Holder, Option Float), wait : Deferred none (Result Cause Holder, Option Float))`
fn sc_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["lkp", "cap", "ttl", "env", "ents"])
}

fn now_ms(b: &ExprBuilder) -> Expr {
    b.app(b.global("Clock", "current_time_millis"), b.unit())
}

fn is_expired(b: &ExprBuilder, exp_opt: Expr) -> Expr {
    b.match_(
        exp_opt,
        [
            arm(b.some_pat("t"), b.bin(BinOp::Ge, now_ms(b), b.loc("t"))),
            arm(Pat::Wild, b.bool_(false)),
        ],
    )
}

fn find_key(b: &ExprBuilder, ents_ref: Expr, key: Expr) -> Expr {
    let pred = b.lam(
        &["p"],
        b.match_(
            b.loc("p"),
            [arm(
                b.tup_pat(&["ek", "_ee"]),
                b.bin(BinOp::Eq, b.loc("ek"), key),
            )],
        ),
    );
    b.app2(b.global("List.Core", "find"), pred, b.rget(ents_ref))
}

fn drop_key(b: &ExprBuilder, list_expr: Expr, key: Expr) -> Expr {
    let pred = b.lam(
        &["p"],
        b.match_(
            b.loc("p"),
            [arm(
                b.tup_pat(&["ek", "_ee"]),
                b.not(b.bin(BinOp::Eq, b.loc("ek"), key)),
            )],
        ),
    );
    b.app2(b.global("List.Core", "filter"), pred, list_expr)
}

fn scoped_cache_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickMake", 1) => {
            let ttl_fn = b.match_(
                b.field(b.loc("opts"), "timeToLive"),
                [
                    arm(b.some_pat("d"), b.some(b.lam(&["_ex", "_k"], b.loc("d")))),
                    arm(Pat::Wild, b.none()),
                ],
            );
            Some(entry::make_sc_expr(b, ttl_fn))
        }
        ("flickMakeWith", 1) => Some(entry::make_sc_expr(b, b.field(b.loc("opts"), "timeToLive"))),
        ("flickUse", 3) => Some(ops::use_expr(b)),
        ("flickUseOption", 3) => Some(ops::use_opt_expr(b, false)),
        ("flickUseSuccess", 3) => Some(ops::use_opt_expr(b, true)),
        ("flickSet", 3) => Some(ops::set_expr(b)),
        ("has", 2) => Some(ops::has_expr(b)),
        ("invalidate", 2) => Some(ops::invalidate_expr(b)),
        ("flickInvalidateWhen", 3) => Some(ops::invalidate_when_expr(b)),
        ("flickRefresh", 2) => Some(ops::refresh_expr(b)),
        ("invalidateAll", 1) => Some(ops::invalidate_all_expr(b)),
        ("size", 1) => Some(ops::size_expr(b)),
        ("keys", 1) => Some(ops::keys_expr(b)),
        _ => None,
    }
}
