//! Audited `Chunk.Core` pure IR adapters over `Chunk.Core.$Chunk(List a)`.

use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, ExprKind, Local, Lower, LowerError, Pat, Span};

impl Lower<'_> {
    pub(super) fn std_chunk_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.mode != ForeignMode::Pure || value.dicts != 0 {
            return None;
        }
        let from = value.binding.from.as_deref()?;
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();

        let is_make = from == "flick:Chunk" && bind == "make" && value.adapter.variadic;
        if !is_make && !has_standard_shape(value) {
            return None;
        }
        if is_make
            && (value.catch.is_some()
                || value.adapter.receiver
                || value.adapter.cancel
                || !value.adapter.optional.is_empty()
                || value.adapter.result.nullable)
        {
            return None;
        }
        let b = ExprBuilder::new(span, "$chk");
        let expr = chunk_core_rt(&b, from, bind, stages).or_else(|| {
            (from == "flick:Chunk")
                .then(|| {
                    chunk_core_build(&b, bind, stages)
                        .or_else(|| chunk_core_slice(&b, bind, stages))
                        .or_else(|| super::std_chunk_ops::template(&b, bind, stages))
                })
                .flatten()
        })?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

pub(super) fn wrap(b: &ExprBuilder, xs: Expr) -> Expr {
    b.app(
        b.at(ExprKind::Con(ExprBuilder::con(
            "Chunk.Core",
            "Chunk",
            "$Chunk",
            1,
        ))),
        xs,
    )
}

pub(super) fn unwrap(b: &ExprBuilder, c: Expr) -> Expr {
    b.match_(
        c,
        vec![arm(
            Pat::Con(
                ExprBuilder::con("Chunk.Core", "Chunk", "$Chunk", 1),
                vec![Pat::Bind(b.name("xs"))],
            ),
            b.loc("xs"),
        )],
    )
}

pub(super) fn wrap_pair(b: &ExprBuilder, pair: Expr) -> Expr {
    b.match_(
        pair,
        vec![arm(
            Pat::Tuple(vec![Pat::Bind(b.name("l")), Pat::Bind(b.name("r"))]),
            b.tuple(vec![wrap(b, b.loc("l")), wrap(b, b.loc("r"))]),
        )],
    )
}

fn wrap_opt(b: &ExprBuilder, opt: Expr) -> Expr {
    b.match_(
        opt,
        vec![
            arm(b.some_pat("v"), b.some(wrap(b, b.loc("v")))),
            arm(Pat::Wild, b.none()),
        ],
    )
}

fn chunk_core_rt(b: &ExprBuilder, from: &str, bind: &str, stages: usize) -> Option<Expr> {
    if from != "flick:rt" {
        return None;
    }
    match (bind, stages) {
        ("chunkEmpty", 0) => Some(wrap(b, b.list(vec![]))),
        ("chunkFromList", 1) => Some(b.lam(&["xs"], wrap(b, b.loc("xs")))),
        ("chunkToList", 1) => Some(b.lam(&["c"], unwrap(b, b.loc("c")))),
        ("chunkAppend", 2) => Some(b.lam(
            &["x", "c"],
            wrap(
                b,
                b.bin(
                    BinOp::ListConcat,
                    unwrap(b, b.loc("c")),
                    b.list(vec![b.loc("x")]),
                ),
            ),
        )),
        ("chunkConcat", 2) => Some(b.lam(
            &["c", "d"],
            wrap(
                b,
                b.bin(
                    BinOp::ListConcat,
                    unwrap(b, b.loc("c")),
                    unwrap(b, b.loc("d")),
                ),
            ),
        )),
        ("chunkSize", 1) => Some(b.lam(
            &["c"],
            b.app(b.global("List.Core", "length"), unwrap(b, b.loc("c"))),
        )),
        ("chunkGet", 2) => Some(b.lam(
            &["i", "c"],
            b.app2(
                b.global("List.Core", "get"),
                b.loc("i"),
                unwrap(b, b.loc("c")),
            ),
        )),
        ("chunkMap", 2) => Some(b.lam(
            &["f", "c"],
            wrap(
                b,
                b.app2(
                    b.global("Instances", "map_list"),
                    b.loc("f"),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        _ => None,
    }
}

fn chunk_core_build(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickNonEmpty", 1) => Some(b.lam(
            &["c"],
            wrap_opt(
                b,
                b.app(b.global("NonEmpty", "from_list"), unwrap(b, b.loc("c"))),
            ),
        )),
        ("isEmpty", 1) => Some(b.lam(
            &["c"],
            b.bin(
                BinOp::Eq,
                b.app(b.global("List.Core", "length"), unwrap(b, b.loc("c"))),
                b.int(0),
            ),
        )),
        ("of", 1) => Some(b.lam(&["x"], wrap(b, b.list(vec![b.loc("x")])))),
        ("make", 2) => Some(b.lam(
            &["x", "rest"],
            wrap(
                b,
                b.bin(BinOp::ListConcat, b.list(vec![b.loc("x")]), b.loc("rest")),
            ),
        )),
        ("makeBy", 2) => Some(b.lam(
            &["n", "f"],
            wrap(
                b,
                b.app2(b.global("NonEmpty", "make_by"), b.loc("n"), b.loc("f")),
            ),
        )),
        ("range", 2) => Some(b.lam(
            &["s", "e"],
            wrap(
                b,
                b.app2(b.global("NonEmpty", "range"), b.loc("s"), b.loc("e")),
            ),
        )),
        ("head", 1) => Some(b.lam(
            &["c"],
            b.app2(
                b.global("List.Core", "get"),
                b.int(0),
                unwrap(b, b.loc("c")),
            ),
        )),
        ("headNonEmpty", 1) => Some(b.lam(
            &["c"],
            b.app(b.global("NonEmpty", "head"), unwrap(b, b.loc("c"))),
        )),
        ("last", 1) => Some(b.lam(
            &["c"],
            b.app(b.global("List.Core", "last"), unwrap(b, b.loc("c"))),
        )),
        ("lastNonEmpty", 1) => Some(b.lam(
            &["c"],
            b.app(b.global("NonEmpty", "last"), unwrap(b, b.loc("c"))),
        )),
        ("tail", 1) => Some(b.lam(
            &["c"],
            wrap_opt(
                b,
                b.app(b.global("List.Slice", "tail"), unwrap(b, b.loc("c"))),
            ),
        )),
        ("tailNonEmpty", 1) => Some(b.lam(
            &["c"],
            wrap(
                b,
                b.app(b.global("NonEmpty", "tail"), unwrap(b, b.loc("c"))),
            ),
        )),
        _ => chunk_core_combine(b, bind, stages),
    }
}

fn chunk_core_combine(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("prepend", 2) => Some(b.lam(
            &["x", "c"],
            wrap(
                b,
                b.bin(
                    BinOp::ListConcat,
                    b.list(vec![b.loc("x")]),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        ("appendAll", 2) => Some(b.lam(
            &["that", "self"],
            wrap(
                b,
                b.bin(
                    BinOp::ListConcat,
                    unwrap(b, b.loc("self")),
                    unwrap(b, b.loc("that")),
                ),
            ),
        )),
        ("reverse", 1) => Some(b.lam(
            &["c"],
            wrap(
                b,
                b.app(b.global("List.Core", "reverse"), unwrap(b, b.loc("c"))),
            ),
        )),
        _ => None,
    }
}

fn chunk_core_slice(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("take", 2) => Some(b.lam(
            &["n", "c"],
            wrap(
                b,
                b.app2(
                    b.global("List.Core", "take"),
                    b.loc("n"),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        ("takeRight", 2) => Some(b.lam(
            &["n", "c"],
            wrap(
                b,
                b.app2(
                    b.global("List.Slice", "take_right"),
                    b.loc("n"),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        ("drop", 2) => Some(b.lam(
            &["n", "c"],
            wrap(
                b,
                b.app2(
                    b.global("List.Core", "drop"),
                    b.loc("n"),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        ("dropRight", 2) => Some(b.lam(
            &["n", "c"],
            wrap(
                b,
                b.app2(
                    b.global("List.Slice", "drop_right"),
                    b.loc("n"),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        ("takeWhile", 2) => Some(b.lam(
            &["p", "c"],
            wrap(
                b,
                b.app2(
                    b.global("List.Slice", "take_while"),
                    b.loc("p"),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        ("dropWhile", 2) => Some(b.lam(
            &["p", "c"],
            wrap(
                b,
                b.app2(
                    b.global("List.Slice", "drop_while"),
                    b.loc("p"),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        _ => chunk_core_split(b, bind, stages),
    }
}

fn chunk_core_split(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    let wrap_nested = |expr: Expr| {
        wrap(
            b,
            b.app2(
                b.global("Instances", "map_list"),
                b.lam(&["p"], wrap(b, b.loc("p"))),
                expr,
            ),
        )
    };
    match (bind, stages) {
        ("splitAt", 2) => Some(b.lam(
            &["n", "c"],
            wrap_pair(
                b,
                b.app2(
                    b.global("List.Slice", "split_at"),
                    b.loc("n"),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        ("splitNonEmptyAt", 2) => Some(b.lam(
            &["n", "c"],
            wrap_pair(
                b,
                b.app2(
                    b.global("NonEmpty", "split_at"),
                    b.loc("n"),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        ("split", 2) => Some(b.lam(
            &["n", "c"],
            wrap_nested(b.app2(
                b.global("List.Slice", "split"),
                b.loc("n"),
                unwrap(b, b.loc("c")),
            )),
        )),
        ("splitWhere", 2) => Some(b.lam(
            &["p", "c"],
            wrap_pair(
                b,
                b.app2(
                    b.global("List.Slice", "split_where"),
                    b.loc("p"),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        ("chunksOf", 2) => Some(b.lam(
            &["n", "c"],
            wrap_nested(b.app2(
                b.global("List.Slice", "chunks_of"),
                b.loc("n"),
                unwrap(b, b.loc("c")),
            )),
        )),
        ("remove", 2) => Some(b.lam(
            &["i", "c"],
            wrap(
                b,
                b.app2(
                    b.global("List.Slice", "remove"),
                    b.loc("i"),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        ("modify", 3) => Some(b.lam(
            &["i", "f", "c"],
            wrap_opt(
                b,
                b.app3(
                    b.global("List.Slice", "modify"),
                    b.loc("i"),
                    b.loc("f"),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        ("replace", 3) => Some(b.lam(
            &["i", "x", "c"],
            wrap_opt(
                b,
                b.app3(
                    b.global("List.Slice", "replace"),
                    b.loc("i"),
                    b.loc("x"),
                    unwrap(b, b.loc("c")),
                ),
            ),
        )),
        _ => None,
    }
}
