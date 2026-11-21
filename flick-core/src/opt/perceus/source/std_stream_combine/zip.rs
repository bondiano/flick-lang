//! `Stream.Combine` `zipWith`, `zipWithArray`, and `zipWithIndex` helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{rt, triple_pat};
use super::super::{Expr, Pat};

pub(super) fn st_zip_with_helper(b: &ExprBuilder) -> Expr {
    let apply_elem = b.app2(
        b.loc("f"),
        b.app2(rt(b, "Helper.list_at"), b.var("i"), b.loc("l")),
        b.app2(rt(b, "Helper.list_at"), b.var("i"), b.loc("r")),
    );
    let step = b.block(
        [
            b.let_("v", apply_elem),
            b.assign("i", b.add_i(b.var("i"), b.int(1))),
        ],
        b.yield_(b.loc("v")),
    );
    let min_len = b.if_(
        b.bin(BinOp::Lt, b.loc("ll"), b.loc("rl")),
        b.loc("ll"),
        b.loc("rl"),
    );
    let stmts = [
        b.let_("ll", b.app(b.global("List.Core", "length"), b.loc("l"))),
        b.let_("rl", b.app(b.global("List.Core", "length"), b.loc("r"))),
        b.let_("n", min_len),
        b.var_decl("i", b.int(0)),
        b.let_(
            "out",
            b.gen_strict(b.while_(b.bin(BinOp::Lt, b.var("i"), b.loc("n")), step)),
        ),
    ];
    let combine = b.lam(
        &["l", "r"],
        b.block(
            stmts,
            b.tuple([
                b.loc("out"),
                b.app2(b.global("List.Core", "drop"), b.loc("n"), b.loc("l")),
                b.app2(b.global("List.Core", "drop"), b.loc("n"), b.loc("r")),
            ]),
        ),
    );
    b.lam(
        &["that", "f", "self"],
        b.app3(
            rt(b, "Helper.st_zip_with_array"),
            b.loc("that"),
            combine,
            b.loc("self"),
        ),
    )
}

fn zip_apply_pair(b: &ExprBuilder, le: Expr, re: Expr) -> Expr {
    let opt_non_empty = |name: &str| {
        b.if_(
            b.bin(
                BinOp::Gt,
                b.app(b.global("List.Core", "length"), b.loc(name)),
                b.int(0),
            ),
            b.some(b.loc(name)),
            b.none(),
        )
    };
    let save = b.block(
        [
            b.rset(b.loc("lbuf"), opt_non_empty("lo")),
            b.rset(b.loc("rbuf"), opt_non_empty("ro")),
        ],
        b.ok(b.loc("out")),
    );
    b.match_(
        b.app2(b.loc("f"), le, re),
        [arm(triple_pat(b, "out", "lo", "ro"), save)],
    )
}

pub(super) fn st_zip_with_array_helper(b: &ExprBuilder) -> Expr {
    let with_left = b.match_(
        b.app(b.loc("pr"), b.unit()),
        [
            arm(b.ok_pat("r"), zip_apply_pair(b, b.loc("l"), b.loc("r"))),
            arm(b.err_pat("d"), b.err(b.loc("d"))),
        ],
    );
    let with_right = b.match_(
        b.app(b.loc("pl"), b.unit()),
        [
            arm(b.ok_pat("l"), zip_apply_pair(b, b.loc("l"), b.loc("r"))),
            arm(b.err_pat("d"), b.err(b.loc("d"))),
        ],
    );
    let pull_both = b.match_(
        b.app(b.loc("pl"), b.unit()),
        [
            arm(b.ok_pat("l"), with_left.clone()),
            arm(b.err_pat("d"), b.err(b.loc("d"))),
        ],
    );
    let check_right = b.match_(
        b.rget(b.loc("rbuf")),
        [arm(b.some_pat("r"), with_right), arm(Pat::Wild, pull_both)],
    );
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.rget(b.loc("lbuf")),
            [arm(b.some_pat("l"), with_left), arm(Pat::Wild, check_right)],
        ),
    );
    let init = [
        b.let_(
            "pl",
            b.app2(b.loc("self"), rt(b, "Helper.ch_empty_pull"), b.loc("own")),
        ),
        b.let_(
            "pr",
            b.app2(b.loc("that"), rt(b, "Helper.ch_empty_pull"), b.loc("own")),
        ),
        b.let_("lbuf", b.rmake(b.none())),
        b.let_("rbuf", b.rmake(b.none())),
    ];
    b.lam(&["that", "f", "self", "_up", "own"], b.block(init, pull))
}

pub(super) fn st_zip_with_index_helper(b: &ExprBuilder) -> Expr {
    let step = b.block(
        [
            b.let_("ci", b.var("i")),
            b.assign("i", b.add_i(b.loc("ci"), b.int(1))),
        ],
        b.yield_(b.tuple([b.loc("a"), b.loc("ci")])),
    );
    let on_chunk = b.block(
        [
            b.var_decl("i", b.rget(b.loc("idx"))),
            b.let_("out", b.gen_strict(b.each("a", b.loc("xs"), step))),
            b.rset(b.loc("idx"), b.var("i")),
        ],
        b.ok(b.loc("out")),
    );
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.app(b.loc("pull"), b.unit()),
            [
                arm(b.ok_pat("xs"), on_chunk),
                arm(b.err_pat("d"), b.err(b.loc("d"))),
            ],
        ),
    );
    let init = [
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("idx", b.rmake(b.int(0))),
    ];
    b.lam(&["self", "up", "own"], b.block(init, pull))
}
