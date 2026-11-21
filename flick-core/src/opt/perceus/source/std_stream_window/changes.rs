//! `Stream.Transform` `changesWith` and `intersperse` helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, Pat, Stmt};

pub(super) fn st_changes_with_helper(b: &ExprBuilder) -> Expr {
    let record = b.block(
        [
            b.assign("prev", b.some(b.loc("a"))),
            b.assign(
                "out",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("out"),
                    b.list([b.loc("a")]),
                ),
            ),
        ],
        b.unit(),
    );
    let on_some = b.if_(
        b.app2(b.loc("same"), b.loc("a"), b.loc("p")),
        b.unit(),
        record.clone(),
    );
    let step_el = b.match_(
        b.var("prev"),
        [arm(b.some_pat("p"), on_some), arm(Pat::Wild, record)],
    );
    let has_out = b.bin(
        BinOp::Gt,
        b.app(b.global("List.Core", "length"), b.var("out")),
        b.int(0),
    );
    let on_chunk = b.block(
        [
            b.var_decl("out", b.list(vec![])),
            b.var_decl("prev", b.rget(b.loc("last"))),
            Stmt::Expr(b.each("a", b.loc("xs"), step_el)),
            b.rset(b.loc("last"), b.var("prev")),
        ],
        b.if_(has_out, b.break_("schg", b.ok(b.var("out"))), b.unit()),
    );
    let loop_body = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("xs"), on_chunk),
            arm(b.err_pat("d"), b.break_("schg", b.err(b.loc("d")))),
        ],
    );
    let init = [
        b.let_("pull", b.app2(b.loc("self"), b.loc("up"), b.loc("own"))),
        b.let_("last", b.rmake(b.none())),
    ];
    let pull = b.lam(&["_u"], b.loop_ctrl("schg", loop_body));
    b.lam(&["same", "self", "up", "own"], b.block(init, pull))
}

pub(super) fn st_intersperse_helper(b: &ExprBuilder) -> Expr {
    let maybe_sep = b.if_(
        b.var("f0"),
        b.assign_e("f0", b.bool_(false)),
        b.yield_(b.loc("sep")),
    );
    let step_el = b.block([Stmt::Expr(maybe_sep)], b.yield_(b.loc("a")));
    let on_chunk = b.block(
        [
            b.var_decl("f0", b.rget(b.loc("first"))),
            b.let_("out", b.gen_strict(b.each("a", b.loc("xs"), step_el))),
            b.rset(b.loc("first"), b.bool_(false)),
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
        b.let_("first", b.rmake(b.bool_(true))),
    ];
    b.lam(&["sep", "self", "up", "own"], b.block(init, pull))
}
