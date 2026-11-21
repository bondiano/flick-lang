//! Audited `Chunk.Ops` filtering, set, transform, and search templates.

use flick_ir::BinOp;

use super::expr_builder::{ExprBuilder, arm};
use super::std_chunk::{unwrap, wrap, wrap_pair};
use super::{Expr, Pat};

pub(super) fn template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    chunk_filter_ops(b, bind, stages)
        .or_else(|| chunk_set_ops(b, bind, stages))
        .or_else(|| chunk_search_ops(b, bind, stages))
}

fn chunk_filter_ops(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("filter", 2) => Some(b.lam(
            &["p", "c"],
            wrap(
                b,
                b.app2(
                    b.global("List.Core", "filter"),
                    b.loc("p"),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        ("filterMap", 2) => Some(b.lam(
            &["f", "c"],
            wrap(
                b,
                b.gen_strict(b.each(
                    "x",
                    unwrap(b, b.loc("c")),
                    b.match_(
                        b.app(b.loc("f"), b.loc("x")),
                        vec![
                            arm(b.ok_pat("v"), b.yield_(b.loc("v"))),
                            arm(Pat::Wild, b.unit()),
                        ],
                    ),
                )),
            ),
        )),
        ("filterMapWhile", 2) => Some(b.lam(
            &["f", "c"],
            wrap(
                b,
                b.app2(
                    b.global("List.Transform", "filter_map_while"),
                    b.loc("f"),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        ("compact", 1) => Some(b.lam(
            &["c"],
            wrap(
                b,
                b.app(
                    b.global("List.Transform", "get_somes"),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        ("flatMap", 2) => Some(b.lam(
            &["f", "c"],
            wrap(
                b,
                b.app2(
                    b.global("List.Core", "flat_map"),
                    b.lam(&["x"], unwrap(b, b.app(b.loc("f"), b.loc("x")))),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        ("flatten", 1) => Some(b.lam(
            &["c"],
            wrap(
                b,
                b.app(
                    b.global("List.Core", "concat"),
                    b.app2(
                        b.global("Instances", "map_list"),
                        b.lam(&["inner"], unwrap(b, b.loc("inner"))),
                        unwrap(b, b.loc("c")),
                    ),
                ),
            ),
        )),
        ("partition", 2) => Some(b.lam(
            &["f", "c"],
            wrap_pair(
                b,
                b.app2(
                    b.global("List.Transform", "partition_map"),
                    b.loc("f"),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        ("separate", 1) => Some(b.lam(
            &["c"],
            wrap_pair(
                b,
                b.app(
                    b.global("List.Transform", "separate"),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        _ => None,
    }
}

fn chunk_set_ops(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    let eq_fn = || b.lam(&["a", "b"], b.bin(BinOp::Eq, b.loc("a"), b.loc("b")));
    match (bind, stages) {
        ("dedupe", 1) => Some(b.lam(
            &["c"],
            wrap(
                b,
                b.app2(
                    b.global("List.Search", "dedupe_with"),
                    eq_fn(),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        ("dedupeAdjacent", 1) => Some(b.lam(
            &["c"],
            wrap(
                b,
                b.app2(
                    b.global("List.Search", "dedupe_adjacent_with"),
                    eq_fn(),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        ("differenceWith", 3) => Some(b.lam(
            &["eq", "that", "self"],
            wrap(
                b,
                b.app3(
                    b.global("List.Search", "difference_with"),
                    b.loc("eq"),
                    unwrap(b, b.loc("that")),
                    unwrap(b, b.loc("self")),
                ),
            ),
        )),
        ("intersection", 2) => Some(b.lam(
            &["that", "self"],
            wrap(
                b,
                b.app3(
                    b.global("List.Search", "intersection_with"),
                    eq_fn(),
                    unwrap(b, b.loc("that")),
                    unwrap(b, b.loc("self")),
                ),
            ),
        )),
        ("union", 2) => Some(b.lam(
            &["that", "self"],
            wrap(
                b,
                b.app3(
                    b.global("List.Search", "union_with"),
                    unwrap(b, b.loc("that")),
                    eq_fn(),
                    unwrap(b, b.loc("self")),
                ),
            ),
        )),
        _ => chunk_zip_ops(b, bind, stages),
    }
}

fn chunk_zip_ops(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("mapAccum", 3) => Some(b.lam(
            &["init", "f", "c"],
            b.match_(
                b.app3(
                    b.global("List.Transform", "map_accum"),
                    b.loc("init"),
                    b.loc("f"),
                    unwrap(b, b.loc("c")),
                ),
                vec![arm(
                    Pat::Tuple(vec![Pat::Bind(b.name("st")), Pat::Bind(b.name("ys"))]),
                    b.tuple(vec![b.loc("st"), wrap(b, b.loc("ys"))]),
                )],
            ),
        )),
        ("flickSortBy", 2) => Some(b.lam(
            &["cmp", "c"],
            wrap(
                b,
                b.app2(
                    b.global("List.Core", "sort_by"),
                    b.loc("cmp"),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        ("unzip", 1) => Some(b.lam(
            &["c"],
            wrap_pair(
                b,
                b.app(b.global("List.Core", "unzip"), unwrap(b, b.loc("c"))),
            ),
        )),
        ("zip", 2) => Some(b.lam(
            &["that", "self"],
            wrap(
                b,
                b.app2(
                    b.global("List.Core", "zip"),
                    unwrap(b, b.loc("self")),
                    unwrap(b, b.loc("that")),
                ),
            ),
        )),
        ("zipWith", 3) => Some(b.lam(
            &["that", "f", "self"],
            wrap(
                b,
                b.app3(
                    b.global("List.Transform", "zip_with"),
                    unwrap(b, b.loc("that")),
                    b.loc("f"),
                    unwrap(b, b.loc("self")),
                ),
            ),
        )),
        _ => None,
    }
}

fn chunk_search_ops(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("containsWith", 3) => Some(b.lam(
            &["eq", "x", "c"],
            b.app3(
                b.global("List.Search", "contains_with"),
                b.loc("eq"),
                b.loc("x"),
                unwrap(b, b.loc("c")),
            ),
        )),
        ("every", 2) => Some(b.lam(
            &["p", "c"],
            b.app2(
                b.global("List.Core", "all"),
                b.loc("p"),
                unwrap(b, b.loc("c")),
            ),
        )),
        ("some", 2) => Some(b.lam(
            &["p", "c"],
            b.app2(
                b.global("List.Core", "any"),
                b.loc("p"),
                unwrap(b, b.loc("c")),
            ),
        )),
        ("findFirst", 2) => Some(b.lam(
            &["p", "c"],
            b.app2(
                b.global("List.Core", "find"),
                b.loc("p"),
                unwrap(b, b.loc("c")),
            ),
        )),
        ("findFirstIndex", 2) => Some(b.lam(
            &["p", "c"],
            b.app2(
                b.global("List.Core", "find_index"),
                b.loc("p"),
                unwrap(b, b.loc("c")),
            ),
        )),
        ("findLast", 2) => Some(b.lam(
            &["p", "c"],
            b.app2(
                b.global("List.Search", "find_last"),
                b.loc("p"),
                unwrap(b, b.loc("c")),
            ),
        )),
        ("findLastIndex", 2) => Some(b.lam(
            &["p", "c"],
            b.app2(
                b.global("List.Search", "find_last_index"),
                b.loc("p"),
                unwrap(b, b.loc("c")),
            ),
        )),
        ("join", 2) => Some(b.lam(
            &["sep", "c"],
            b.app2(
                b.global("String", "join"),
                b.loc("sep"),
                unwrap(b, b.loc("c")),
            ),
        )),
        ("reduce", 3) => Some(b.lam(
            &["init", "f", "c"],
            b.app3(
                b.global("List.Core", "fold_left"),
                b.loc("f"),
                b.loc("init"),
                unwrap(b, b.loc("c")),
            ),
        )),
        ("reduceRight", 3) => Some(b.lam(
            &["init", "f", "c"],
            b.app3(
                b.global("List.Core", "fold_left"),
                b.loc("f"),
                b.loc("init"),
                b.app(b.global("List.Core", "reverse"), unwrap(b, b.loc("c"))),
            ),
        )),
        _ => None,
    }
}
