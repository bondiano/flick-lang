//! `Resource.{manual,use,refresh}` and `Effect.resource_auto` pure IR templates.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};
use super::{acquire_holder_expr, lease_holder_expr, retire_holder_expr};

fn resource_manual(b: &ExprBuilder) -> Expr {
    let acq = acquire_holder_expr(b, b.app(b.loc("restore"), b.loc("acq")), Some(b.loc("env")));
    let on_fail = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("ex0")),
        [
            arm(b.some_pat("c"), b.err(b.loc("c"))),
            arm(
                Pat::Wild,
                b.err(b.app(b.global("Cause", "die"), b.str_("resource init"))),
            ),
        ],
    );
    let init = b.match_(
        b.app(b.global("Exit", "value"), b.loc("ex0")),
        [
            arm(b.some_pat("h"), b.ok(b.loc("h"))),
            arm(Pat::Wild, on_fail),
        ],
    );
    let fin_body = b.match_(
        b.rget(b.loc("cur")),
        [
            arm(b.ok_pat("h"), retire_holder_expr(b, b.loc("h"))),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let body = b.block(
        [
            b.let_("env", b.app(b.loc("env_capture"), b.unit())),
            b.let_("ex0", b.app(b.global("Effect.Core", "exit"), b.task(acq))),
            b.let_("init", init),
            b.let_("cur", b.rmake(b.loc("init"))),
            b.let_("lk", b.app(b.global("Semaphore", "make"), b.int(1))),
            Stmt::Expr(b.app(b.global("Scope", "add_finalizer"), b.task(fin_body))),
        ],
        b.tuple([b.loc("cur"), b.loc("acq"), b.loc("env"), b.loc("lk")]),
    );
    b.lam(
        &["acq"],
        b.app(
            b.global("Effect.Concurrency", "uninterruptible_mask"),
            b.lam(&["restore"], body),
        ),
    )
}

fn resource_refresh(b: &ExprBuilder) -> Expr {
    let acq = acquire_holder_expr(b, b.app(b.loc("restore"), b.loc("acq")), Some(b.loc("env")));
    let retire_prev = b.match_(
        b.loc("prev"),
        [
            arm(b.ok_pat("ph"), retire_holder_expr(b, b.loc("ph"))),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let on_ok = b.block(
        [
            b.let_("prev", b.rget(b.loc("cur"))),
            b.rset(b.loc("cur"), b.ok(b.loc("nxt"))),
        ],
        retire_prev,
    );
    let store_err_if_unset = b.match_(
        b.rget(b.loc("cur")),
        [
            arm(b.ok_pat("_ph"), b.unit()),
            arm(
                Pat::Wild,
                b.block([b.rset(b.loc("cur"), b.err(b.loc("c")))], b.unit()),
            ),
        ],
    );
    let on_cause = b.block(
        [Stmt::Expr(store_err_if_unset)],
        b.app(b.global("Failure", "rethrow"), b.loc("c")),
    );
    let on_fail = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("ex")),
        [arm(b.some_pat("c"), on_cause), arm(Pat::Wild, b.unit())],
    );
    let body = b.block(
        [b.let_("ex", b.app(b.global("Effect.Core", "exit"), b.task(acq)))],
        b.match_(
            b.app(b.global("Exit", "value"), b.loc("ex")),
            [arm(b.some_pat("nxt"), on_ok), arm(Pat::Wild, on_fail)],
        ),
    );
    let masked = b.app(
        b.global("Effect.Concurrency", "uninterruptible_mask"),
        b.lam(&["restore"], body),
    );
    let with_lock = b.app2(
        b.global("Semaphore", "with_permit"),
        b.loc("lk"),
        b.task(masked),
    );
    b.lam(
        &["res"],
        b.match_(
            b.loc("res"),
            [arm(b.tup_pat(&["cur", "acq", "env", "lk"]), with_lock)],
        ),
    )
}

pub(super) fn resource_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickManual", 1) => Some(resource_manual(b)),
        ("flickUse", 2) => {
            let get_cur = b.match_(
                b.rget(b.loc("cur")),
                [
                    arm(b.ok_pat("h"), b.loc("h")),
                    arm(
                        b.err_pat("c"),
                        b.app(b.global("Failure", "rethrow"), b.loc("c")),
                    ),
                ],
            );
            Some(b.lam(
                &["res", "f"],
                b.match_(
                    b.loc("res"),
                    [arm(
                        b.tup_pat(&["cur", "_acq", "_env", "_lk"]),
                        lease_holder_expr(b, get_cur, b.loc("f")),
                    )],
                ),
            ))
        }
        ("flickRefresh", 1) => Some(resource_refresh(b)),
        _ => None,
    }
}

pub(super) fn resource_auto_template(b: &ExprBuilder) -> Expr {
    let repeat_refresh = b.app2(
        b.global("Effect.Core", "repeat"),
        b.loc("sched"),
        b.task(b.app(b.global("Resource", "refresh"), b.loc("r"))),
    );
    let spawn = b.app3(
        b.global("Fiber", "spawn"),
        b.loc("ow"),
        b.lam(&["_c"], b.unit()),
        b.task(b.block([b.let_("_u", repeat_refresh)], b.unit())),
    );
    b.lam(
        &["sched", "acq"],
        b.block(
            [
                b.let_("r", b.app(b.global("Resource", "manual"), b.loc("acq"))),
                b.let_("ow", b.app(b.global("Scope", "current"), b.unit())),
                b.let_("_f", spawn),
            ],
            b.loc("r"),
        ),
    )
}
