//! Audited `Request` pure IR adapter + batching engine (Stage 4D).
//!
//! Cooperative single-threaded batching: concurrent `Request.run` calls enqueue
//! into a resolver's window and wake a detached sorter fiber that groups entries
//! by `batchKey`, respects `preCheck`/`collectWhile`/`delay`, and runs `runAll`.

mod add;
mod batch;
mod cancel;
mod run;

use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::std_request_ops::resolver_template;
use super::{Code, Expr, Local, Lower, LowerError, Pat, Span};

impl Lower<'_> {
    pub(super) fn std_request_foreign(
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
        let b = ExprBuilder::new(span, "$rq");
        let expr = match from {
            "flick:Request" => request_template(&b, bind, value.mode, stages)?,
            "flick:RequestResolver" => resolver_template(&b, bind, value.mode, stages)?,
            _ => return None,
        };
        Some(self.lower_with_manual_scope(&expr, "$rq", block, span))
    }
}

// Request: `("$req", payload)`
// Entry: `(req, span_opt, comp_ref : Ref (Exit ->{} Unit), st_ref : Ref (completed : Bool, unint : Bool, batch_opt : Option BatchRef), done : Deferred)`
// BatchRef: `Ref (key, entries : List Entry, pending : List Entry, run_id : Int, started : Bool, fib_opt : Option Fiber)`
// Resolver: `(delay_fn, bkey_fn, eq_keys, pre_opt, coll_fn, run_fn, win_ref, pend_ref)`
pub(super) fn res_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&[
        "r_del", "r_key", "r_eq", "r_pre", "r_col", "r_run", "r_win", "r_pnd",
    ])
}

pub(super) fn entry_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["e_req", "e_sp", "e_cmp", "e_st", "e_dn"])
}

pub(super) fn batch_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["bk", "bents", "bpnd", "brid", "bst", "bfib"])
}

pub(super) fn same_entry_expr(b: &ExprBuilder, a: Expr, c: Expr) -> Expr {
    let cmp_c = b.match_(
        c,
        [arm(
            b.tup_pat(&["_6", "_7", "_8", "cs_", "_9"]),
            b.app2(b.global("Ref", "same"), b.loc("as_"), b.loc("cs_")),
        )],
    );
    b.match_(a, [arm(b.tup_pat(&["_1", "_2", "_3", "as_", "_5"]), cmp_c)])
}

fn filter_out_entry_by_st(b: &ExprBuilder, list: Expr, target_st: Expr) -> Expr {
    let pred = b.lam(
        &["x"],
        b.match_(
            b.loc("x"),
            [arm(
                b.tup_pat(&["_1", "_2", "_3", "xs_", "_5"]),
                b.not(b.app2(b.global("Ref", "same"), b.loc("xs_"), target_st)),
            )],
        ),
    );
    b.app2(b.global("List.Core", "filter"), pred, list)
}

fn has_entry_by_st(b: &ExprBuilder, list: Expr, target_st: Expr) -> Expr {
    let pred = b.lam(
        &["x"],
        b.match_(
            b.loc("x"),
            [arm(
                b.tup_pat(&["_1", "_2", "_3", "xs_", "_5"]),
                b.app2(b.global("Ref", "same"), b.loc("xs_"), target_st),
            )],
        ),
    );
    b.app2(b.global("List.Core", "any"), pred, list)
}

fn filter_out_entry(b: &ExprBuilder, list: Expr, target: Expr) -> Expr {
    let pred = b.lam(&["x"], b.not(same_entry_expr(b, b.loc("x"), target)));
    b.app2(b.global("List.Core", "filter"), pred, list)
}

fn has_entry(b: &ExprBuilder, list: Expr, target: Expr) -> Expr {
    let pred = b.lam(&["x"], same_entry_expr(b, b.loc("x"), target));
    b.app2(b.global("List.Core", "any"), pred, list)
}

fn filter_out_batch(b: &ExprBuilder, list: Expr, target: Expr) -> Expr {
    let pred = b.lam(
        &["x"],
        b.not(b.app2(b.global("Ref", "same"), b.loc("x"), target)),
    );
    b.app2(b.global("List.Core", "filter"), pred, list)
}

fn fork_detached(b: &ExprBuilder, task: Expr) -> Expr {
    b.block(
        [b.let_("fo", b.app(b.loc("owner_new"), b.unit()))],
        b.app3(
            b.global("Fiber", "spawn"),
            b.loc("fo"),
            b.lam(&["_f"], b.unit()),
            task,
        ),
    )
}

fn complete_entry_expr(b: &ExprBuilder, ex: Expr, ent: Expr) -> Expr {
    let call = b.app(b.rget(b.loc("cr")), ex);
    b.match_(ent, [arm(b.tup_pat(&["_1", "_2", "cr", "_4", "_5"]), call)])
}

fn complete_effect_template(b: &ExprBuilder) -> Expr {
    let has_fatal = b.bin(
        BinOp::Or,
        b.app(b.global("Exit", "has_interrupts"), b.loc("ex")),
        b.app(b.global("Cause", "has_dies"), b.loc("c")),
    );
    let fail_arm = arm(
        b.some_pat("c"),
        b.if_(
            has_fatal,
            b.app(b.global("Failure", "rethrow"), b.loc("c")),
            complete_entry_expr(b, b.loc("ex"), b.loc("e")),
        ),
    );
    let fallback_arm = arm(Pat::Wild, complete_entry_expr(b, b.loc("ex"), b.loc("e")));
    let body = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("ex")),
        [fail_arm, fallback_arm],
    );
    let ex_let = b.let_("ex", b.app(b.global("Effect.Core", "exit"), b.loc("t")));
    b.lam(&["t", "e"], b.block([ex_let], body))
}

fn request_template(b: &ExprBuilder, bind: &str, mode: ForeignMode, stages: usize) -> Option<Expr> {
    match (mode, bind, stages) {
        (ForeignMode::Pure, "flickOf", 1) => {
            Some(b.lam(&["p"], b.tuple([b.str_("$req"), b.loc("p")])))
        }
        (ForeignMode::Pure, "flickPayload", 1) => Some(b.lam(
            &["r"],
            b.match_(b.loc("r"), [arm(b.tup_pat(&["_t", "p"]), b.loc("p"))]),
        )),
        (ForeignMode::Pure, "flickRequest", 1) => {
            let payload = b.match_(b.loc("e_req"), [arm(b.tup_pat(&["_t", "p"]), b.loc("p"))]);
            Some(b.lam(&["e"], b.match_(b.loc("e"), [arm(entry_pat(b), payload)])))
        }
        (ForeignMode::Effect, "flickRun", 2) => Some(run::request_run_template(b)),
        (ForeignMode::Effect, "flickComplete", 2) => Some(b.lam(
            &["ex", "e"],
            complete_entry_expr(b, b.loc("ex"), b.loc("e")),
        )),
        (ForeignMode::Effect, "completeEffect", 2) => Some(complete_effect_template(b)),
        (ForeignMode::Effect, "succeed", 2) => Some(b.lam(
            &["v", "e"],
            complete_entry_expr(
                b,
                b.app(b.global("Exit", "succeed"), b.loc("v")),
                b.loc("e"),
            ),
        )),
        (ForeignMode::Effect, "fail", 2) => Some(b.lam(
            &["err", "e"],
            complete_entry_expr(b, b.app(b.global("Exit", "fail"), b.loc("err")), b.loc("e")),
        )),
        (ForeignMode::Effect, "flickFailCause", 2) => Some(b.lam(
            &["c", "e"],
            complete_entry_expr(
                b,
                b.app(b.global("Exit", "fail_cause"), b.loc("c")),
                b.loc("e"),
            ),
        )),
        _ => None,
    }
}
