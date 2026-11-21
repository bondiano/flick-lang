//! Source-constructor `Channel.Core` IR helpers (lists, effects, queues, pubsub, schedules).

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, ExprKind, Pat, Stmt};

pub(super) fn ch_from_list_helper(b: &ExprBuilder) -> Expr {
    let take_elem = b.block(
        [b.rset(b.loc("idx"), b.add_i(b.loc("i"), b.int(1)))],
        b.ok(b.app2(rt(b, "Helper.list_at"), b.loc("i"), b.loc("xs"))),
    );
    let pull = b.lam(
        &["_u"],
        b.block(
            [b.let_("i", b.rget(b.loc("idx")))],
            b.if_(
                b.bin(BinOp::Ge, b.loc("i"), b.loc("len")),
                b.err(b.unit()),
                take_elem,
            ),
        ),
    );
    b.lam(
        &["xs", "_up", "_own"],
        b.block(
            [
                b.let_("len", b.app(b.global("List.Core", "length"), b.loc("xs"))),
                b.let_("idx", b.rmake(b.int(0))),
            ],
            pull,
        ),
    )
}

pub(super) fn ch_from_list_chunked_helper(b: &ExprBuilder) -> Expr {
    let take_chunk = b.block(
        [
            b.let_("end", b.add_i(b.loc("i"), b.loc("step"))),
            b.rset(b.loc("idx"), b.loc("end")),
            b.let_(
                "chunk",
                b.app3(
                    b.global("List.Core", "slice"),
                    b.loc("i"),
                    b.loc("end"),
                    b.loc("xs"),
                ),
            ),
        ],
        b.ok(b.loc("chunk")),
    );
    let pull = b.lam(
        &["_u"],
        b.block(
            [b.let_("i", b.rget(b.loc("idx")))],
            b.if_(
                b.bin(BinOp::Ge, b.loc("i"), b.loc("len")),
                b.err(b.unit()),
                take_chunk,
            ),
        ),
    );
    let step = b.if_(
        b.bin(BinOp::Le, b.loc("sz"), b.int(0)),
        b.int(1),
        b.loc("sz"),
    );
    b.lam(
        &["sz", "xs", "_up", "_own"],
        b.block(
            [
                b.let_("step", step),
                b.let_("len", b.app(b.global("List.Core", "length"), b.loc("xs"))),
                b.let_("idx", b.rmake(b.int(0))),
            ],
            pull,
        ),
    )
}

pub(super) fn ch_from_effect_helper(b: &ExprBuilder) -> Expr {
    let run_once = b.block(
        [b.rset(b.loc("ran"), b.bool_(true))],
        b.ok(b.await_(b.loc("t"))),
    );
    let pull = b.lam(
        &["_u"],
        b.if_(b.rget(b.loc("ran")), b.err(b.unit()), run_once),
    );
    b.lam(
        &["t", "_up", "_own"],
        b.block([b.let_("ran", b.rmake(b.bool_(false)))], pull),
    )
}

pub(super) fn ch_from_queue_helper(b: &ExprBuilder) -> Expr {
    let take_next = b.if_(
        b.loc("arr"),
        b.app(b.global("Queue", "take_all"), b.loc("q")),
        b.app(b.global("Queue", "take"), b.loc("q")),
    );
    let pull = b.lam(
        &["_u"],
        b.match_(
            take_next,
            [
                arm(b.some_pat("v"), b.ok(b.loc("v"))),
                arm(Pat::Wild, b.err(b.unit())),
            ],
        ),
    );
    b.lam(&["arr", "q", "_up", "_own"], pull)
}

pub(super) fn ch_from_sub_helper(b: &ExprBuilder) -> Expr {
    // If hub shuts down (`PubSub.take` interrupts), turn interrupt-only cause into `Err ()`!
    let on_cause = b.lam(
        &["c"],
        b.if_(
            b.app(b.global("Cause", "has_interrupts_only"), b.loc("c")),
            b.err(b.unit()),
            b.app(b.global("Failure", "rethrow"), b.loc("c")),
        ),
    );
    let take_next = b.if_(
        b.loc("arr"),
        b.app(b.global("PubSub", "take_all"), b.loc("sub")),
        b.app(b.global("PubSub", "take"), b.loc("sub")),
    );
    let pull = b.lam(
        &["_u"],
        b.app2(
            b.global("Effect.Errors", "catch_cause"),
            on_cause,
            b.task(b.ok(take_next)),
        ),
    );
    b.lam(&["arr", "sub", "_up", "_own"], pull)
}

pub(super) fn ch_from_pubsub_helper(b: &ExprBuilder) -> Expr {
    let sub_ch = b.app2(
        rt(b, "Helper.ch_from_sub"),
        b.loc("arr"),
        b.app(b.global("PubSub", "subscribe"), b.loc("hub")),
    );
    b.lam(
        &["arr", "hub"],
        b.app(rt(b, "Helper.ch_unwrap"), b.task(sub_ch)),
    )
}

pub(super) fn ch_sched_step_helper(b: &ExprBuilder) -> Expr {
    // (step_fn, inp) ->{} (out, done : Bool)
    let maybe_sleep = b.if_(
        b.app(rt(b, "Duration.IsPositive"), b.loc("del")),
        b.app(rt(b, "Async.EffectSleep"), b.loc("del")),
        b.unit(),
    );
    let cont_branch = b.block(
        [Stmt::Expr(maybe_sleep)],
        b.tuple([b.loc("out"), b.bool_(false)]),
    );
    let on_step = b.if_(
        b.loc("done"),
        b.tuple([b.loc("out"), b.bool_(true)]),
        cont_branch,
    );
    let body = b.block(
        [b.let_("now", b.app(rt(b, "Host.CurrentTimeMillis"), b.unit()))],
        b.match_(
            b.app2(b.loc("step"), b.loc("now"), b.loc("inp")),
            [arm(b.tup_pat(&["out", "del", "done"]), on_step)],
        ),
    );
    b.lam(&["step", "inp"], body)
}

pub(super) fn ch_from_schedule_helper(b: &ExprBuilder) -> Expr {
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.app2(rt(b, "Helper.ch_sched_step"), b.loc("step"), b.unit()),
            [arm(
                pair_pat(b, "out", "done"),
                b.if_(b.loc("done"), b.err(b.loc("out")), b.ok(b.loc("out"))),
            )],
        ),
    );
    b.lam(
        &["sched", "_up", "_own"],
        b.block([b.let_("step", b.app(b.loc("sched"), b.unit()))], pull),
    )
}

pub(super) fn ch_unwrap_helper(b: &ExprBuilder) -> Expr {
    let init_pull = b.block(
        [
            b.let_(
                "ch",
                b.app2(rt(b, "Layer.OwnerRun"), b.loc("own"), b.loc("t")),
            ),
            b.let_("p1", b.app2(b.loc("ch"), b.loc("up"), b.loc("own"))),
            b.rset(b.loc("pr"), b.some(b.loc("p1"))),
        ],
        b.loc("p1"),
    );
    let get_pull = b.match_(
        b.rget(b.loc("pr")),
        [
            arm(b.some_pat("p0"), b.loc("p0")),
            arm(Pat::Wild, init_pull),
        ],
    );
    let pull = b.lam(
        &["_u"],
        b.block([b.let_("p", get_pull)], b.app(b.loc("p"), b.unit())),
    );
    b.lam(
        &["t", "up", "own"],
        b.block([b.let_("pr", b.rmake(b.none()))], pull),
    )
}

pub(super) fn ch_make_pubsub_helper(b: &ExprBuilder) -> Expr {
    // opts: { capacity : Option Int; strategy : Option Queue.Strategy; replay : Option Int }
    let strat_pat = |name: &str| Pat::Con(ExprBuilder::con("Queue", "Strategy", name, 0), vec![]);
    let rec = |with_cap: bool| {
        if with_cap {
            b.at(ExprKind::Record(vec![
                ("capacity".into(), b.loc("c")),
                ("replay".into(), b.field(b.loc("opts"), "replay")),
            ]))
        } else {
            b.at(ExprKind::Record(vec![(
                "replay".into(),
                b.field(b.loc("opts"), "replay"),
            )]))
        }
    };
    let by_strat = b.match_(
        b.loc("st"),
        [
            arm(
                strat_pat("Dropping"),
                b.app(b.global("PubSub", "dropping"), rec(true)),
            ),
            arm(
                strat_pat("Sliding"),
                b.app(b.global("PubSub", "sliding"), rec(true)),
            ),
            arm(Pat::Wild, b.app(b.global("PubSub", "bounded"), rec(true))),
        ],
    );
    let with_cap = b.match_(
        b.field(b.loc("opts"), "strategy"),
        [
            arm(b.some_pat("st"), by_strat),
            arm(Pat::Wild, b.app(b.global("PubSub", "bounded"), rec(true))),
        ],
    );
    let body = b.match_(
        b.field(b.loc("opts"), "capacity"),
        [
            arm(b.some_pat("c"), with_cap),
            arm(
                Pat::Wild,
                b.app(b.global("PubSub", "unbounded"), rec(false)),
            ),
        ],
    );
    b.lam(&["opts"], body)
}
