//! `Stream.Combine` `flatMapPar`, `repeatElements`, and `timeoutOrElse` helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, ExprKind, Pat, Stmt};

pub(super) fn st_flat_map_par_helper(b: &ExprBuilder) -> Expr {
    let par_opts = b.at(ExprKind::Record(vec![
        ("bufferSize".into(), b.field(b.loc("opts"), "bufferSize")),
        ("concurrency".into(), b.field(b.loc("opts"), "concurrency")),
        ("switch".into(), b.bool_(false)),
    ]));
    let mapped = b.app2(
        rt(b, "Helper.ch_map"),
        b.loc("f"),
        b.app(rt(b, "Helper.ch_flatten_array"), b.loc("self")),
    );
    let par_call = b.app2(rt(b, "Helper.ch_merge_all"), par_opts, mapped);
    let seq_call = b.app2(
        rt(b, "Helper.ch_flat_map"),
        b.loc("f"),
        b.app(rt(b, "Helper.ch_flatten_array"), b.loc("self")),
    );
    let on_conc = b.if_(
        b.bin(BinOp::Le, b.loc("n"), b.int(1)),
        seq_call,
        par_call.clone(),
    );
    b.lam(
        &["opts", "f", "self"],
        b.match_(
            b.field(b.loc("opts"), "concurrency"),
            [arm(b.some_pat("n"), on_conc), arm(Pat::Wild, par_call)],
        ),
    )
}

pub(super) fn st_repeat_elements_helper(b: &ExprBuilder) -> Expr {
    let step_decision = b.if_(
        b.loc("dn"),
        b.block([b.rset(b.loc("rep"), b.none())], b.unit()),
        b.break_("sre", b.ok(b.list([b.loc("el")]))),
    );
    let step_cur = b.match_(
        b.app2(rt(b, "Helper.ch_sched_step"), b.loc("step"), b.loc("el")),
        [arm(pair_pat(b, "_o", "dn"), step_decision)],
    );
    let on_rep = b.match_(b.loc("pair"), [arm(pair_pat(b, "el", "step"), step_cur)]);
    let start_rep = b.block(
        [b.rset(
            b.loc("rep"),
            b.some(b.tuple([b.loc("el"), b.app(b.loc("sched"), b.unit())])),
        )],
        b.break_("sre", b.ok(b.list([b.loc("el")]))),
    );
    let pull_fresh = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("el"), start_rep),
            arm(b.err_pat("d"), b.break_("sre", b.err(b.loc("d")))),
        ],
    );
    let loop_body = b.match_(
        b.rget(b.loc("rep")),
        [arm(b.some_pat("pair"), on_rep), arm(Pat::Wild, pull_fresh)],
    );
    let init = [
        b.let_(
            "pull",
            b.app3(
                rt(b, "Helper.ch_flatten_array"),
                b.loc("self"),
                b.loc("up"),
                b.loc("own"),
            ),
        ),
        b.let_("rep", b.rmake(b.none())),
    ];
    b.lam(
        &["sched", "self", "up", "own"],
        b.block(init, b.lam(&["_u"], b.loop_ctrl("sre", loop_body))),
    )
}

pub(super) fn st_timeout_or_else_helper(b: &ExprBuilder) -> Expr {
    let switch_fb = b.block(
        [
            Stmt::Expr(b.app(b.loc("sclose"), b.none())),
            b.let_("p", b.app2(b.loc("fb"), b.loc("up"), b.loc("own"))),
            b.rset(b.loc("fbp"), b.some(b.loc("p"))),
        ],
        b.app(b.loc("p"), b.unit()),
    );
    let timed_pull = b.app3(
        rt(b, "Async.TimeoutOrElse"),
        b.loc("dur"),
        b.task(b.none()),
        b.task(b.some(b.app(b.loc("pull"), b.unit()))),
    );
    let check_timeout = b.match_(
        timed_pull,
        [
            arm(b.some_pat("res"), b.loc("res")),
            arm(Pat::Wild, switch_fb),
        ],
    );
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.rget(b.loc("fbp")),
            [
                arm(b.some_pat("p"), b.app(b.loc("p"), b.unit())),
                arm(Pat::Wild, check_timeout),
            ],
        ),
    );
    let scope_body = b.block(
        [
            b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("sown"))),
            b.let_("fbp", b.rmake(b.none())),
        ],
        pull,
    );
    let open = b.lam(
        &["up", "own"],
        b.match_(
            b.app(rt(b, "Helper.ch_fork_scope"), b.loc("own")),
            [arm(pair_pat(b, "sown", "sclose"), scope_body)],
        ),
    );
    b.lam(
        &["dur", "fb", "self"],
        b.if_(
            b.not(b.app(rt(b, "Duration.IsPositive"), b.loc("dur"))),
            b.loc("fb"),
            open,
        ),
    )
}
