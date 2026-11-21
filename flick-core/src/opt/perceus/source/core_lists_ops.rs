//! Closed pure templates for `flick:Array` set, grouping, scanning, zipping, and unfolding operations.

use flick_ir::BinOp;

use super::expr_builder::{ExprBuilder, arm};
use super::{Expr, ExprKind, Pat, Stmt};

pub(super) fn template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("containsWith", 3) => Some(contains_with(b)),
        ("dedupeWith", 2) => Some(dedupe_with(b)),
        ("dedupeAdjacentWith", 2) => Some(dedupe_adjacent_with(b)),
        ("differenceWith", 3) => Some(set_filter_with(b, true)),
        ("intersectionWith", 3) => Some(set_filter_with(b, false)),
        ("unionWith", 3) => Some(union_with(b)),
        ("groupBy", 2) => Some(group_by(b)),
        ("cartesianWith", 3) => Some(b.lam(
            &["that", "f", "xs"],
            b.gen_strict(b.each(
                "x",
                b.loc("xs"),
                b.each(
                    "y",
                    b.loc("that"),
                    b.yield_(b.app2(b.loc("f"), b.loc("x"), b.loc("y"))),
                ),
            )),
        )),
        ("zipWith", 3) => Some(zip_with(b)),
        ("mapAccum", 3) => Some(map_accum(b)),
        ("scan", 3) => Some(scan_left(b)),
        ("scanRight", 3) => Some(scan_right(b)),
        ("extend", 2) => Some(extend_suffixes(b)),
        ("partition", 2) => Some(partition_map(b)),
        ("unfold", 2) => Some(unfold(b)),
        _ => None,
    }
}

fn contains_with(b: &ExprBuilder) -> Expr {
    b.lam(
        &["eq", "target", "xs"],
        b.block(
            vec![
                b.var_decl("hit", b.bool_(false)),
                Stmt::Expr(b.for_ctrl(
                    "$cw",
                    "x",
                    b.loc("xs"),
                    b.if_(
                        b.app2(b.loc("eq"), b.loc("target"), b.loc("x")),
                        b.block(
                            vec![b.assign("hit", b.bool_(true))],
                            b.break_("$cw", b.unit()),
                        ),
                        b.unit(),
                    ),
                )),
            ],
            b.var("hit"),
        ),
    )
}

fn dedupe_with(b: &ExprBuilder) -> Expr {
    b.lam(
        &["eq", "xs"],
        b.block(
            vec![
                b.var_decl("out", b.list(vec![])),
                Stmt::Expr(b.each(
                    "x",
                    b.loc("xs"),
                    b.if_(
                        b.app3(
                            b.global("List.Search", "contains_with"),
                            b.loc("eq"),
                            b.loc("x"),
                            b.var("out"),
                        ),
                        b.unit(),
                        b.block(
                            vec![b.assign(
                                "out",
                                b.bin(BinOp::ListConcat, b.var("out"), b.list(vec![b.loc("x")])),
                            )],
                            b.unit(),
                        ),
                    ),
                )),
            ],
            b.var("out"),
        ),
    )
}

fn dedupe_adjacent_with(b: &ExprBuilder) -> Expr {
    b.lam(
        &["eq", "xs"],
        b.gen_strict(b.block(
            vec![b.var_decl("prev", b.none())],
            b.each(
                "x",
                b.loc("xs"),
                b.match_(
                    b.var("prev"),
                    vec![
                        arm(
                            b.some_pat("p"),
                            b.if_(
                                b.app2(b.loc("eq"), b.loc("x"), b.loc("p")),
                                b.unit(),
                                b.block(
                                    vec![b.assign("prev", b.some(b.loc("x")))],
                                    b.yield_(b.loc("x")),
                                ),
                            ),
                        ),
                        arm(
                            Pat::Wild,
                            b.block(
                                vec![b.assign("prev", b.some(b.loc("x")))],
                                b.yield_(b.loc("x")),
                            ),
                        ),
                    ],
                ),
            ),
        )),
    )
}

fn set_filter_with(b: &ExprBuilder, negate: bool) -> Expr {
    let hit = b.app3(
        b.global("List.Search", "contains_with"),
        b.loc("eq"),
        b.loc("x"),
        b.loc("that"),
    );
    let cond = if negate { b.not(hit) } else { hit };
    b.lam(
        &["eq", "that", "xs"],
        b.gen_strict(b.each(
            "x",
            b.loc("xs"),
            b.if_(cond, b.yield_(b.loc("x")), b.unit()),
        )),
    )
}

fn union_with(b: &ExprBuilder) -> Expr {
    b.lam(
        &["that", "eq", "xs"],
        b.if_(
            b.bin(
                BinOp::Eq,
                b.app(b.global("List.Core", "length"), b.loc("xs")),
                b.int(0),
            ),
            b.loc("that"),
            b.if_(
                b.bin(
                    BinOp::Eq,
                    b.app(b.global("List.Core", "length"), b.loc("that")),
                    b.int(0),
                ),
                b.loc("xs"),
                b.bin(
                    BinOp::ListConcat,
                    b.loc("xs"),
                    b.gen_strict(b.each(
                        "y",
                        b.loc("that"),
                        b.if_(
                            b.app3(
                                b.global("List.Search", "contains_with"),
                                b.lam(
                                    &["cand", "kept"],
                                    b.app2(b.loc("eq"), b.loc("kept"), b.loc("cand")),
                                ),
                                b.loc("y"),
                                b.loc("xs"),
                            ),
                            b.unit(),
                            b.yield_(b.loc("y")),
                        ),
                    )),
                ),
            ),
        ),
    )
}

fn group_by(b: &ExprBuilder) -> Expr {
    b.lam(
        &["f", "xs"],
        b.block(
            vec![
                b.var_decl("rec", b.app(b.global("Record", "empty"), b.unit())),
                Stmt::Expr(b.each(
                    "x",
                    b.loc("xs"),
                    b.block(
                        vec![
                            b.let_("k", b.app(b.loc("f"), b.loc("x"))),
                            b.let_(
                                "next_grp",
                                b.match_(
                                    b.app2(b.global("Record", "get_key"), b.loc("k"), b.var("rec")),
                                    vec![
                                        arm(
                                            b.some_pat("prev"),
                                            b.bin(
                                                BinOp::ListConcat,
                                                b.loc("prev"),
                                                b.list(vec![b.loc("x")]),
                                            ),
                                        ),
                                        arm(Pat::Wild, b.list(vec![b.loc("x")])),
                                    ],
                                ),
                            ),
                            b.assign(
                                "rec",
                                b.app3(
                                    b.global("Record", "set"),
                                    b.loc("k"),
                                    b.loc("next_grp"),
                                    b.var("rec"),
                                ),
                            ),
                        ],
                        b.unit(),
                    ),
                )),
            ],
            b.var("rec"),
        ),
    )
}

fn zip_with(b: &ExprBuilder) -> Expr {
    b.lam(
        &["that", "f", "xs"],
        b.gen_strict(b.each(
            "pair",
            b.app2(b.global("List.Core", "zip"), b.loc("xs"), b.loc("that")),
            b.match_(
                b.loc("pair"),
                vec![arm(
                    Pat::Tuple(vec![Pat::Bind(b.name("a")), Pat::Bind(b.name("bv"))]),
                    b.yield_(b.app2(b.loc("f"), b.loc("a"), b.loc("bv"))),
                )],
            ),
        )),
    )
}

fn map_accum(b: &ExprBuilder) -> Expr {
    b.lam(
        &["init", "f", "xs"],
        b.block(
            vec![
                b.var_decl("st", b.loc("init")),
                b.let_(
                    "out",
                    b.gen_strict(b.each(
                        "x",
                        b.loc("xs"),
                        b.match_(
                            b.app2(b.loc("f"), b.var("st"), b.loc("x")),
                            vec![arm(
                                Pat::Tuple(vec![Pat::Bind(b.name("ns")), Pat::Bind(b.name("y"))]),
                                b.block(vec![b.assign("st", b.loc("ns"))], b.yield_(b.loc("y"))),
                            )],
                        ),
                    )),
                ),
            ],
            b.tuple(vec![b.var("st"), b.loc("out")]),
        ),
    )
}

fn scan_left(b: &ExprBuilder) -> Expr {
    b.lam(
        &["init", "f", "xs"],
        b.gen_strict(b.block(
            vec![
                b.var_decl("acc", b.loc("init")),
                Stmt::Expr(b.yield_(b.var("acc"))),
            ],
            b.each(
                "x",
                b.loc("xs"),
                b.block(
                    vec![b.assign("acc", b.app2(b.loc("f"), b.var("acc"), b.loc("x")))],
                    b.yield_(b.var("acc")),
                ),
            ),
        )),
    )
}

fn scan_right(b: &ExprBuilder) -> Expr {
    b.lam(
        &["init", "f", "xs"],
        b.app(
            b.global("List.Core", "reverse"),
            b.app3(
                b.global("List.Transform", "scan"),
                b.loc("init"),
                b.loc("f"),
                b.app(b.global("List.Core", "reverse"), b.loc("xs")),
            ),
        ),
    )
}

fn extend_suffixes(b: &ExprBuilder) -> Expr {
    b.lam(
        &["f", "xs"],
        b.gen_strict(b.block(
            vec![
                b.let_("n", b.app(b.global("List.Core", "length"), b.loc("xs"))),
                b.var_decl("i", b.int(0)),
            ],
            b.while_(
                b.bin(BinOp::Lt, b.var("i"), b.loc("n")),
                b.block(
                    vec![
                        b.let_(
                            "suf",
                            b.app3(
                                b.global("List.Core", "slice"),
                                b.var("i"),
                                b.loc("n"),
                                b.loc("xs"),
                            ),
                        ),
                        b.assign("i", b.add_i(b.var("i"), b.int(1))),
                    ],
                    b.yield_(b.app(b.loc("f"), b.loc("suf"))),
                ),
            ),
        )),
    )
}

fn partition_map(b: &ExprBuilder) -> Expr {
    b.lam(
        &["f", "xs"],
        b.block(
            vec![
                b.var_decl("ok_xs", b.list(vec![])),
                b.var_decl("err_xs", b.list(vec![])),
                Stmt::Expr(b.each(
                    "x",
                    b.loc("xs"),
                    b.match_(
                        b.app(b.loc("f"), b.loc("x")),
                        vec![
                            arm(
                                b.ok_pat("v"),
                                b.block(
                                    vec![b.assign(
                                        "ok_xs",
                                        b.bin(
                                            BinOp::ListConcat,
                                            b.var("ok_xs"),
                                            b.list(vec![b.loc("v")]),
                                        ),
                                    )],
                                    b.unit(),
                                ),
                            ),
                            arm(
                                b.err_pat("e"),
                                b.block(
                                    vec![b.assign(
                                        "err_xs",
                                        b.bin(
                                            BinOp::ListConcat,
                                            b.var("err_xs"),
                                            b.list(vec![b.loc("e")]),
                                        ),
                                    )],
                                    b.unit(),
                                ),
                            ),
                        ],
                    ),
                )),
            ],
            b.tuple(vec![b.var("ok_xs"), b.var("err_xs")]),
        ),
    )
}

fn unfold(b: &ExprBuilder) -> Expr {
    b.lam(
        &["seed", "f"],
        b.gen_strict(b.block(
            vec![b.var_decl("cur", b.loc("seed"))],
            b.at(ExprKind::ControlLoop {
                id: "$unfold".into(),
                condition: None,
                iterable: false,
                body: Box::new(b.match_(
                    b.app(b.loc("f"), b.var("cur")),
                    vec![
                        arm(
                            Pat::Con(
                                ExprBuilder::con("Prelude", "Option", "Some", 1),
                                vec![Pat::Tuple(vec![
                                    Pat::Bind(b.name("v")),
                                    Pat::Bind(b.name("next")),
                                ])],
                            ),
                            b.block(
                                vec![
                                    Stmt::Expr(b.yield_(b.loc("v"))),
                                    b.assign("cur", b.loc("next")),
                                ],
                                b.unit(),
                            ),
                        ),
                        arm(Pat::Wild, b.break_("$unfold", b.unit())),
                    ],
                )),
            }),
        )),
    )
}
