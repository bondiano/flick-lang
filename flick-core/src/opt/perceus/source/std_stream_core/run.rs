//! `Stream.Run` terminal consumer helpers.

use super::super::Expr;
use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::rt;

pub(super) fn st_run_collect_helper(b: &ExprBuilder) -> Expr {
    let step = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(
                b.ok_pat("xs"),
                b.each("x", b.loc("xs"), b.yield_(b.loc("x"))),
            ),
            arm(b.err_pat("_d"), b.break_("src", b.unit())),
        ],
    );
    let consume = b.lam(&["pull"], b.gen_strict(b.loop_ctrl("src", step)));
    b.lam(
        &["self"],
        b.app2(rt(b, "Helper.ch_run_scoped"), b.loc("self"), consume),
    )
}

pub(super) fn st_run_fold_helper(b: &ExprBuilder) -> Expr {
    let fold_chunk = b.each(
        "x",
        b.loc("xs"),
        b.assign_e("acc", b.app2(b.loc("f"), b.var("acc"), b.loc("x"))),
    );
    let step = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("xs"), fold_chunk),
            arm(b.err_pat("_d"), b.break_("srf", b.var("acc"))),
        ],
    );
    let consume = b.lam(
        &["pull"],
        b.block([b.var_decl("acc", b.loc("init"))], b.loop_ctrl("srf", step)),
    );
    b.lam(
        &["init", "f", "self"],
        b.app2(rt(b, "Helper.ch_run_scoped"), b.loc("self"), consume),
    )
}

pub(super) fn st_run_for_each_helper(b: &ExprBuilder) -> Expr {
    let run_chunk = b.each(
        "x",
        b.loc("xs"),
        b.block([b.let_("_r", b.app(b.loc("f"), b.loc("x")))], b.unit()),
    );
    let step = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("xs"), run_chunk),
            arm(b.err_pat("_d"), b.break_("srfe", b.unit())),
        ],
    );
    let consume = b.lam(&["pull"], b.loop_ctrl("srfe", step));
    b.lam(
        &["f", "self"],
        b.app2(rt(b, "Helper.ch_run_scoped"), b.loc("self"), consume),
    )
}

pub(super) fn st_run_for_each_while_helper(b: &ExprBuilder) -> Expr {
    let check_elem = b.if_(
        b.app(b.loc("f"), b.loc("x")),
        b.unit(),
        b.break_("srfw", b.unit()),
    );
    let step = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [
            arm(b.ok_pat("xs"), b.each("x", b.loc("xs"), check_elem)),
            arm(b.err_pat("_d"), b.break_("srfw", b.unit())),
        ],
    );
    let consume = b.lam(&["pull"], b.loop_ctrl("srfw", step));
    b.lam(
        &["f", "self"],
        b.app2(rt(b, "Helper.ch_run_scoped"), b.loc("self"), consume),
    )
}

pub(super) fn st_mk_string_helper(b: &ExprBuilder) -> Expr {
    let collected = b.app(rt(b, "Helper.st_run_collect"), b.loc("self"));
    b.lam(
        &["self"],
        b.app2(b.global("String", "join"), b.str_(""), collected),
    )
}
