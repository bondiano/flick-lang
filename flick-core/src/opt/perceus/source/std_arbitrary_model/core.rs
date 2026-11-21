//! Core shrink-tree pull, retention, and mapping helpers for `Arbitrary`.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat, Stmt};
use super::rset_e;

pub(super) fn pull_each_helper(b: &ExprBuilder) -> Expr {
    let next_val = b.some(b.app(
        b.loc("f"),
        b.app2(rt(b, "Helper.list_at"), b.loc("i"), b.loc("xs")),
    ));
    let step = b.block(
        [b.rset(b.loc("ir"), b.add_i(b.loc("i"), b.int(1)))],
        next_val,
    );
    let body = b.if_(b.bin(BinOp::Ge, b.loc("i"), b.loc("len")), b.none(), step);
    let pull = b.lam(&["_u"], b.block([b.let_("i", b.rget(b.loc("ir")))], body));
    let stmts = [
        b.let_("len", b.app(b.global("List.Core", "length"), b.loc("xs"))),
        b.let_("ir", b.rmake(b.int(0))),
    ];
    b.lam(&["xs", "f"], b.block(stmts, pull))
}

pub(super) fn concat_pulls_helper(b: &ExprBuilder) -> Expr {
    let cond = b.bin(
        BinOp::And,
        b.not(b.var("done")),
        b.bin(BinOp::Lt, b.rget(b.loc("ir")), b.loc("len")),
    );
    let cur_pull = b.app2(rt(b, "Helper.list_at"), b.rget(b.loc("ir")), b.loc("pulls"));
    let on_item = b.block(
        [
            b.assign("res", b.some(b.loc("item"))),
            b.assign("done", b.bool_(true)),
        ],
        b.unit(),
    );
    let on_none = rset_e(b, b.loc("ir"), b.add_i(b.rget(b.loc("ir")), b.int(1)));
    let loop_body = b.match_(
        b.app(cur_pull, b.unit()),
        [arm(b.some_pat("item"), on_item), arm(Pat::Wild, on_none)],
    );
    let pull = b.lam(
        &["_u"],
        b.block(
            [
                b.var_decl("res", b.none()),
                b.var_decl("done", b.bool_(false)),
                Stmt::Expr(b.while_(cond, loop_body)),
            ],
            b.var("res"),
        ),
    );
    let stmts = [
        b.let_(
            "len",
            b.app(b.global("List.Core", "length"), b.loc("pulls")),
        ),
        b.let_("ir", b.rmake(b.int(0))),
    ];
    b.lam(&["pulls"], b.block(stmts, pull))
}

pub(super) fn lazy_shrinks_helper(b: &ExprBuilder) -> Expr {
    let made = b.match_(
        b.app(b.loc("mk"), b.unit()),
        [
            arm(b.some_pat("q"), b.loc("q")),
            arm(Pat::Wild, b.lam(&["_"], b.none())),
        ],
    );
    let init = b.block(
        [
            b.let_("made", made),
            b.rset(b.loc("pr"), b.some(b.loc("made"))),
        ],
        b.loc("made"),
    );
    let get_p = b.match_(
        b.rget(b.loc("pr")),
        [
            arm(b.some_pat("existing"), b.loc("existing")),
            arm(Pat::Wild, init),
        ],
    );
    let pull = b.lam(
        &["_u"],
        b.block([b.let_("p", get_p)], b.app(b.loc("p"), b.unit())),
    );
    b.lam(
        &["val", "mk"],
        b.block(
            [b.let_("pr", b.rmake(b.none()))],
            b.tuple([b.loc("val"), b.some(pull)]),
        ),
    )
}

fn retain_pull_fresh(b: &ExprBuilder) -> Expr {
    let retained_opt = b.match_(
        b.loc("opt_a"),
        [
            arm(
                b.some_pat("a"),
                b.some(b.app(rt(b, "Helper.arb_retain"), b.loc("a"))),
            ),
            arm(Pat::Wild, b.none()),
        ],
    );
    let append_h = b.app2(
        b.global("List.Core", "append"),
        b.loc("h"),
        b.list([b.loc("it")]),
    );
    let on_opt_a = b.block(
        [
            b.let_("it", retained_opt),
            b.rset(b.loc("hr"), append_h),
            b.rset(b.loc("ir"), b.add_i(b.loc("i"), b.int(1))),
        ],
        b.some(b.loc("it")),
    );
    let on_end = b.block([b.rset(b.loc("er"), b.bool_(true))], b.none());
    let pull_step = b.match_(
        b.app(b.loc("pull"), b.unit()),
        [arm(b.some_pat("opt_a"), on_opt_a), arm(Pat::Wild, on_end)],
    );
    b.if_(b.rget(b.loc("er")), b.none(), pull_step)
}

pub(super) fn retain_helper(b: &ExprBuilder) -> Expr {
    let from_cache = b.block(
        [b.rset(b.loc("ir"), b.add_i(b.loc("i"), b.int(1)))],
        b.some(b.app2(rt(b, "Helper.list_at"), b.loc("i"), b.loc("h"))),
    );
    let in_cache = b.bin(
        BinOp::Lt,
        b.loc("i"),
        b.app(b.global("List.Core", "length"), b.loc("h")),
    );
    let cursor = b.lam(
        &["_u2"],
        b.block(
            [
                b.let_("i", b.rget(b.loc("ir"))),
                b.let_("h", b.rget(b.loc("hr"))),
            ],
            b.if_(in_cache, from_cache, retain_pull_fresh(b)),
        ),
    );
    let mk_cursor = b.lam(&["_u"], b.block([b.let_("ir", b.rmake(b.int(0)))], cursor));
    let with_pull = b.block(
        [
            b.let_("hr", b.rmake(b.list(vec![]))),
            b.let_("er", b.rmake(b.bool_(false))),
        ],
        b.tuple([b.loc("val"), b.some(mk_cursor)]),
    );
    let match_sh = b.match_(
        b.loc("sh_opt"),
        [
            arm(b.some_pat("pull"), with_pull),
            arm(Pat::Wild, b.tuple([b.loc("val"), b.none()])),
        ],
    );
    b.lam(
        &["s"],
        b.match_(b.loc("s"), [arm(pair_pat(b, "val", "sh_opt"), match_sh)]),
    )
}

pub(super) fn from_retained_helper(b: &ExprBuilder) -> Expr {
    let mapped = b.app2(
        rt(b, "Helper.arb_map_retained"),
        b.app(b.loc("mk"), b.unit()),
        rt(b, "Helper.arb_from_retained"),
    );
    let match_mk = b.match_(
        b.loc("mk_opt"),
        [
            arm(b.some_pat("mk"), b.tuple([b.loc("v"), b.some(mapped)])),
            arm(Pat::Wild, b.tuple([b.loc("v"), b.none()])),
        ],
    );
    b.lam(
        &["r"],
        b.match_(b.loc("r"), [arm(pair_pat(b, "v", "mk_opt"), match_mk)]),
    )
}

pub(super) fn map_retained_helper(b: &ExprBuilder) -> Expr {
    let on_opt_r = b.match_(
        b.loc("opt_r"),
        [
            arm(
                b.some_pat("r"),
                b.some(b.some(b.app(b.loc("f"), b.loc("r")))),
            ),
            arm(Pat::Wild, b.some(b.none())),
        ],
    );
    let body = b.match_(
        b.app(b.loc("nxt"), b.unit()),
        [arm(b.some_pat("opt_r"), on_opt_r), arm(Pat::Wild, b.none())],
    );
    b.lam(&["nxt", "f", "_u"], body)
}

pub(super) fn map_pull_helper(b: &ExprBuilder) -> Expr {
    let on_opt_s = b.match_(
        b.loc("opt_s"),
        [
            arm(
                b.some_pat("s"),
                b.some(b.some(b.app(b.loc("f"), b.loc("s")))),
            ),
            arm(Pat::Wild, b.some(b.none())),
        ],
    );
    let body = b.match_(
        b.app(b.loc("p"), b.unit()),
        [arm(b.some_pat("opt_s"), on_opt_s), arm(Pat::Wild, b.none())],
    );
    b.lam(&["p", "f", "_u"], body)
}

pub(super) fn map_sample_helper(b: &ExprBuilder) -> Expr {
    let rec_map = b.lam(
        &["c"],
        b.app2(rt(b, "Helper.arb_map_sample"), b.loc("c"), b.loc("f")),
    );
    let mapped_sh = b.match_(
        b.loc("sh_opt"),
        [
            arm(
                b.some_pat("sh"),
                b.some(b.app2(rt(b, "Helper.arb_map_pull"), b.loc("sh"), rec_map)),
            ),
            arm(Pat::Wild, b.none()),
        ],
    );
    let pair = b.tuple([b.app(b.loc("f"), b.loc("v")), mapped_sh]);
    b.lam(
        &["s", "f"],
        b.match_(b.loc("s"), [arm(pair_pat(b, "v", "sh_opt"), pair)]),
    )
}
