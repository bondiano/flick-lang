//! `Stream.Transform` `groupAdjacentBy` helper.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::pair_pat;
use super::super::{Expr, Pat, Stmt};

fn group_adj_step(b: &ExprBuilder) -> Expr {
    let same_key = b.assign_e(
        "st",
        b.some(b.tuple([
            b.loc("pk"),
            b.app2(
                b.global("List.Core", "append"),
                b.loc("grp"),
                b.list([b.loc("a")]),
            ),
        ])),
    );
    let new_group = b.block(
        [
            b.assign(
                "out",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("out"),
                    b.list([b.tuple([b.loc("pk"), b.loc("grp")])]),
                ),
            ),
            b.assign("st", b.some(b.tuple([b.loc("k"), b.list([b.loc("a")])]))),
        ],
        b.unit(),
    );
    let compare_pair = b.match_(
        b.loc("pair"),
        [arm(
            pair_pat(b, "pk", "grp"),
            b.if_(
                b.bin(BinOp::Eq, b.loc("k"), b.loc("pk")),
                same_key,
                new_group,
            ),
        )],
    );
    let first_group = b.assign_e("st", b.some(b.tuple([b.loc("k"), b.list([b.loc("a")])])));
    b.block(
        [b.let_("k", b.app(b.loc("f"), b.loc("a")))],
        b.match_(
            b.var("st"),
            [
                arm(b.some_pat("pair"), compare_pair),
                arm(Pat::Wild, first_group),
            ],
        ),
    )
}

pub(super) fn st_group_adjacent_by_helper(b: &ExprBuilder) -> Expr {
    let has_out = b.bin(
        BinOp::Gt,
        b.app(b.global("List.Core", "length"), b.var("out")),
        b.int(0),
    );
    let on_chunk = b.block(
        [
            b.var_decl("out", b.list(vec![])),
            b.var_decl("st", b.rget(b.loc("cur"))),
            Stmt::Expr(b.each("a", b.loc("xs"), group_adj_step(b))),
            b.rset(b.loc("cur"), b.var("st")),
        ],
        b.if_(has_out, b.break_("sga", b.ok(b.var("out"))), b.unit()),
    );
    let flush_last = b.match_(
        b.loc("last"),
        [
            arm(b.some_pat("p"), b.break_("sga", b.ok(b.list([b.loc("p")])))),
            arm(Pat::Wild, b.break_("sga", b.err(b.loc("d")))),
        ],
    );
    let on_end = b.block(
        [
            b.rset(b.loc("done"), b.bool_(true)),
            b.let_("last", b.rget(b.loc("cur"))),
            b.rset(b.loc("cur"), b.none()),
        ],
        flush_last,
    );
    let loop_body = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [arm(b.ok_pat("xs"), on_chunk), arm(b.err_pat("d"), on_end)],
    );
    let pull = b.lam(
        &["_u"],
        b.if_(
            b.rget(b.loc("done")),
            b.err(b.unit()),
            b.loop_ctrl("sga", loop_body),
        ),
    );
    let init = [
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("cur", b.rmake(b.none())),
        b.let_("done", b.rmake(b.bool_(false))),
    ];
    b.lam(&["f", "self", "up", "own"], b.block(init, pull))
}
