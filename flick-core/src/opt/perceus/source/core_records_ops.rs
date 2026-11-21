//! Closed pure templates for `flick:Record` partitioning, quantification, set, and subrecord ops.

use super::expr_builder::{ExprBuilder, arm};
use super::{Expr, Pat, Stmt};

pub(super) fn template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("partition", 2) => Some(record_partition(b, true)),
        ("separate", 1) => Some(record_partition(b, false)),
        ("getSuccesses", 1) => Some(record_results(b, true)),
        ("getFailures", 1) => Some(record_results(b, false)),
        ("every", 2) => Some(record_quant(b, false)),
        ("some", 2) => Some(record_quant(b, true)),
        ("union", 3) => Some(record_union(b)),
        ("intersection", 3) => Some(record_intersection(b)),
        ("isSubrecordBy", 3) => Some(record_subrecord(b, false)),
        ("flickMakeEquivalence", 3) => Some(record_subrecord(b, true)),
        _ => None,
    }
}

fn record_partition(b: &ExprBuilder, with_fn: bool) -> Expr {
    let entries = b.app(b.global("Record", "to_entries"), b.loc("r"));
    let kv_pat = Pat::Tuple(vec![Pat::Bind(b.name("k")), Pat::Bind(b.name("v"))]);
    let res = if with_fn {
        b.app2(b.loc("f"), b.loc("v"), b.loc("k"))
    } else {
        b.loc("v")
    };
    let body = b.block(
        vec![
            b.var_decl("ok_r", b.app(b.global("Record", "empty"), b.unit())),
            b.var_decl("err_r", b.app(b.global("Record", "empty"), b.unit())),
            Stmt::Expr(b.each(
                "pair",
                entries,
                b.match_(
                    b.loc("pair"),
                    vec![arm(
                        kv_pat,
                        b.match_(
                            res,
                            vec![
                                arm(
                                    b.ok_pat("y"),
                                    b.block(
                                        vec![b.assign(
                                            "ok_r",
                                            b.app3(
                                                b.global("Record", "set"),
                                                b.loc("k"),
                                                b.loc("y"),
                                                b.var("ok_r"),
                                            ),
                                        )],
                                        b.unit(),
                                    ),
                                ),
                                arm(
                                    b.err_pat("e"),
                                    b.block(
                                        vec![b.assign(
                                            "err_r",
                                            b.app3(
                                                b.global("Record", "set"),
                                                b.loc("k"),
                                                b.loc("e"),
                                                b.var("err_r"),
                                            ),
                                        )],
                                        b.unit(),
                                    ),
                                ),
                            ],
                        ),
                    )],
                ),
            )),
        ],
        b.tuple(vec![b.var("ok_r"), b.var("err_r")]),
    );
    if with_fn {
        b.lam(&["f", "r"], body)
    } else {
        b.lam(&["r"], body)
    }
}

fn record_results(b: &ExprBuilder, successes: bool) -> Expr {
    let entries = b.app(b.global("Record", "to_entries"), b.loc("r"));
    let kv_pat = Pat::Tuple(vec![Pat::Bind(b.name("k")), Pat::Bind(b.name("v"))]);
    let arm_pat = if successes {
        b.ok_pat("y")
    } else {
        b.err_pat("y")
    };
    b.lam(
        &["r"],
        b.app(
            b.global("Record", "from_entries"),
            b.gen_strict(b.each(
                "pair",
                entries,
                b.match_(
                    b.loc("pair"),
                    vec![arm(
                        kv_pat,
                        b.match_(
                            b.loc("v"),
                            vec![
                                arm(arm_pat, b.yield_(b.tuple(vec![b.loc("k"), b.loc("y")]))),
                                arm(Pat::Wild, b.unit()),
                            ],
                        ),
                    )],
                ),
            )),
        ),
    )
}

fn record_quant(b: &ExprBuilder, is_some: bool) -> Expr {
    let kv_pat = Pat::Tuple(vec![Pat::Bind(b.name("k")), Pat::Bind(b.name("v"))]);
    let pred = b.app2(b.loc("p"), b.loc("v"), b.loc("k"));
    let hit = if is_some { pred } else { b.not(pred) };
    b.lam(
        &["p", "r"],
        b.block(
            vec![
                b.var_decl("ans", b.bool_(!is_some)),
                Stmt::Expr(b.for_ctrl(
                    "$rq",
                    "pair",
                    b.app(b.global("Record", "to_entries"), b.loc("r")),
                    b.match_(
                        b.loc("pair"),
                        vec![arm(
                            kv_pat,
                            b.if_(
                                hit,
                                b.block(
                                    vec![b.assign("ans", b.bool_(is_some))],
                                    b.break_("$rq", b.unit()),
                                ),
                                b.unit(),
                            ),
                        )],
                    ),
                )),
            ],
            b.var("ans"),
        ),
    )
}

fn record_union(b: &ExprBuilder) -> Expr {
    let kv_pat = Pat::Tuple(vec![Pat::Bind(b.name("k")), Pat::Bind(b.name("v"))]);
    b.lam(
        &["that", "combine", "self"],
        b.block(
            vec![
                b.var_decl("out", b.loc("self")),
                Stmt::Expr(b.each(
                    "pair",
                    b.app(b.global("Record", "to_entries"), b.loc("that")),
                    b.match_(
                        b.loc("pair"),
                        vec![arm(
                            kv_pat,
                            b.block(
                                vec![
                                    b.let_(
                                        "val",
                                        b.match_(
                                            b.app2(
                                                b.global("Record", "get_key"),
                                                b.loc("k"),
                                                b.var("out"),
                                            ),
                                            vec![
                                                arm(
                                                    b.some_pat("prev"),
                                                    b.app2(
                                                        b.loc("combine"),
                                                        b.loc("prev"),
                                                        b.loc("v"),
                                                    ),
                                                ),
                                                arm(Pat::Wild, b.loc("v")),
                                            ],
                                        ),
                                    ),
                                    b.assign(
                                        "out",
                                        b.app3(
                                            b.global("Record", "set"),
                                            b.loc("k"),
                                            b.loc("val"),
                                            b.var("out"),
                                        ),
                                    ),
                                ],
                                b.unit(),
                            ),
                        )],
                    ),
                )),
            ],
            b.var("out"),
        ),
    )
}

fn record_intersection(b: &ExprBuilder) -> Expr {
    let kv_pat = Pat::Tuple(vec![Pat::Bind(b.name("k")), Pat::Bind(b.name("v"))]);
    b.lam(
        &["that", "combine", "self"],
        b.app(
            b.global("Record", "from_entries"),
            b.gen_strict(b.each(
                "pair",
                b.app(b.global("Record", "to_entries"), b.loc("self")),
                b.match_(
                    b.loc("pair"),
                    vec![arm(
                        kv_pat,
                        b.match_(
                            b.app2(b.global("Record", "get_key"), b.loc("k"), b.loc("that")),
                            vec![
                                arm(
                                    b.some_pat("tv"),
                                    b.yield_(b.tuple(vec![
                                        b.loc("k"),
                                        b.app2(b.loc("combine"), b.loc("v"), b.loc("tv")),
                                    ])),
                                ),
                                arm(Pat::Wild, b.unit()),
                            ],
                        ),
                    )],
                ),
            )),
        ),
    )
}

fn record_subrecord(b: &ExprBuilder, both: bool) -> Expr {
    let check_dir = |a_name: &str, b_name: &str, loop_id: &str| {
        let kv_pat = Pat::Tuple(vec![Pat::Bind(b.name("k")), Pat::Bind(b.name("v"))]);
        b.block(
            vec![
                b.var_decl("ok", b.bool_(true)),
                Stmt::Expr(b.for_ctrl(
                    loop_id,
                    "pair",
                    b.app(b.global("Record", "to_entries"), b.loc(a_name)),
                    b.match_(
                        b.loc("pair"),
                        vec![arm(
                            kv_pat,
                            b.match_(
                                b.app2(b.global("Record", "get_key"), b.loc("k"), b.loc(b_name)),
                                vec![
                                    arm(
                                        b.some_pat("tv"),
                                        b.if_(
                                            b.app2(b.loc("eq"), b.loc("v"), b.loc("tv")),
                                            b.unit(),
                                            b.block(
                                                vec![b.assign("ok", b.bool_(false))],
                                                b.break_(loop_id, b.unit()),
                                            ),
                                        ),
                                    ),
                                    arm(
                                        Pat::Wild,
                                        b.block(
                                            vec![b.assign("ok", b.bool_(false))],
                                            b.break_(loop_id, b.unit()),
                                        ),
                                    ),
                                ],
                            ),
                        )],
                    ),
                )),
            ],
            b.var("ok"),
        )
    };
    if both {
        b.lam(
            &["eq", "left", "right"],
            b.if_(
                check_dir("left", "right", "$rsub1"),
                check_dir("right", "left", "$rsub2"),
                b.bool_(false),
            ),
        )
    } else {
        b.lam(&["eq", "that", "self"], check_dir("self", "that", "$rsub"))
    }
}
