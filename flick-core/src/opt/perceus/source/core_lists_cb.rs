//! Closed pure templates for callback-taking `flick:rt` list operations (including stable sort).

use flick_ir::{Arith, BinOp, Num};

use super::expr_builder::{ExprBuilder, arm};
use super::{Expr, Pat, Span, Stmt};

pub(super) fn template(from: &str, bind: &str, stages: usize, span: Span) -> Option<Expr> {
    if from != "flick:rt" {
        return None;
    }
    let b = ExprBuilder::new(span, "$lcb");
    match (bind, stages) {
        ("listMapIndexed", 2) => Some(list_map_indexed(&b)),
        ("listFilter", 2) => Some(b.lam(
            &["p", "xs"],
            b.gen_strict(b.each(
                "x",
                b.loc("xs"),
                b.if_(
                    b.app(b.loc("p"), b.loc("x")),
                    b.yield_(b.loc("x")),
                    b.unit(),
                ),
            )),
        )),
        ("listFilterMap", 2) => Some(b.lam(
            &["f", "xs"],
            b.gen_strict(b.each(
                "x",
                b.loc("xs"),
                b.match_(
                    b.app(b.loc("f"), b.loc("x")),
                    vec![
                        arm(b.some_pat("y"), b.yield_(b.loc("y"))),
                        arm(Pat::Wild, b.unit()),
                    ],
                ),
            )),
        )),
        ("listFind", 2) => Some(list_find(&b, false)),
        ("listFindIndex", 2) => Some(list_find(&b, true)),
        ("listAll", 2) => Some(list_all(&b)),
        ("listPartition", 2) => Some(list_partition(&b)),
        ("listInitialize", 2) => Some(list_initialize(&b)),
        _ => None,
    }
}

pub(super) fn sort_by_template(span: Span) -> Expr {
    stable_merge_sort(&ExprBuilder::new(span, "$lcb"))
}

fn list_map_indexed(b: &ExprBuilder) -> Expr {
    b.lam(
        &["f", "xs"],
        b.gen_strict(b.block(
            vec![b.var_decl("i", b.int(0))],
            b.each(
                "x",
                b.loc("xs"),
                b.block(
                    vec![
                        b.let_("idx", b.var("i")),
                        b.assign("i", b.add_i(b.var("i"), b.int(1))),
                    ],
                    b.yield_(b.app2(b.loc("f"), b.loc("idx"), b.loc("x"))),
                ),
            ),
        )),
    )
}

fn list_find(b: &ExprBuilder, index_only: bool) -> Expr {
    let found_val = if index_only { b.var("i") } else { b.loc("x") };
    let miss_step = if index_only {
        b.block(vec![b.assign("i", b.add_i(b.var("i"), b.int(1)))], b.unit())
    } else {
        b.unit()
    };
    let mut stmts = Vec::new();
    if index_only {
        stmts.push(b.var_decl("i", b.int(0)));
    }
    stmts.push(b.var_decl("found", b.none()));
    stmts.push(Stmt::Expr(b.for_ctrl(
        "$lfind",
        "x",
        b.loc("xs"),
        b.if_(
            b.app(b.loc("p"), b.loc("x")),
            b.block(
                vec![b.assign("found", b.some(found_val))],
                b.break_("$lfind", b.unit()),
            ),
            miss_step,
        ),
    )));
    b.lam(&["p", "xs"], b.block(stmts, b.var("found")))
}

fn list_all(b: &ExprBuilder) -> Expr {
    b.lam(
        &["p", "xs"],
        b.block(
            vec![
                b.var_decl("ok", b.bool_(true)),
                Stmt::Expr(b.for_ctrl(
                    "$lall",
                    "x",
                    b.loc("xs"),
                    b.if_(
                        b.app(b.loc("p"), b.loc("x")),
                        b.unit(),
                        b.block(
                            vec![b.assign("ok", b.bool_(false))],
                            b.break_("$lall", b.unit()),
                        ),
                    ),
                )),
            ],
            b.var("ok"),
        ),
    )
}

fn list_partition(b: &ExprBuilder) -> Expr {
    let push = |var: &str| {
        b.block(
            vec![b.assign(
                var,
                b.bin(BinOp::ListConcat, b.var(var), b.list(vec![b.loc("x")])),
            )],
            b.unit(),
        )
    };
    b.lam(
        &["p", "xs"],
        b.block(
            vec![
                b.var_decl("yes", b.list(vec![])),
                b.var_decl("no", b.list(vec![])),
                Stmt::Expr(b.each(
                    "x",
                    b.loc("xs"),
                    b.if_(b.app(b.loc("p"), b.loc("x")), push("yes"), push("no")),
                )),
            ],
            b.tuple(vec![b.var("yes"), b.var("no")]),
        ),
    )
}

fn list_initialize(b: &ExprBuilder) -> Expr {
    b.lam(
        &["n", "f"],
        b.gen_strict(b.block(
            vec![b.var_decl("i", b.int(0))],
            b.while_(
                b.bin(BinOp::Lt, b.var("i"), b.loc("n")),
                b.block(
                    vec![
                        b.let_("idx", b.var("i")),
                        b.assign("i", b.add_i(b.var("i"), b.int(1))),
                    ],
                    b.yield_(b.app(b.loc("f"), b.loc("idx"))),
                ),
            ),
        )),
    )
}

fn merge_runs_into_yield(b: &ExprBuilder) -> Expr {
    let pick_pair = b.if_(
        b.bin(
            BinOp::Eq,
            b.app2(b.loc("cmp"), b.loc("lv"), b.loc("rv")),
            b.ord("GT"),
        ),
        b.block(
            vec![
                Stmt::Expr(b.yield_(b.loc("rv"))),
                b.assign("r", b.loc("rtl")),
            ],
            b.unit(),
        ),
        b.block(
            vec![
                Stmt::Expr(b.yield_(b.loc("lv"))),
                b.assign("l", b.loc("ltl")),
            ],
            b.unit(),
        ),
    );
    let drain_l = b.block(
        vec![Stmt::Expr(b.each("x", b.var("l"), b.yield_(b.loc("x"))))],
        b.break_("$mrg", b.unit()),
    );
    let drain_r = b.block(
        vec![Stmt::Expr(b.each("y", b.var("r"), b.yield_(b.loc("y"))))],
        b.break_("$mrg", b.unit()),
    );
    let cons = |hd: &str, tl: &str| {
        Pat::List(
            vec![Pat::Bind(b.name(hd))],
            Some(Box::new(Pat::Bind(b.name(tl)))),
        )
    };
    b.at(super::ExprKind::ControlLoop {
        id: "$mrg".into(),
        condition: None,
        iterable: false,
        body: Box::new(b.match_(
            b.var("l"),
            vec![
                arm(
                    cons("lv", "ltl"),
                    b.match_(
                        b.var("r"),
                        vec![arm(cons("rv", "rtl"), pick_pair), arm(Pat::Wild, drain_l)],
                    ),
                ),
                arm(Pat::Wild, drain_r),
            ],
        )),
    })
}

fn merge_pass(b: &ExprBuilder) -> Expr {
    let slice_of = |s: Expr, e: Expr, xs: Expr| b.app3(b.global("List.Core", "slice"), s, e, xs);
    let clamp_n = |raw: &str| {
        b.if_(
            b.bin(BinOp::Lt, b.loc(raw), b.loc("n")),
            b.loc(raw),
            b.loc("n"),
        )
    };
    b.gen_strict(b.block(
        vec![b.var_decl("pos", b.int(0))],
        b.while_(
            b.bin(BinOp::Lt, b.var("pos"), b.loc("n")),
            b.block(
                vec![
                    b.let_("mid_raw", b.add_i(b.var("pos"), b.var("width"))),
                    b.let_("mid", clamp_n("mid_raw")),
                    b.let_("end_raw", b.add_i(b.loc("mid"), b.var("width"))),
                    b.let_("end", clamp_n("end_raw")),
                    b.var_decl("l", slice_of(b.var("pos"), b.loc("mid"), b.var("cur"))),
                    b.var_decl("r", slice_of(b.loc("mid"), b.loc("end"), b.var("cur"))),
                    Stmt::Expr(merge_runs_into_yield(b)),
                    b.assign("pos", b.loc("end")),
                ],
                b.unit(),
            ),
        ),
    ))
}

/// Stable bottom-up iterative merge sort (O(N log N), O(1) stack, preserves equal order).
fn stable_merge_sort(b: &ExprBuilder) -> Expr {
    b.lam(
        &["len_fn", "cmp", "xs"],
        b.block(
            vec![
                b.let_("n", b.app(b.loc("len_fn"), b.loc("xs"))),
                b.var_decl("cur", b.loc("xs")),
                b.var_decl("width", b.int(1)),
                Stmt::Expr(b.while_(
                    b.bin(BinOp::Lt, b.var("width"), b.loc("n")),
                    b.block(
                        vec![
                            b.assign("cur", merge_pass(b)),
                            b.assign(
                                "width",
                                b.bin(BinOp::Arith(Arith::Mul, Num::Int), b.var("width"), b.int(2)),
                            ),
                        ],
                        b.unit(),
                    ),
                )),
            ],
            b.var("cur"),
        ),
    )
}
