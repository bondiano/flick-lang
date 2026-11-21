//! Closed pure templates for `flick:Array` and `flick:Iterable` slicing, searching, and `NonEmpty` helpers.

use flick_ir::BinOp;

use super::expr_builder::{ExprBuilder, arm};
use super::{Expr, Pat, Span, Stmt};

pub(super) fn template(from: &str, bind: &str, stages: usize, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$arr");
    if from == "flick:Iterable" && bind == "flickFilterMapWhile" && stages == 2 {
        return Some(take_while_filter(&b));
    }
    if from != "flick:Array" {
        return None;
    }
    array_basic(&b, bind, stages)
        .or_else(|| array_match_nonempty(&b, bind, stages))
        .or_else(|| super::core_lists_ops::template(&b, bind, stages))
}

fn array_basic(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("takeWhile", 2) => Some(b.lam(
            &["p", "xs"],
            b.gen_strict(b.for_ctrl(
                "$tw",
                "x",
                b.loc("xs"),
                b.if_(
                    b.app(b.loc("p"), b.loc("x")),
                    b.yield_(b.loc("x")),
                    b.break_("$tw", b.unit()),
                ),
            )),
        )),
        ("dropWhile", 2) => Some(drop_while(b, false)),
        ("takeWhileFilter", 2) => Some(take_while_filter(b)),
        ("dropWhileFilter", 2) => Some(drop_while(b, true)),
        ("span", 2) => Some(span_where(b, false)),
        ("splitWhere", 2) => Some(span_where(b, true)),
        ("chop", 2) => Some(chop(b)),
        ("modify", 3) => Some(modify_at(b)),
        ("findLast", 2) => Some(find_last(b, false)),
        ("findLastIndex", 2) => Some(find_last(b, true)),
        ("findFirstWithIndex", 2) => Some(find_first_with_index(b)),
        ("countBy", 2) => Some(count_by(b)),
        ("liftOption", 2) => Some(b.lam(
            &["f", "a"],
            b.match_(
                b.app(b.loc("f"), b.loc("a")),
                vec![
                    arm(b.some_pat("v"), b.list(vec![b.loc("v")])),
                    arm(Pat::Wild, b.list(vec![])),
                ],
            ),
        )),
        ("liftPredicate", 2) => Some(b.lam(
            &["p", "a"],
            b.if_(
                b.app(b.loc("p"), b.loc("a")),
                b.list(vec![b.loc("a")]),
                b.list(vec![]),
            ),
        )),
        ("liftResult", 2) => Some(b.lam(
            &["f", "a"],
            b.match_(
                b.app(b.loc("f"), b.loc("a")),
                vec![
                    arm(b.ok_pat("v"), b.list(vec![b.loc("v")])),
                    arm(Pat::Wild, b.list(vec![])),
                ],
            ),
        )),
        _ => None,
    }
}

fn array_match_nonempty(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("match", 2) => Some(b.lam(
            &["cases", "xs"],
            b.if_(
                b.bin(
                    BinOp::Eq,
                    b.app(b.global("List.Core", "length"), b.loc("xs")),
                    b.int(0),
                ),
                b.app(b.field(b.loc("cases"), "onEmpty"), b.unit()),
                b.app(b.field(b.loc("cases"), "onNonEmpty"), b.loc("xs")),
            ),
        )),
        ("flickMatchLeft", 2) => Some(b.lam(
            &["cases", "xs"],
            b.match_(
                b.loc("xs"),
                vec![
                    arm(
                        Pat::List(vec![], None),
                        b.app(b.field(b.loc("cases"), "onEmpty"), b.unit()),
                    ),
                    arm(
                        Pat::List(
                            vec![Pat::Bind(b.name("hd"))],
                            Some(Box::new(Pat::Bind(b.name("tl")))),
                        ),
                        b.app2(
                            b.field(b.loc("cases"), "onNonEmpty"),
                            b.loc("hd"),
                            b.loc("tl"),
                        ),
                    ),
                ],
            ),
        )),
        ("flickMatchRight", 2) => Some(match_right(b)),
        ("makeBy", 2) => Some(b.lam(
            &["n", "f"],
            b.app2(
                b.global("List.Core", "initialize"),
                b.if_(b.bin(BinOp::Lt, b.loc("n"), b.int(1)), b.int(1), b.loc("n")),
                b.loc("f"),
            ),
        )),
        ("modifyHeadNonEmpty", 2) => Some(b.lam(
            &["f", "ne"],
            b.app2(
                b.global("NonEmpty", "set_head"),
                b.app(b.loc("f"), b.app(b.global("NonEmpty", "head"), b.loc("ne"))),
                b.loc("ne"),
            ),
        )),
        ("modifyLastNonEmpty", 2) => Some(b.lam(
            &["f", "ne"],
            b.app2(
                b.global("NonEmpty", "set_last"),
                b.app(b.loc("f"), b.app(b.global("NonEmpty", "last"), b.loc("ne"))),
                b.loc("ne"),
            ),
        )),
        ("groupWith", 2) => Some(group_with(b)),
        ("flickMax", 2) => Some(non_empty_extremum(b, "GT")),
        ("flickMin", 2) => Some(non_empty_extremum(b, "LT")),
        _ => None,
    }
}

fn take_while_filter(b: &ExprBuilder) -> Expr {
    b.lam(
        &["f", "xs"],
        b.gen_strict(b.for_ctrl(
            "$twf",
            "x",
            b.loc("xs"),
            b.match_(
                b.app(b.loc("f"), b.loc("x")),
                vec![
                    arm(b.ok_pat("v"), b.yield_(b.loc("v"))),
                    arm(Pat::Wild, b.break_("$twf", b.unit())),
                ],
            ),
        )),
    )
}

fn match_right(b: &ExprBuilder) -> Expr {
    b.lam(
        &["cases", "xs"],
        b.match_(
            b.app(b.global("NonEmpty", "from_list"), b.loc("xs")),
            vec![
                arm(
                    b.some_pat("ne"),
                    b.match_(
                        b.app(b.global("NonEmpty", "unappend"), b.loc("ne")),
                        vec![arm(
                            Pat::Tuple(vec![Pat::Bind(b.name("init")), Pat::Bind(b.name("last"))]),
                            b.app2(
                                b.field(b.loc("cases"), "onNonEmpty"),
                                b.loc("init"),
                                b.loc("last"),
                            ),
                        )],
                    ),
                ),
                arm(
                    Pat::Wild,
                    b.app(b.field(b.loc("cases"), "onEmpty"), b.unit()),
                ),
            ],
        ),
    )
}

fn drop_while(b: &ExprBuilder, is_result: bool) -> Expr {
    let keep_dropping = if is_result {
        b.match_(
            b.app(b.loc("p"), b.loc("x")),
            vec![
                arm(b.ok_pat("_v"), b.bool_(true)),
                arm(Pat::Wild, b.bool_(false)),
            ],
        )
    } else {
        b.app(b.loc("p"), b.loc("x"))
    };
    b.lam(
        &["p", "xs"],
        b.gen_strict(b.block(
            vec![b.var_decl("drop", b.bool_(true))],
            b.each(
                "x",
                b.loc("xs"),
                b.if_(
                    b.var("drop"),
                    b.if_(
                        keep_dropping,
                        b.unit(),
                        b.block(vec![b.assign("drop", b.bool_(false))], b.yield_(b.loc("x"))),
                    ),
                    b.yield_(b.loc("x")),
                ),
            ),
        )),
    )
}

fn span_where(b: &ExprBuilder, invert: bool) -> Expr {
    let pred = b.app(b.loc("p"), b.loc("x"));
    let stop = if invert { pred } else { b.not(pred) };
    b.lam(
        &["p", "xs"],
        b.block(
            vec![
                b.var_decl("idx", b.int(0)),
                Stmt::Expr(b.for_ctrl(
                    "$span",
                    "x",
                    b.loc("xs"),
                    b.if_(
                        stop,
                        b.break_("$span", b.unit()),
                        b.block(
                            vec![b.assign("idx", b.add_i(b.var("idx"), b.int(1)))],
                            b.unit(),
                        ),
                    ),
                )),
            ],
            b.app2(
                b.global("List.Slice", "split_at"),
                b.var("idx"),
                b.loc("xs"),
            ),
        ),
    )
}

fn chop(b: &ExprBuilder) -> Expr {
    b.lam(
        &["f", "xs"],
        b.gen_strict(b.block(
            vec![b.var_decl("cur", b.loc("xs"))],
            b.while_(
                b.bin(
                    BinOp::Gt,
                    b.app(b.global("List.Core", "length"), b.var("cur")),
                    b.int(0),
                ),
                b.match_(
                    b.app(b.loc("f"), b.var("cur")),
                    vec![arm(
                        Pat::Tuple(vec![Pat::Bind(b.name("val")), Pat::Bind(b.name("rest"))]),
                        b.block(
                            vec![
                                Stmt::Expr(b.yield_(b.loc("val"))),
                                b.assign("cur", b.loc("rest")),
                            ],
                            b.unit(),
                        ),
                    )],
                ),
            ),
        )),
    )
}

fn modify_at(b: &ExprBuilder) -> Expr {
    b.lam(
        &["i", "f", "xs"],
        b.match_(
            b.app2(b.global("List.Core", "get"), b.loc("i"), b.loc("xs")),
            vec![
                arm(
                    b.some_pat("v"),
                    b.some(b.app3(
                        b.global("List.Core", "set"),
                        b.loc("i"),
                        b.app(b.loc("f"), b.loc("v")),
                        b.loc("xs"),
                    )),
                ),
                arm(Pat::Wild, b.none()),
            ],
        ),
    )
}

fn find_last(b: &ExprBuilder, index_only: bool) -> Expr {
    let val = if index_only { b.var("i") } else { b.loc("x") };
    b.lam(
        &["p", "xs"],
        b.block(
            vec![
                b.var_decl(
                    "i",
                    b.sub_i(
                        b.app(b.global("List.Core", "length"), b.loc("xs")),
                        b.int(1),
                    ),
                ),
                b.var_decl("found", b.none()),
                Stmt::Expr(b.for_ctrl(
                    "$flast",
                    "x",
                    b.app(b.global("List.Core", "reverse"), b.loc("xs")),
                    b.if_(
                        b.app(b.loc("p"), b.loc("x")),
                        b.block(
                            vec![b.assign("found", b.some(val))],
                            b.break_("$flast", b.unit()),
                        ),
                        b.block(vec![b.assign("i", b.sub_i(b.var("i"), b.int(1)))], b.unit()),
                    ),
                )),
            ],
            b.var("found"),
        ),
    )
}

fn find_first_with_index(b: &ExprBuilder) -> Expr {
    b.lam(
        &["p", "xs"],
        b.block(
            vec![
                b.var_decl("i", b.int(0)),
                b.var_decl("found", b.none()),
                Stmt::Expr(b.for_ctrl(
                    "$ffwi",
                    "x",
                    b.loc("xs"),
                    b.if_(
                        b.app(b.loc("p"), b.loc("x")),
                        b.block(
                            vec![b.assign("found", b.some(b.tuple(vec![b.loc("x"), b.var("i")])))],
                            b.break_("$ffwi", b.unit()),
                        ),
                        b.block(vec![b.assign("i", b.add_i(b.var("i"), b.int(1)))], b.unit()),
                    ),
                )),
            ],
            b.var("found"),
        ),
    )
}

fn count_by(b: &ExprBuilder) -> Expr {
    b.lam(
        &["p", "xs"],
        b.block(
            vec![
                b.var_decl("n", b.int(0)),
                Stmt::Expr(b.each(
                    "x",
                    b.loc("xs"),
                    b.if_(
                        b.app(b.loc("p"), b.loc("x")),
                        b.block(vec![b.assign("n", b.add_i(b.var("n"), b.int(1)))], b.unit()),
                        b.unit(),
                    ),
                )),
            ],
            b.var("n"),
        ),
    )
}

fn group_with(b: &ExprBuilder) -> Expr {
    b.lam(
        &["same", "ne"],
        b.gen_strict(b.block(
            vec![
                b.let_("first", b.app(b.global("NonEmpty", "head"), b.loc("ne"))),
                b.var_decl("leader", b.loc("first")),
                b.var_decl("cur", b.list(vec![b.loc("first")])),
                Stmt::Expr(b.each(
                    "x",
                    b.app(b.global("NonEmpty", "tail"), b.loc("ne")),
                    b.if_(
                        b.app2(b.loc("same"), b.var("leader"), b.loc("x")),
                        b.block(
                            vec![b.assign(
                                "cur",
                                b.bin(BinOp::ListConcat, b.var("cur"), b.list(vec![b.loc("x")])),
                            )],
                            b.unit(),
                        ),
                        b.block(
                            vec![
                                Stmt::Expr(b.yield_(b.var("cur"))),
                                b.assign("leader", b.loc("x")),
                                b.assign("cur", b.list(vec![b.loc("x")])),
                            ],
                            b.unit(),
                        ),
                    ),
                )),
            ],
            b.yield_(b.var("cur")),
        )),
    )
}

fn non_empty_extremum(b: &ExprBuilder, target_ord: &str) -> Expr {
    b.lam(
        &["cmp", "ne"],
        b.block(
            vec![
                b.var_decl("best", b.app(b.global("NonEmpty", "head"), b.loc("ne"))),
                Stmt::Expr(b.each(
                    "x",
                    b.app(b.global("NonEmpty", "tail"), b.loc("ne")),
                    b.if_(
                        b.bin(
                            BinOp::Eq,
                            b.app2(b.loc("cmp"), b.loc("x"), b.var("best")),
                            b.ord(target_ord),
                        ),
                        b.block(vec![b.assign("best", b.loc("x"))], b.unit()),
                        b.unit(),
                    ),
                )),
            ],
            b.var("best"),
        ),
    )
}

pub(super) fn option_product_many(b: &ExprBuilder) -> Expr {
    b.lam(
        &["others", "self"],
        b.match_(
            b.loc("self"),
            vec![
                arm(
                    b.some_pat("first"),
                    b.block(
                        vec![
                            b.var_decl("ok", b.bool_(true)),
                            b.let_(
                                "rest",
                                b.gen_strict(b.block(
                                    vec![Stmt::Expr(b.yield_(b.loc("first")))],
                                    b.for_ctrl(
                                        "$opt_pm",
                                        "o",
                                        b.loc("others"),
                                        b.match_(
                                            b.loc("o"),
                                            vec![
                                                arm(b.some_pat("v"), b.yield_(b.loc("v"))),
                                                arm(
                                                    Pat::Wild,
                                                    b.block(
                                                        vec![b.assign("ok", b.bool_(false))],
                                                        b.break_("$opt_pm", b.unit()),
                                                    ),
                                                ),
                                            ],
                                        ),
                                    ),
                                )),
                            ),
                        ],
                        b.if_(b.var("ok"), b.some(b.loc("rest")), b.none()),
                    ),
                ),
                arm(Pat::Wild, b.none()),
            ],
        ),
    )
}

pub(super) fn result_all(b: &ExprBuilder) -> Expr {
    b.lam(
        &["xs"],
        b.block(
            vec![
                b.var_decl("failed", b.none()),
                b.let_(
                    "vals",
                    b.gen_strict(b.for_ctrl(
                        "$res_all",
                        "r",
                        b.loc("xs"),
                        b.match_(
                            b.loc("r"),
                            vec![
                                arm(b.ok_pat("v"), b.yield_(b.loc("v"))),
                                arm(
                                    b.err_pat("e"),
                                    b.block(
                                        vec![b.assign("failed", b.some(b.loc("e")))],
                                        b.break_("$res_all", b.unit()),
                                    ),
                                ),
                            ],
                        ),
                    )),
                ),
            ],
            b.match_(
                b.var("failed"),
                vec![
                    arm(b.some_pat("e"), b.err(b.loc("e"))),
                    arm(Pat::Wild, b.ok(b.loc("vals"))),
                ],
            ),
        ),
    )
}
