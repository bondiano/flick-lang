//! `RcRef` pure IR templates (`flickMake`, `get`, `invalidate`).

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};
use super::{rc_entry_release, rc_retire_entry};

fn rc_ref_make(b: &ExprBuilder) -> Expr {
    let retire_cur = b.match_(
        b.rget(b.loc("cur")),
        [
            arm(
                b.some_pat("e"),
                b.block(
                    [b.rset(b.loc("cur"), b.none())],
                    rc_retire_entry(b, b.loc("e")),
                ),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let tuple = b.tuple([
        b.field(b.loc("opts"), "acquire"),
        b.field(b.loc("opts"), "idleTimeToLive"),
        b.loc("env"),
        b.loc("pown"),
        b.loc("cur"),
        b.loc("lk"),
    ]);
    b.lam(
        &["opts"],
        b.block(
            [
                b.let_("env", b.app(b.loc("env_capture"), b.unit())),
                b.let_("pown", b.app(b.global("Scope", "current"), b.unit())),
                b.let_("cur", b.rmake(b.none())),
                b.let_("lk", b.app(b.global("Semaphore", "make"), b.int(1))),
                Stmt::Expr(b.app(b.global("Scope", "add_finalizer"), b.task(retire_cur))),
            ],
            tuple,
        ),
    )
}

fn rc_ref_acquire_new(b: &ExprBuilder, on_evict: Expr) -> Expr {
    let run_acq = b.task(b.app2(b.loc("env_run"), b.loc("env"), b.loc("acq")));
    let run_in_owner = b.task(b.app2(b.loc("owner_run"), b.loc("ho"), run_acq));
    let new_entry = b.rmake(b.tuple([
        b.loc("hv"),
        b.loc("ho"),
        b.int(1),
        b.bool_(false),
        b.bool_(false),
        b.int(0),
    ]));
    let rel = rc_entry_release(b, b.loc("ent"), b.loc("idle"), b.loc("pown"), on_evict);
    let on_ok = b.block(
        [
            b.let_("ent", new_entry),
            b.rset(b.loc("cur"), b.some(b.loc("ent"))),
            Stmt::Expr(b.app(b.global("Scope", "add_finalizer"), b.task(rel))),
        ],
        b.loc("hv"),
    );
    let close_and_rethrow = b.block(
        [Stmt::Expr(b.app2(
            b.loc("owner_close"),
            b.loc("ho"),
            b.some(b.loc("hc")),
        ))],
        b.app(b.global("Failure", "rethrow"), b.loc("hc")),
    );
    let on_fail = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("hex")),
        [
            arm(b.some_pat("hc"), close_and_rethrow),
            arm(
                Pat::Wild,
                b.app(b.global("Effect.Core", "die"), b.str_("rcref")),
            ),
        ],
    );
    let tail = b.match_(
        b.app(b.global("Exit", "value"), b.loc("hex")),
        [arm(b.some_pat("hv"), on_ok), arm(Pat::Wild, on_fail)],
    );
    b.block(
        [
            b.let_("ho", b.app(b.loc("owner_new"), b.unit())),
            b.let_("hex", b.app(b.global("Effect.Core", "exit"), run_in_owner)),
        ],
        tail,
    )
}

fn rc_ref_get(b: &ExprBuilder) -> Expr {
    let clear_if_same = b.if_(
        b.app2(b.global("Ref", "same"), b.loc("ce"), b.loc("ent")),
        b.block([b.rset(b.loc("cur"), b.none())], b.unit()),
        b.unit(),
    );
    let on_evict = b.match_(
        b.rget(b.loc("cur")),
        [
            arm(b.some_pat("ce"), clear_if_same),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let acquire_new = rc_ref_acquire_new(b, on_evict.clone());
    let bump = b.tuple([
        b.loc("ev"),
        b.loc("eo"),
        b.add_i(b.loc("h"), b.int(1)),
        b.bool_(false),
        b.bool_(false),
        b.add_i(b.loc("ep"), b.int(1)),
    ]);
    let rel = rc_entry_release(b, b.loc("ent"), b.loc("idle"), b.loc("pown"), on_evict);
    let reuse = b.block(
        [
            b.rset(b.loc("ent"), bump),
            Stmt::Expr(b.app(b.global("Scope", "add_finalizer"), b.task(rel))),
        ],
        b.loc("ev"),
    );
    let on_existing = b.match_(
        b.rget(b.loc("ent")),
        [arm(
            b.tup_pat(&["ev", "eo", "h", "ret", "cls", "ep"]),
            b.if_(
                b.bin(BinOp::Or, b.loc("ret"), b.loc("cls")),
                acquire_new.clone(),
                reuse,
            ),
        )],
    );
    let body = b.match_(
        b.rget(b.loc("cur")),
        [
            arm(b.some_pat("ent"), on_existing),
            arm(Pat::Wild, acquire_new),
        ],
    );
    b.lam(
        &["rc"],
        b.match_(
            b.loc("rc"),
            [arm(
                b.tup_pat(&["acq", "idle", "env", "pown", "cur", "lk"]),
                b.app2(
                    b.global("Semaphore", "with_permit"),
                    b.loc("lk"),
                    b.task(body),
                ),
            )],
        ),
    )
}

fn rc_ref_invalidate(b: &ExprBuilder) -> Expr {
    let body = b.match_(
        b.rget(b.loc("cur")),
        [
            arm(
                b.some_pat("ent"),
                b.block(
                    [b.rset(b.loc("cur"), b.none())],
                    rc_retire_entry(b, b.loc("ent")),
                ),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    );
    b.lam(
        &["rc"],
        b.match_(
            b.loc("rc"),
            [arm(
                b.tup_pat(&["_acq", "_idle", "_env", "_pown", "cur", "lk"]),
                b.app2(
                    b.global("Semaphore", "with_permit"),
                    b.loc("lk"),
                    b.task(body),
                ),
            )],
        ),
    )
}

pub(super) fn rc_ref_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickMake", 1) => Some(rc_ref_make(b)),
        ("get", 1) => Some(rc_ref_get(b)),
        ("invalidate", 1) => Some(rc_ref_invalidate(b)),
        _ => None,
    }
}
