//! Audited `HashSet` pure IR adapters built directly over `HashMap w a Unit`.

use super::expr_builder::{ExprBuilder, arm};
use super::std_hash_map::{make_map, witness_from_dict};
use super::{Expr, ExprKind, Pat};

fn set_con() -> flick_ir::Constructor {
    ExprBuilder::con("HashSet", "HashSet", "$Set", 1)
}

fn wrap(b: &ExprBuilder, map: Expr) -> Expr {
    b.app(b.at(ExprKind::Con(set_con())), map)
}

fn unwrap(b: &ExprBuilder, s: Expr) -> Expr {
    b.match_(
        s,
        vec![arm(
            Pat::Con(set_con(), vec![Pat::Bind(b.name("m"))]),
            b.loc("m"),
        )],
    )
}

pub(super) fn empty_from_dict(b: &ExprBuilder) -> Expr {
    b.lam(
        &["hdict"],
        wrap(
            b,
            make_map(b, witness_from_dict(b, "hdict"), b.list(vec![])),
        ),
    )
}

pub(super) fn plain_template(
    b: &ExprBuilder,
    from: &str,
    bind: &str,
    stages: usize,
) -> Option<Expr> {
    match from {
        "flick:rt" => hash_set_rt(b, bind, stages),
        "flick:HashSet" => hash_set_ops(b, bind, stages),
        _ => None,
    }
}

fn hash_set_rt(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("hashSetWith", 1) => Some(b.lam(
            &["w"],
            wrap(b, b.app(b.global("HashMap", "with"), b.loc("w"))),
        )),
        ("hashSetFromList", 2) => Some(b.lam(
            &["w", "xs"],
            wrap(
                b,
                b.app3(
                    b.global("List.Core", "fold_left"),
                    b.lam(
                        &["acc", "x"],
                        b.app3(
                            b.global("HashMap", "insert"),
                            b.loc("x"),
                            b.unit(),
                            b.loc("acc"),
                        ),
                    ),
                    b.app(b.global("HashMap", "with"), b.loc("w")),
                    b.loc("xs"),
                ),
            ),
        )),
        ("hashSetInsert", 2) => Some(b.lam(
            &["x", "s"],
            wrap(
                b,
                b.app3(
                    b.global("HashMap", "insert"),
                    b.loc("x"),
                    b.unit(),
                    unwrap(b, b.loc("s")),
                ),
            ),
        )),
        ("hashSetRemove", 2) => Some(b.lam(
            &["x", "s"],
            wrap(
                b,
                b.app2(
                    b.global("HashMap", "remove"),
                    b.loc("x"),
                    unwrap(b, b.loc("s")),
                ),
            ),
        )),
        ("hashSetMember", 2) => Some(b.lam(
            &["x", "s"],
            b.app2(
                b.global("HashMap", "has"),
                b.loc("x"),
                unwrap(b, b.loc("s")),
            ),
        )),
        ("hashSetSize", 1) => Some(b.lam(
            &["s"],
            b.app(b.global("HashMap", "size"), unwrap(b, b.loc("s"))),
        )),
        ("hashSetToList", 1) => Some(b.lam(
            &["s"],
            b.app(b.global("HashMap", "keys"), unwrap(b, b.loc("s"))),
        )),
        ("hashSetUnion", 2) => Some(b.lam(
            &["a", "b"],
            wrap(
                b,
                b.app2(
                    b.global("HashMap", "union"),
                    unwrap(b, b.loc("a")),
                    unwrap(b, b.loc("b")),
                ),
            ),
        )),
        ("hashSetRehash", 2) => Some(b.lam(
            &["w", "s"],
            wrap(
                b,
                b.app2(
                    b.global("HashMap", "rehash"),
                    b.loc("w"),
                    unwrap(b, b.loc("s")),
                ),
            ),
        )),
        _ => None,
    }
}

fn hash_set_ops(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickIsEmpty", 1) => Some(b.lam(
            &["s"],
            b.app(b.global("HashMap", "is_empty"), unwrap(b, b.loc("s"))),
        )),
        ("flickIntersection", 2) => Some(b.lam(
            &["that", "self"],
            wrap(
                b,
                b.app2(
                    b.global("HashMap", "filter"),
                    b.lam(
                        &["_u", "k"],
                        b.app2(b.global("HashSet", "member"), b.loc("k"), b.loc("that")),
                    ),
                    unwrap(b, b.loc("self")),
                ),
            ),
        )),
        ("flickDifference", 2) => Some(b.lam(
            &["that", "self"],
            wrap(
                b,
                b.app2(
                    b.global("HashMap", "filter"),
                    b.lam(
                        &["_u", "k"],
                        b.not(b.app2(b.global("HashSet", "member"), b.loc("k"), b.loc("that"))),
                    ),
                    unwrap(b, b.loc("self")),
                ),
            ),
        )),
        ("flickIsSubset", 2) => Some(b.lam(
            &["that", "self"],
            b.app2(
                b.global("HashMap", "every"),
                b.lam(
                    &["_u", "k"],
                    b.app2(b.global("HashSet", "member"), b.loc("k"), b.loc("that")),
                ),
                unwrap(b, b.loc("self")),
            ),
        )),
        ("flickMap", 3) => Some(b.lam(
            &["w", "f", "s"],
            b.app2(
                b.global("HashSet", "from_list"),
                b.loc("w"),
                b.app2(
                    b.global("Instances", "map_list"),
                    b.loc("f"),
                    b.app(b.global("HashSet", "to_list"), b.loc("s")),
                ),
            ),
        )),
        ("flickFilter", 2) => Some(b.lam(
            &["p", "s"],
            wrap(
                b,
                b.app2(
                    b.global("HashMap", "filter"),
                    b.lam(&["_u", "k"], b.app(b.loc("p"), b.loc("k"))),
                    unwrap(b, b.loc("s")),
                ),
            ),
        )),
        ("flickSome", 2) => Some(b.lam(
            &["p", "s"],
            b.app2(
                b.global("HashMap", "some"),
                b.lam(&["_u", "k"], b.app(b.loc("p"), b.loc("k"))),
                unwrap(b, b.loc("s")),
            ),
        )),
        ("flickEvery", 2) => Some(b.lam(
            &["p", "s"],
            b.app2(
                b.global("HashMap", "every"),
                b.lam(&["_u", "k"], b.app(b.loc("p"), b.loc("k"))),
                unwrap(b, b.loc("s")),
            ),
        )),
        ("flickReduce", 3) => Some(b.lam(
            &["z", "f", "s"],
            b.app3(
                b.global("HashMap", "reduce"),
                b.loc("z"),
                b.lam(
                    &["acc", "_u", "k"],
                    b.app2(b.loc("f"), b.loc("acc"), b.loc("k")),
                ),
                unwrap(b, b.loc("s")),
            ),
        )),
        _ => None,
    }
}
