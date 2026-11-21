//! `Stream.Combine` `raceAll` and `interleaveWith` helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat, Stmt};

fn race_make_task(b: &ExprBuilder) -> Expr {
    let on_exit = b.lam(
        &["ex"],
        b.match_(
            b.app(b.global("Exit", "failure"), b.loc("ex")),
            [
                arm(
                    b.some_pat("c"),
                    b.some(b.task(b.app(b.loc("cclose"), b.some(b.loc("c"))))),
                ),
                arm(Pat::Wild, b.none()),
            ],
        ),
    );
    let claim = b.match_(
        b.rget(b.loc("win")),
        [
            arm(
                b.some_pat("_w"),
                b.block([Stmt::Expr(b.app(b.loc("cclose"), b.none()))], b.loc("res")),
            ),
            arm(
                Pat::Wild,
                b.block([b.rset(b.loc("win"), b.some(b.loc("p")))], b.loc("res")),
            ),
        ],
    );
    let run = b.task(b.block(
        [
            b.let_(
                "p",
                b.app2(b.loc("s"), rt(b, "Helper.ch_empty_pull"), b.loc("co")),
            ),
            b.let_("res", b.app(b.loc("p"), b.unit())),
        ],
        claim,
    ));
    let scoped = b.app3(
        b.global("Effect.Errors", "on_exit_primitive"),
        b.bool_(false),
        on_exit,
        run,
    );
    b.lam(
        &["s"],
        b.task(b.match_(
            b.app(rt(b, "Helper.ch_fork_scope"), b.loc("own")),
            [arm(pair_pat(b, "co", "cclose"), scoped)],
        )),
    )
}

pub(super) fn st_race_all_helper(b: &ExprBuilder) -> Expr {
    let step_race = b.block(
        [
            b.let_("prev", b.var("raced")),
            b.let_(
                "nxt",
                b.app(
                    b.loc("make_t"),
                    b.app2(rt(b, "Helper.list_at"), b.var("i"), b.loc("streams")),
                ),
            ),
            b.assign(
                "raced",
                b.task(b.app2(rt(b, "Async.RaceFirst"), b.loc("prev"), b.loc("nxt"))),
            ),
            b.assign("i", b.add_i(b.var("i"), b.int(1))),
        ],
        b.unit(),
    );
    let first = b.app(
        b.loc("make_t"),
        b.app2(rt(b, "Helper.list_at"), b.int(0), b.loc("streams")),
    );
    let race_all = b.block(
        [
            b.let_("make_t", race_make_task(b)),
            b.var_decl("raced", first),
            b.var_decl("i", b.int(1)),
            b.let_(
                "n",
                b.app(b.global("List.Core", "length"), b.loc("streams")),
            ),
            Stmt::Expr(b.while_(b.bin(BinOp::Lt, b.var("i"), b.loc("n")), step_race)),
        ],
        b.await_(b.var("raced")),
    );
    let is_empty = b.bin(
        BinOp::Eq,
        b.app(b.global("List.Core", "length"), b.loc("streams")),
        b.int(0),
    );
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.rget(b.loc("win")),
            [
                arm(b.some_pat("wp"), b.app(b.loc("wp"), b.unit())),
                arm(Pat::Wild, b.if_(is_empty, b.err(b.unit()), race_all)),
            ],
        ),
    );
    b.lam(
        &["streams", "_up", "own"],
        b.block([b.let_("win", b.rmake(b.none()))], pull),
    )
}

fn interleave_side(b: &ExprBuilder, pull_loc: &str, done_loc: &str) -> Expr {
    let pull_one = b.match_(
        b.app(b.loc(pull_loc), b.unit()),
        [
            arm(b.ok_pat("v"), b.break_("sil", b.ok(b.list([b.loc("v")])))),
            arm(
                b.err_pat("_d"),
                b.block([b.rset(b.loc(done_loc), b.bool_(true))], b.unit()),
            ),
        ],
    );
    b.if_(b.rget(b.loc(done_loc)), b.unit(), pull_one)
}

pub(super) fn st_interleave_with_helper(b: &ExprBuilder) -> Expr {
    let choose = b.if_(
        b.loc("go_l"),
        interleave_side(b, "pl", "ld"),
        interleave_side(b, "pr", "rd"),
    );
    let pull_dec = b.match_(
        b.app(b.loc("pd"), b.unit()),
        [
            arm(b.ok_pat("go_l"), choose),
            arm(b.err_pat("d"), b.break_("sil", b.err(b.loc("d")))),
        ],
    );
    let both_done = b.bin(BinOp::And, b.rget(b.loc("ld")), b.rget(b.loc("rd")));
    let loop_body = b.if_(both_done, b.break_("sil", b.err(b.unit())), pull_dec);
    let init = [
        b.let_(
            "pd",
            b.app3(
                rt(b, "Helper.ch_flatten_array"),
                b.loc("dec"),
                b.loc("up"),
                b.loc("own"),
            ),
        ),
        b.let_(
            "pl",
            b.app3(
                rt(b, "Helper.ch_flatten_array"),
                b.loc("self"),
                b.loc("up"),
                b.loc("own"),
            ),
        ),
        b.let_(
            "pr",
            b.app3(
                rt(b, "Helper.ch_flatten_array"),
                b.loc("that"),
                b.loc("up"),
                b.loc("own"),
            ),
        ),
        b.let_("ld", b.rmake(b.bool_(false))),
        b.let_("rd", b.rmake(b.bool_(false))),
    ];
    let pull = b.lam(&["_u"], b.loop_ctrl("sil", loop_body));
    b.lam(&["that", "dec", "self", "up", "own"], b.block(init, pull))
}
