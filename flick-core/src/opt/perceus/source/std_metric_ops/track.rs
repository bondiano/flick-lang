//! `Effect.Observe` `track*` adapters for `Metric`.

use flick_ir::{Arith, BinOp, Num};

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};

fn unwrap_exit(b: &ExprBuilder, ex: Expr) -> Expr {
    let val_scrut = b.app(b.global("Exit", "value"), ex.clone());
    let on_fail = b.match_(
        b.app(b.global("Exit", "failure"), ex),
        [
            arm(
                b.some_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
            arm(
                Pat::Wild,
                b.app(b.global("Effect.Core", "die"), b.str_("track exit")),
            ),
        ],
    );
    b.match_(
        val_scrut,
        [arm(b.some_pat("v"), b.loc("v")), arm(Pat::Wild, on_fail)],
    )
}

fn track_failure_part(b: &ExprBuilder, finder: &str, ok_result: bool, var_name: &str) -> Expr {
    let update = b.app2(
        b.global("Metric", "update"),
        b.app(b.loc("f"), b.loc(var_name)),
        b.loc("m"),
    );
    let pat = if ok_result {
        b.ok_pat(var_name)
    } else {
        b.some_pat(var_name)
    };
    let on_cause = b.match_(
        b.app(b.global("Cause", finder), b.loc("c")),
        [arm(pat, update), arm(Pat::Wild, b.unit())],
    );
    let on_fail = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("ex")),
        [arm(b.some_pat("c"), on_cause), arm(Pat::Wild, b.unit())],
    );
    b.lam(
        &["m", "f", "t"],
        b.block(
            [
                b.let_("ex", b.app(b.global("Effect.Core", "exit"), b.loc("t"))),
                Stmt::Expr(on_fail),
            ],
            unwrap_exit(b, b.loc("ex")),
        ),
    )
}

fn track_duration_template(b: &ExprBuilder) -> Expr {
    let elapsed = b.bin(
        BinOp::Arith(Arith::Sub, Num::BigInt),
        b.loc("t1"),
        b.loc("t0"),
    );
    let dur = b.match_(
        b.app(b.global("Duration", "nanos"), elapsed),
        [
            arm(b.ok_pat("d"), b.loc("d")),
            arm(Pat::Wild, b.global("Duration", "zero")),
        ],
    );
    b.lam(
        &["m", "f", "t"],
        b.block(
            [
                b.let_(
                    "t0",
                    b.app(b.global("Clock", "monotonic_time_nanos"), b.unit()),
                ),
                b.let_("ex", b.app(b.global("Effect.Core", "exit"), b.loc("t"))),
                b.let_(
                    "t1",
                    b.app(b.global("Clock", "monotonic_time_nanos"), b.unit()),
                ),
                b.let_("dur", dur),
                Stmt::Expr(b.app2(
                    b.global("Metric", "update"),
                    b.app(b.loc("f"), b.loc("dur")),
                    b.loc("m"),
                )),
            ],
            unwrap_exit(b, b.loc("ex")),
        ),
    )
}

pub(in super::super) fn effect_track_template(
    b: &ExprBuilder,
    bind: &str,
    stages: usize,
) -> Option<Expr> {
    if stages != 3 {
        return None;
    }
    match bind {
        "flickTrack" => Some(b.lam(
            &["m", "f", "t"],
            b.block(
                [
                    b.let_("ex", b.app(b.global("Effect.Core", "exit"), b.loc("t"))),
                    Stmt::Expr(b.app2(
                        b.global("Metric", "update"),
                        b.app(b.loc("f"), b.loc("ex")),
                        b.loc("m"),
                    )),
                ],
                unwrap_exit(b, b.loc("ex")),
            ),
        )),
        "trackSuccesses" => Some(b.lam(
            &["m", "f", "t"],
            b.block(
                [
                    b.let_("v", b.await_(b.loc("t"))),
                    Stmt::Expr(b.app2(
                        b.global("Metric", "update"),
                        b.app(b.loc("f"), b.loc("v")),
                        b.loc("m"),
                    )),
                ],
                b.loc("v"),
            ),
        )),
        "trackErrors" => Some(track_failure_part(b, "find_error_option", false, "err")),
        "flickTrackDefects" => Some(track_failure_part(b, "find_defect", true, "def")),
        "trackDuration" => Some(track_duration_template(b)),
        _ => None,
    }
}
