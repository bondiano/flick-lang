//! `ScopedRef` pure IR templates (`flickFromAcquire`, `flickMake`, `flickUse`, `flickSet`).

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Stmt};
use super::{acquire_holder_expr, lease_holder_expr, plain_holder_expr, retire_holder_expr};

fn register_scoped_ref(b: &ExprBuilder, init_holder: Expr) -> Expr {
    let fin = b.app(
        b.global("Scope", "add_finalizer"),
        b.task(retire_holder_expr(b, b.rget(b.loc("cur")))),
    );
    b.block(
        [
            b.let_("h", init_holder),
            b.let_("cur", b.rmake(b.loc("h"))),
            b.let_("lk", b.app(b.global("Semaphore", "make"), b.int(1))),
            Stmt::Expr(fin),
        ],
        b.tuple([b.loc("cur"), b.loc("lk")]),
    )
}

fn scoped_ref_set(b: &ExprBuilder) -> Expr {
    let nxt = acquire_holder_expr(b, b.app(b.loc("restore"), b.loc("acq")), None);
    let swap = b.block(
        [
            b.let_("nxt", nxt),
            b.let_("prev", b.rget(b.loc("cur"))),
            b.rset(b.loc("cur"), b.loc("nxt")),
        ],
        retire_holder_expr(b, b.loc("prev")),
    );
    let masked = b.app(
        b.global("Effect.Concurrency", "uninterruptible_mask"),
        b.lam(&["restore"], swap),
    );
    let with_lock = b.app2(
        b.global("Semaphore", "with_permit"),
        b.loc("lk"),
        b.task(masked),
    );
    b.lam(
        &["acq", "ref"],
        b.match_(b.loc("ref"), [arm(b.tup_pat(&["cur", "lk"]), with_lock)]),
    )
}

pub(super) fn scoped_ref_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickFromAcquire", 1) => {
            let acq = acquire_holder_expr(b, b.app(b.loc("restore"), b.loc("acq")), None);
            Some(b.lam(
                &["acq"],
                b.app(
                    b.global("Effect.Concurrency", "uninterruptible_mask"),
                    b.lam(&["restore"], register_scoped_ref(b, acq)),
                ),
            ))
        }
        ("flickMake", 1) => Some(b.lam(
            &["f"],
            register_scoped_ref(b, plain_holder_expr(b, b.app(b.loc("f"), b.unit()))),
        )),
        ("flickUse", 2) => Some(b.lam(
            &["ref", "f"],
            b.match_(
                b.loc("ref"),
                [arm(
                    b.tup_pat(&["cur", "_lk"]),
                    lease_holder_expr(b, b.rget(b.loc("cur")), b.loc("f")),
                )],
            ),
        )),
        ("flickSet", 2) => Some(scoped_ref_set(b)),
        _ => None,
    }
}
