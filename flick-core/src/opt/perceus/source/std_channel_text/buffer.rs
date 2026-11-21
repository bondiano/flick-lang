//! Buffered and line-splitting channel IR helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{cat, rt, str_lit};
use super::super::{Expr, Pat, Stmt};

fn buffer_producer(b: &ExprBuilder) -> Expr {
    let offer_all = b.block(
        [b.let_(
            "_o",
            b.app2(b.global("Queue", "offer_all"), b.loc("v"), b.loc("q")),
        )],
        b.unit(),
    );
    let offer_one = b.block(
        [b.let_(
            "_o",
            b.app2(b.global("Queue", "offer"), b.loc("v"), b.loc("q")),
        )],
        b.unit(),
    );
    let on_done = b.block(
        [
            b.rset(b.loc("dref"), b.some(b.loc("d"))),
            b.let_("_e", b.app(b.global("Queue", "end"), b.loc("q"))),
        ],
        b.break_("bufp", b.unit()),
    );
    let loop_body = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("v"), b.if_(b.loc("arr"), offer_all, offer_one)),
            arm(b.err_pat("d"), on_done),
        ],
    );
    let run_pull = b.block(
        [b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("forked")))],
        b.loop_ctrl("bufp", loop_body),
    );
    let on_cause = b.lam(
        &["c"],
        b.block(
            [b.let_(
                "_f",
                b.app2(b.global("Queue", "fail_cause"), b.loc("c"), b.loc("q")),
            )],
            b.unit(),
        ),
    );
    let body = b.task(b.app2(
        b.global("Effect.Errors", "catch_cause"),
        on_cause,
        b.task(run_pull),
    ));
    b.app3(
        b.global("Fiber", "spawn"),
        b.loc("forked"),
        b.lam(&["_c"], b.unit()),
        body,
    )
}

fn buffer_consumer(b: &ExprBuilder) -> Expr {
    let take_next = b.if_(
        b.loc("arr"),
        b.app(b.global("Queue", "take_all"), b.loc("q")),
        b.app(b.global("Queue", "take"), b.loc("q")),
    );
    let on_end = b.match_(
        b.rget(b.loc("dref")),
        [
            arm(b.some_pat("d"), b.err(b.loc("d"))),
            arm(Pat::Wild, b.err(b.unit())),
        ],
    );
    b.lam(
        &["_u"],
        b.match_(
            take_next,
            [
                arm(b.some_pat("v"), b.ok(b.loc("v"))),
                arm(Pat::Wild, on_end),
            ],
        ),
    )
}

pub(super) fn ch_buffer_helper(b: &ExprBuilder) -> Expr {
    // (arr: Bool, opts: Buffer, self: Channel) -> Channel
    // Producer fiber runs in forked scope, pushes elements (or chunk elements if arr) into `q`,
    // saves done value in `dref`, and calls `Queue.end q` or `Queue.fail_cause c q`.
    let shutdown_fin = b.task(b.block(
        [b.let_("_sh", b.app(b.global("Queue", "shutdown"), b.loc("q")))],
        b.unit(),
    ));
    let reg_fin = b.app2(
        rt(b, "Layer.OwnerRun"),
        b.loc("forked"),
        b.task(b.app(b.global("Scope", "add_finalizer"), shutdown_fin)),
    );
    let build = b.lam(
        &["up", "_own", "forked"],
        b.block(
            [
                b.let_("q", b.app(b.global("Queue", "make"), b.loc("opts"))),
                b.let_("dref", b.rmake(b.none())),
                Stmt::Expr(reg_fin),
                b.let_("_fib", buffer_producer(b)),
            ],
            buffer_consumer(b),
        ),
    );
    b.lam(
        &["arr", "opts", "self"],
        b.app(rt(b, "Helper.ch_bracket"), build),
    )
}

fn strip_cr(b: &ExprBuilder, s: Expr) -> Expr {
    let last_char = b.app3(
        b.global("String", "slice"),
        b.sub_i(b.loc("sl"), b.int(1)),
        b.loc("sl"),
        b.loc("sv"),
    );
    let ends_cr = b.bin(
        BinOp::And,
        b.bin(BinOp::Gt, b.loc("sl"), b.int(0)),
        b.bin(BinOp::Eq, last_char, str_lit(b, "\r")),
    );
    let trimmed = b.app3(
        b.global("String", "slice"),
        b.int(0),
        b.sub_i(b.loc("sl"), b.int(1)),
        b.loc("sv"),
    );
    b.block(
        [
            b.let_("sv", s),
            b.let_("sl", b.app(b.global("String", "length"), b.loc("sv"))),
        ],
        b.if_(ends_cr, trimmed, b.loc("sv")),
    )
}

fn split_lines_chunk(b: &ExprBuilder) -> Expr {
    let part_i = b.app2(rt(b, "Helper.list_at"), b.var("i"), b.loc("parts"));
    let push_line = b.block(
        [
            b.let_("ln", strip_cr(b, part_i)),
            b.assign(
                "out",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("out"),
                    b.list([b.loc("ln")]),
                ),
            ),
            b.assign("i", b.add_i(b.var("i"), b.int(1))),
        ],
        b.unit(),
    );
    let last_part = b.app2(
        rt(b, "Helper.list_at"),
        b.sub_i(b.loc("pn"), b.int(1)),
        b.loc("parts"),
    );
    let each_str = b.block(
        [
            b.let_("comb", cat(b, b.var("cur"), b.loc("s"))),
            b.let_(
                "parts",
                b.app2(b.global("String", "split"), str_lit(b, "\n"), b.loc("comb")),
            ),
            b.let_("pn", b.app(b.global("List.Core", "length"), b.loc("parts"))),
            b.var_decl("i", b.int(0)),
            Stmt::Expr(b.while_(
                b.bin(BinOp::Lt, b.var("i"), b.sub_i(b.loc("pn"), b.int(1))),
                push_line,
            )),
            b.assign("cur", last_part),
        ],
        b.unit(),
    );
    let has_out = b.bin(
        BinOp::Gt,
        b.app(b.global("List.Core", "length"), b.var("out")),
        b.int(0),
    );
    b.block(
        [
            b.var_decl("out", b.list(vec![])),
            b.var_decl("cur", b.rget(b.loc("rem"))),
            Stmt::Expr(b.each("s", b.loc("xs"), each_str)),
            b.rset(b.loc("rem"), b.var("cur")),
        ],
        b.if_(has_out, b.break_("spl", b.ok(b.var("out"))), b.unit()),
    )
}

pub(super) fn ch_split_lines_helper(b: &ExprBuilder) -> Expr {
    // Channel over `NonEmpty String -> NonEmpty String` splitting on `\n` or `\r\n`.
    let flush_left = b.block(
        [
            b.rset(b.loc("rem"), str_lit(b, "")),
            b.rset(b.loc("done_v"), b.some(b.loc("d"))),
        ],
        b.break_("spl", b.ok(b.list([strip_cr(b, b.loc("left"))]))),
    );
    let on_done = b.block(
        [b.let_("left", b.rget(b.loc("rem")))],
        b.if_(
            b.bin(
                BinOp::Gt,
                b.app(b.global("String", "length"), b.loc("left")),
                b.int(0),
            ),
            flush_left,
            b.break_("spl", b.err(b.loc("d"))),
        ),
    );
    let pull_loop = b.loop_ctrl(
        "spl",
        b.match_(
            b.app(b.loc("up"), b.unit()),
            [
                arm(b.ok_pat("xs"), split_lines_chunk(b)),
                arm(b.err_pat("d"), on_done),
            ],
        ),
    );
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.rget(b.loc("done_v")),
            [
                arm(b.some_pat("dv"), b.err(b.loc("dv"))),
                arm(Pat::Wild, pull_loop),
            ],
        ),
    );
    b.lam(
        &["up", "_own"],
        b.block(
            [
                b.let_("rem", b.rmake(str_lit(b, ""))),
                b.let_("done_v", b.rmake(b.none())),
            ],
            pull,
        ),
    )
}
