//! Audited `Cache` pure IR adapter (Stage 4B).
//!
//! Single-flight per-key memoizing cache with capacity eviction (LRU/FIFO),
//! failure caching, optional dynamic TTL (`make_with`), and maker-environment
//! capture via `EnvCapture`/`EnvRun`.

mod lookup;
mod manage;

use flick_ir::{Arith, BinOp, Foreign, ForeignMode, Num};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Expr, Local, Lower, LowerError, Pat, Span, Stmt};

impl Lower<'_> {
    pub(super) fn std_cache_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || value.mode != ForeignMode::Effect || !has_standard_shape(value) {
            return None;
        }
        if value.binding.from()? != "flick:Cache" {
            return None;
        }
        let b = ExprBuilder::new(span, "$ca");
        let expr = cache_template(
            &b,
            value.binding.export_name().unwrap_or(""),
            value.stages.len(),
        )?;
        Some(self.lower_with_manual_scope(&expr, "$ca", block, span))
    }
}

// Cache: `(lookup, cap : Int, ttl_fn : Option (Exit -> k -> Duration), env, ents : Ref (List (k, Entry)))`
// Entry: `Ref (state : Option (Exit, Option Float), wait : Deferred none (Exit, Option Float))`
//   - `state == None`: lookup in flight; waiters await `wait`
//   - `state == Some (exit, exp_ms)`: resolved (`exp_ms == None` never expires)
fn cache_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["lkp", "cap", "ttl", "env", "ents"])
}

fn now_ms(b: &ExprBuilder) -> Expr {
    b.app(b.global("Clock", "current_time_millis"), b.unit())
}

fn is_expired_expr(b: &ExprBuilder, exp_opt: Expr) -> Expr {
    b.match_(
        exp_opt,
        [
            arm(b.some_pat("t"), b.bin(BinOp::Ge, now_ms(b), b.loc("t"))),
            arm(Pat::Wild, b.bool_(false)),
        ],
    )
}

fn compute_exp_expr(b: &ExprBuilder, ttl_opt: Expr, ex_val: Expr, key_val: Expr) -> Expr {
    let deadline = b.bin(BinOp::Arith(Arith::Add, Num::Float), now_ms(b), b.loc("ms"));
    let dur = b.app2(b.loc("tf"), ex_val, key_val);
    let on_tf = b.match_(
        b.app(b.global("Duration", "to_millis"), dur),
        [
            arm(b.some_pat("ms"), b.some(deadline)),
            arm(Pat::Wild, b.none()),
        ],
    );
    b.match_(
        ttl_opt,
        [arm(b.some_pat("tf"), on_tf), arm(Pat::Wild, b.none())],
    )
}

fn unwrap_exit_expr(b: &ExprBuilder, ex_val: Expr) -> Expr {
    let on_fail = b.match_(
        b.app(b.global("Exit", "failure"), ex_val.clone()),
        [
            arm(
                b.some_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
            arm(
                Pat::Wild,
                b.app(b.global("Effect.Core", "die"), b.str_("cache exit")),
            ),
        ],
    );
    b.match_(
        b.app(b.global("Exit", "value"), ex_val),
        [arm(b.some_pat("v"), b.loc("v")), arm(Pat::Wild, on_fail)],
    )
}

fn find_key_expr(b: &ExprBuilder, ents_ref: Expr, key: Expr) -> Expr {
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

fn drop_key_expr(b: &ExprBuilder, list_expr: Expr, key: Expr) -> Expr {
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

fn evict_over_cap_expr(b: &ExprBuilder, list_expr: Expr, cap_expr: Expr) -> Expr {
    let over_cap = b.bin(
        BinOp::And,
        b.bin(BinOp::Ge, cap_expr.clone(), b.int(0)),
        b.bin(BinOp::Gt, b.loc("n"), cap_expr.clone()),
    );
    let dropped = b.app2(
        b.global("List.Core", "drop"),
        b.sub_i(b.loc("n"), cap_expr),
        list_expr.clone(),
    );
    b.block(
        [b.let_(
            "n",
            b.app(b.global("List.Core", "length"), list_expr.clone()),
        )],
        b.if_(over_cap, dropped, list_expr),
    )
}

fn put_entry_stmt(b: &ExprBuilder, key: Expr, entry: Expr) -> Stmt {
    let without_key = drop_key_expr(b, b.rget(b.loc("ents")), key.clone());
    let appended = b.app2(
        b.global("List.Core", "append"),
        without_key,
        b.list([b.tuple([key, entry])]),
    );
    b.rset(
        b.loc("ents"),
        evict_over_cap_expr(b, appended, b.loc("cap")),
    )
}

fn run_lookup_expr(b: &ExprBuilder) -> Expr {
    let run_lkp = b.task(b.app2(
        b.loc("env_run"),
        b.loc("env"),
        b.task(b.app(b.loc("lkp"), b.loc("k"))),
    ));
    let exp = compute_exp_expr(b, b.loc("ttl"), b.loc("ex"), b.loc("k"));
    b.block(
        [
            b.let_("wd", b.dmake()),
            b.let_("ent", b.rmake(b.tuple([b.none(), b.loc("wd")]))),
            put_entry_stmt(b, b.loc("k"), b.loc("ent")),
            b.let_("ex", b.app(b.global("Effect.Core", "exit"), run_lkp)),
            b.let_("exp", exp),
            b.let_("res", b.tuple([b.loc("ex"), b.loc("exp")])),
            b.rset(b.loc("ent"), b.tuple([b.some(b.loc("res")), b.loc("wd")])),
            b.dsucceed(b.loc("res"), b.loc("wd")),
        ],
        unwrap_exit_expr(b, b.loc("ex")),
    )
}

fn cache_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickMake", 1) => Some(lookup::make_expr(b)),
        ("flickMakeWith", 2) => Some(lookup::make_with_expr(b)),
        ("get", 2) => Some(lookup::get_expr(b)),
        ("getOption", 2) => Some(lookup::get_option_expr(b)),
        ("getSuccess", 2) => Some(lookup::get_success_expr(b)),
        ("set", 3) => Some(lookup::set_expr(b)),
        ("has", 2) => Some(manage::has_expr(b)),
        ("invalidate", 2) => Some(manage::invalidate_expr(b)),
        ("invalidateWhen", 3) => Some(manage::invalidate_when_expr(b)),
        ("refresh", 2) => Some(manage::refresh_expr(b)),
        ("invalidateAll", 1) => Some(manage::invalidate_all_expr(b)),
        ("size", 1) => Some(manage::size_expr(b)),
        ("flickKeys", 1) => Some(manage::keys_expr(b)),
        ("flickValues", 1) => Some(manage::values_expr(b)),
        ("flickEntries", 1) => Some(manage::entries_expr(b)),
        _ => None,
    }
}
