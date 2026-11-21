//! Entry sorting and batch-full flush logic for `Request`.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};
use super::batch::fork_batch_expr;
use super::{batch_pat, entry_pat, filter_out_batch};

fn find_or_create_batch(b: &ExprBuilder) -> Expr {
    let key_matches = b.lam(
        &["br"],
        b.match_(
            b.rget(b.loc("br")),
            [arm(
                batch_pat(b),
                b.bin(BinOp::Eq, b.loc("bk"), b.loc("ek")),
            )],
        ),
    );
    let hit_let = b.let_(
        "hit",
        b.app2(
            b.global("List.Core", "find"),
            key_matches,
            b.rget(b.loc("r_pnd")),
        ),
    );
    let empty_batch = b.tuple([
        b.loc("ek"),
        b.list(vec![]),
        b.list(vec![]),
        b.int(0),
        b.bool_(false),
        b.none(),
    ]);
    let appended = b.app2(
        b.global("List.Core", "append"),
        b.rget(b.loc("r_pnd")),
        b.list([b.loc("nb")]),
    );
    let make_new = b.block(
        [
            b.let_("nb", b.rmake(empty_batch)),
            b.rset(b.loc("r_pnd"), appended),
        ],
        b.tuple([b.loc("nb"), b.bool_(true)]),
    );
    let pick = b.match_(
        b.loc("hit"),
        [
            arm(b.some_pat("br"), b.tuple([b.loc("br"), b.bool_(false)])),
            arm(Pat::Wild, make_new),
        ],
    );
    b.no_preempt(b.block([hit_let], pick))
}

fn append_entry_to_batch(b: &ExprBuilder) -> Expr {
    let nents = b.app2(
        b.global("List.Core", "append"),
        b.loc("bents"),
        b.list([b.loc("ae")]),
    );
    let npnd = b.app2(
        b.global("List.Core", "append"),
        b.loc("bpnd"),
        b.list([b.loc("ae")]),
    );
    let updated = b.tuple([
        b.loc("bk"),
        b.loc("nents"),
        b.loc("npnd"),
        b.loc("brid"),
        b.loc("bst"),
        b.loc("bfib"),
    ]);
    let update_batch = b.block(
        [
            b.let_("nents", nents),
            b.let_("npnd", npnd),
            b.rset(b.loc("br"), updated),
        ],
        b.loc("nents"),
    );
    let attach_batch = b.block(
        [b.rset(
            b.loc("e_st"),
            b.tuple([b.loc("cmp"), b.loc("un"), b.some(b.loc("br"))]),
        )],
        b.match_(b.rget(b.loc("br")), [arm(batch_pat(b), update_batch)]),
    );
    let on_state = b.match_(
        b.rget(b.loc("e_st")),
        [arm(b.tup_pat(&["cmp", "un", "_bo"]), attach_batch)],
    );
    b.no_preempt(b.match_(b.loc("ae"), [arm(entry_pat(b), on_state)]))
}

fn flush_full_batch(b: &ExprBuilder) -> Expr {
    let take_prev_fib = b.no_preempt(b.block(
        [b.rset(
            b.loc("r_pnd"),
            filter_out_batch(b, b.rget(b.loc("r_pnd")), b.loc("br")),
        )],
        b.match_(b.rget(b.loc("br")), [arm(batch_pat(b), b.loc("bfib"))]),
    ));
    let push_stop = b.block(
        [b.rset(
            b.loc("to_stop"),
            b.app2(
                b.global("List.Core", "append"),
                b.rget(b.loc("to_stop")),
                b.list([b.loc("pf")]),
            ),
        )],
        b.unit(),
    );
    let queue_stop = b.match_(
        b.loc("prev_fib"),
        [arm(b.some_pat("pf"), push_stop), arm(Pat::Wild, b.unit())],
    );
    let run_now = fork_batch_expr(b, b.loc("br"), b.lam(&["_u"], b.unit()));
    b.block(
        [b.let_("prev_fib", take_prev_fib), Stmt::Expr(run_now)],
        queue_stop,
    )
}

pub(super) fn add_entry_expr(b: &ExprBuilder) -> Expr {
    let maybe_fork = b.if_(
        b.loc("is_new"),
        fork_batch_expr(b, b.loc("br"), b.loc("r_del")),
        b.unit(),
    );
    let maybe_flush = b.if_(
        b.app(b.loc("r_col"), b.loc("cur_ents")),
        b.unit(),
        flush_full_batch(b),
    );
    let handle_batch = b.block(
        [
            Stmt::Expr(maybe_fork),
            b.let_("cur_ents", append_entry_to_batch(b)),
        ],
        maybe_flush,
    );
    let body = b.block(
        [
            b.let_("ek", b.app(b.loc("r_key"), b.loc("ae"))),
            b.let_("bref", find_or_create_batch(b)),
        ],
        b.match_(
            b.loc("bref"),
            [arm(b.tup_pat(&["br", "is_new"]), handle_batch)],
        ),
    );
    let pre_arm = arm(
        b.some_pat("pf"),
        b.if_(b.app(b.loc("pf"), b.loc("ae")), body.clone(), b.unit()),
    );
    b.lam(
        &["ae"],
        b.match_(b.loc("r_pre"), [pre_arm, arm(Pat::Wild, body)]),
    )
}
