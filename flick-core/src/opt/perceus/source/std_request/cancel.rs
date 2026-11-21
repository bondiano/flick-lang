//! Cancellation and entry removal for `Request.run`.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};
use super::{batch_pat, complete_entry_expr, filter_out_batch, filter_out_entry, has_entry};

fn remove_from_batch(b: &ExprBuilder) -> Expr {
    let skip = b.bin(
        BinOp::Or,
        b.loc("bst"),
        b.not(has_entry(b, b.loc("bents"), b.loc("ent"))),
    );
    let nents = b.let_("nents", filter_out_entry(b, b.loc("bents"), b.loc("ent")));
    let npnd = b.let_("npnd", filter_out_entry(b, b.loc("bpnd"), b.loc("ent")));
    let empty = b.let_(
        "empty",
        b.bin(
            BinOp::Eq,
            b.app(b.global("List.Core", "length"), b.loc("nents")),
            b.int(0),
        ),
    );
    let updated_batch = b.tuple([
        b.loc("bk"),
        b.loc("nents"),
        b.loc("npnd"),
        b.loc("brid"),
        b.if_(b.loc("empty"), b.bool_(true), b.loc("bst")),
        b.if_(b.loc("empty"), b.none(), b.loc("bfib")),
    ]);
    let drop_if_empty = b.if_(
        b.loc("empty"),
        b.block(
            [b.rset(
                b.loc("r_pnd"),
                filter_out_batch(b, b.rget(b.loc("r_pnd")), b.loc("br")),
            )],
            b.unit(),
        ),
        b.unit(),
    );
    let result = b.tuple([
        b.bool_(true),
        b.if_(b.loc("empty"), b.loc("bfib"), b.none()),
    ]);
    let do_remove = b.block(
        [
            nents,
            npnd,
            empty,
            b.rset(b.loc("br"), updated_batch),
            Stmt::Expr(drop_if_empty),
        ],
        result,
    );
    let body = b.if_(skip, b.tuple([b.bool_(false), b.none()]), do_remove);
    b.match_(b.rget(b.loc("br")), [arm(batch_pat(b), body)])
}

fn remove_entry_expr(b: &ExprBuilder) -> Expr {
    let not_removed = b.tuple([b.bool_(false), b.none()]);
    let in_win = b.block(
        [b.rset(
            b.loc("r_win"),
            b.tuple([
                filter_out_entry(b, b.loc("wents"), b.loc("ent")),
                b.loc("wsrt"),
            ]),
        )],
        b.tuple([b.bool_(true), b.none()]),
    );
    let check_bopt = b.match_(
        b.loc("bopt"),
        [
            arm(b.some_pat("br"), remove_from_batch(b)),
            arm(Pat::Wild, not_removed.clone()),
        ],
    );
    let check_st = b.match_(
        b.rget(b.loc("st_ref")),
        [arm(
            b.tup_pat(&["_cmp", "un", "bopt"]),
            b.if_(b.loc("un"), not_removed, check_bopt),
        )],
    );
    let check_win = b.if_(has_entry(b, b.loc("wents"), b.loc("ent")), in_win, check_st);
    b.no_preempt(b.match_(
        b.rget(b.loc("r_win")),
        [arm(b.tup_pat(&["wents", "wsrt"]), check_win)],
    ))
}

pub(super) fn on_cancel_expr(b: &ExprBuilder) -> Expr {
    let interrupt_ent = b.if_(
        b.loc("do_int"),
        complete_entry_expr(
            b,
            b.app(b.global("Exit", "interrupt"), b.none()),
            b.loc("ent"),
        ),
        b.unit(),
    );
    let stop_batch = b.match_(
        b.loc("stop_fib"),
        [
            arm(
                b.some_pat("sf"),
                b.app(b.global("Fiber", "interrupt"), b.loc("sf")),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let handle = b.block([Stmt::Expr(interrupt_ent)], stop_batch);
    let body = b.match_(
        remove_entry_expr(b),
        [arm(b.tup_pat(&["do_int", "stop_fib"]), handle)],
    );
    b.app(
        b.global("Effect.Concurrency", "uninterruptible_mask"),
        b.lam(&["_rst"], body),
    )
}
