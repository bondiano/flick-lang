//! Higher-order `HashMap` bulk, mapping, filtering, and query templates.

use super::expr_builder::{ExprBuilder, arm};
use super::std_hash_map::{make_map, unwrap_map};
use super::{Expr, Pat};

pub(super) fn template(b: &ExprBuilder, from: &str, bind: &str, stages: usize) -> Option<Expr> {
    bulk_template(b, from, bind, stages).or_else(|| map_filter_template(b, from, bind, stages))
}

fn entry_pat(b: &ExprBuilder) -> Pat {
    Pat::Tuple(vec![
        Pat::Bind(b.name("eh")),
        Pat::Bind(b.name("ek")),
        Pat::Bind(b.name("ev")),
    ])
}

fn transform_entries(b: &ExprBuilder, body: Expr) -> Expr {
    unwrap_map(
        b,
        b.loc("m"),
        "w",
        "es",
        make_map(
            b,
            b.loc("w"),
            b.gen_strict(b.each(
                "ent",
                b.loc("es"),
                b.match_(b.loc("ent"), vec![arm(entry_pat(b), body)]),
            )),
        ),
    )
}

fn bulk_template(b: &ExprBuilder, from: &str, bind: &str, stages: usize) -> Option<Expr> {
    let fold_pairs = |init: Expr, pairs: Expr| {
        b.app3(
            b.global("List.Core", "fold_left"),
            b.lam(
                &["acc", "pair"],
                b.match_(
                    b.loc("pair"),
                    vec![arm(
                        Pat::Tuple(vec![Pat::Bind(b.name("pk")), Pat::Bind(b.name("pv"))]),
                        b.app3(
                            b.global("HashMap", "insert"),
                            b.loc("pk"),
                            b.loc("pv"),
                            b.loc("acc"),
                        ),
                    )],
                ),
            ),
            init,
            pairs,
        )
    };
    match (from, bind, stages) {
        ("flick:rt", "hashMapUnion", 2) => Some(b.lam(
            &["a", "b"],
            fold_pairs(
                b.loc("a"),
                b.app(b.global("HashMap", "to_list"), b.loc("b")),
            ),
        )),
        ("flick:rt", "hashMapRehash", 2) => Some(b.lam(
            &["w", "m"],
            fold_pairs(
                make_map(b, b.loc("w"), b.list(vec![])),
                b.app(b.global("HashMap", "to_list"), b.loc("m")),
            ),
        )),
        ("flick:HashMap", "flickFromList", 2) => Some(b.lam(
            &["w", "pairs"],
            fold_pairs(make_map(b, b.loc("w"), b.list(vec![])), b.loc("pairs")),
        )),
        ("flick:rt", "hashMapFromSeq", 2) => Some(b.lam(
            &["w", "pairs"],
            b.app3(
                b.global("Seq", "fold"),
                b.lam(
                    &["acc", "pair"],
                    b.match_(
                        b.loc("pair"),
                        vec![arm(
                            Pat::Tuple(vec![Pat::Bind(b.name("pk")), Pat::Bind(b.name("pv"))]),
                            b.app3(
                                b.global("HashMap", "insert"),
                                b.loc("pk"),
                                b.loc("pv"),
                                b.loc("acc"),
                            ),
                        )],
                    ),
                ),
                make_map(b, b.loc("w"), b.list(vec![])),
                b.loc("pairs"),
            ),
        )),
        ("flick:HashMap", "flickSetMany", 2) => {
            Some(b.lam(&["pairs", "m"], fold_pairs(b.loc("m"), b.loc("pairs"))))
        }
        ("flick:HashMap", "flickRemoveMany", 2) => Some(b.lam(
            &["ks", "m"],
            b.app3(
                b.global("List.Core", "fold_left"),
                b.lam(
                    &["acc", "k"],
                    b.app2(b.global("HashMap", "remove"), b.loc("k"), b.loc("acc")),
                ),
                b.loc("m"),
                b.loc("ks"),
            ),
        )),
        ("flick:HashMap", "flickModify", 3) => Some(b.lam(
            &["k", "f", "m"],
            b.app3(
                b.global("HashMap", "modify_at"),
                b.loc("k"),
                b.lam(
                    &["opt"],
                    b.match_(
                        b.loc("opt"),
                        vec![
                            arm(b.some_pat("v"), b.some(b.app(b.loc("f"), b.loc("v")))),
                            arm(Pat::Wild, b.none()),
                        ],
                    ),
                ),
                b.loc("m"),
            ),
        )),
        _ => None,
    }
}

fn map_filter_template(b: &ExprBuilder, from: &str, bind: &str, stages: usize) -> Option<Expr> {
    if from != "flick:HashMap" {
        return None;
    }
    match (bind, stages) {
        ("flickMap", 2) => Some(b.lam(
            &["f", "m"],
            transform_entries(
                b,
                b.yield_(b.tuple(vec![
                    b.loc("eh"),
                    b.loc("ek"),
                    b.app2(b.loc("f"), b.loc("ev"), b.loc("ek")),
                ])),
            ),
        )),
        ("flickFlatMap", 2) => Some(b.lam(
            &["f", "m"],
            unwrap_map(
                b,
                b.loc("m"),
                "w",
                "es",
                b.app3(
                    b.global("List.Core", "fold_left"),
                    b.lam(
                        &["acc", "ent"],
                        b.match_(
                            b.loc("ent"),
                            vec![arm(
                                entry_pat(b),
                                b.app2(
                                    b.global("HashMap", "union"),
                                    b.loc("acc"),
                                    b.app2(b.loc("f"), b.loc("ev"), b.loc("ek")),
                                ),
                            )],
                        ),
                    ),
                    make_map(b, b.loc("w"), b.list(vec![])),
                    b.loc("es"),
                ),
            ),
        )),
        ("flickReduce", 3) => Some(b.lam(
            &["z", "f", "m"],
            unwrap_map(
                b,
                b.loc("m"),
                "_w",
                "es",
                b.app3(
                    b.global("List.Core", "fold_left"),
                    b.lam(
                        &["acc", "ent"],
                        b.match_(
                            b.loc("ent"),
                            vec![arm(
                                entry_pat(b),
                                b.app3(b.loc("f"), b.loc("acc"), b.loc("ev"), b.loc("ek")),
                            )],
                        ),
                    ),
                    b.loc("z"),
                    b.loc("es"),
                ),
            ),
        )),
        _ => filter_entries_template(b, bind, stages),
    }
}

fn filter_entries_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickFilter", 2) => Some(b.lam(
            &["f", "m"],
            transform_entries(
                b,
                b.if_(
                    b.app2(b.loc("f"), b.loc("ev"), b.loc("ek")),
                    b.yield_(b.tuple(vec![b.loc("eh"), b.loc("ek"), b.loc("ev")])),
                    b.unit(),
                ),
            ),
        )),
        ("flickCompact", 1) => Some(b.lam(
            &["m"],
            transform_entries(
                b,
                b.match_(
                    b.loc("ev"),
                    vec![
                        arm(
                            b.some_pat("y"),
                            b.yield_(b.tuple(vec![b.loc("eh"), b.loc("ek"), b.loc("y")])),
                        ),
                        arm(Pat::Wild, b.unit()),
                    ],
                ),
            ),
        )),
        ("flickFilterMap", 2) => Some(b.lam(
            &["f", "m"],
            transform_entries(
                b,
                b.match_(
                    b.app2(b.loc("f"), b.loc("ev"), b.loc("ek")),
                    vec![
                        arm(
                            b.ok_pat("y"),
                            b.yield_(b.tuple(vec![b.loc("eh"), b.loc("ek"), b.loc("y")])),
                        ),
                        arm(Pat::Wild, b.unit()),
                    ],
                ),
            ),
        )),
        _ => query_template(b, bind, stages),
    }
}

fn query_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickFindFirst", 2) => Some(b.lam(
            &["f", "m"],
            b.app2(
                b.global("List.Core", "find"),
                b.lam(
                    &["pair"],
                    b.match_(
                        b.loc("pair"),
                        vec![arm(
                            Pat::Tuple(vec![Pat::Bind(b.name("pk")), Pat::Bind(b.name("pv"))]),
                            b.app2(b.loc("f"), b.loc("pv"), b.loc("pk")),
                        )],
                    ),
                ),
                b.app(b.global("HashMap", "to_list"), b.loc("m")),
            ),
        )),
        ("flickSome" | "flickHasBy", 2) => Some(b.lam(
            &["f", "m"],
            b.match_(
                b.app2(b.global("HashMap", "find_first"), b.loc("f"), b.loc("m")),
                vec![
                    arm(b.some_pat("_p"), b.bool_(true)),
                    arm(Pat::Wild, b.bool_(false)),
                ],
            ),
        )),
        ("flickEvery", 2) => Some(b.lam(
            &["f", "m"],
            b.app2(
                b.global("List.Core", "all"),
                b.lam(
                    &["pair"],
                    b.match_(
                        b.loc("pair"),
                        vec![arm(
                            Pat::Tuple(vec![Pat::Bind(b.name("pk")), Pat::Bind(b.name("pv"))]),
                            b.app2(b.loc("f"), b.loc("pv"), b.loc("pk")),
                        )],
                    ),
                ),
                b.app(b.global("HashMap", "to_list"), b.loc("m")),
            ),
        )),
        _ => None,
    }
}
