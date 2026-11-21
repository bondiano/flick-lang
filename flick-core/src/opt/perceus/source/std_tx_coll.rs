//! Audited `TxChunk`, `TxHashMap`, `TxHashSet`, and `TxPriorityQueue` pure IR
//! adapters (Stage 4C).

mod hashmap;
mod hashset;
mod pq;

use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::ExprBuilder;
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, Local, Lower, LowerError, Span};

impl Lower<'_> {
    pub(super) fn std_tx_coll_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.mode != ForeignMode::Effect || !has_standard_shape(value) {
            return None;
        }
        let from = value.binding.from.as_deref()?;
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$txc");
        if from == "flick:TxPriorityQueue" && value.dicts == 1 {
            let expr = pq::tx_pq_dict_template(&b, bind, stages)?;
            return Some(self.expr(&expr, &Env::new(), block));
        }
        if value.dicts != 0 {
            return None;
        }
        let expr = match from {
            "flick:TxChunk" => tx_chunk_template(&b, bind, stages)?,
            "flick:TxHashMap" => hashmap::tx_hashmap_template(&b, bind, stages)?,
            "flick:TxHashSet" => hashset::tx_hashset_template(&b, bind, stages)?,
            "flick:TxPriorityQueue" => pq::tx_pq_template(&b, bind, stages)?,
            _ => return None,
        };
        Some(self.expr(&expr, &Env::new(), block))
    }
}

fn update_chunk_op(b: &ExprBuilder, arg: &str, module: &str, op: &str) -> Expr {
    b.lam(
        &[arg, "c"],
        b.app2(
            b.global("TxRef", "update"),
            b.lam(
                &["ch"],
                b.app2(b.global(module, op), b.loc(arg), b.loc("ch")),
            ),
            b.loc("c"),
        ),
    )
}

fn tx_chunk_concat(b: &ExprBuilder) -> Expr {
    let step = b.block(
        [b.let_("that", b.app(b.global("TxRef", "get"), b.loc("other")))],
        b.app2(
            b.global("TxRef", "update"),
            b.lam(
                &["ch"],
                b.app2(
                    b.global("Chunk.Core", "append_all"),
                    b.loc("that"),
                    b.loc("ch"),
                ),
            ),
            b.loc("c"),
        ),
    );
    b.lam(
        &["other", "c"],
        b.app(b.global("Tx", "atomically"), b.task(step)),
    )
}

fn tx_chunk_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("make", 1) => Some(b.lam(&["c"], b.app(b.global("TxRef", "make"), b.loc("c")))),
        ("empty", 1) => Some(b.lam(
            &["_u"],
            b.app(b.global("TxRef", "make"), b.global("Chunk.Core", "empty")),
        )),
        ("fromIterable", 1) => Some(b.lam(
            &["xs"],
            b.app(
                b.global("TxRef", "make"),
                b.app(b.global("Chunk.Core", "from_list"), b.loc("xs")),
            ),
        )),
        ("get", 1) => Some(b.lam(&["c"], b.app(b.global("TxRef", "get"), b.loc("c")))),
        ("set", 2) => Some(b.lam(
            &["v", "c"],
            b.app2(b.global("TxRef", "set"), b.loc("v"), b.loc("c")),
        )),
        ("update", 2) => Some(b.lam(
            &["f", "c"],
            b.app2(b.global("TxRef", "update"), b.loc("f"), b.loc("c")),
        )),
        ("modify", 2) => Some(b.lam(
            &["f", "c"],
            b.app2(b.global("TxRef", "modify"), b.loc("f"), b.loc("c")),
        )),
        ("append", 2) => Some(update_chunk_op(b, "x", "Chunk.Core", "append")),
        ("prepend", 2) => Some(update_chunk_op(b, "x", "Chunk.Core", "prepend")),
        ("size", 1) => Some(b.lam(
            &["c"],
            b.app(
                b.global("Chunk.Core", "size"),
                b.app(b.global("TxRef", "get"), b.loc("c")),
            ),
        )),
        ("isEmpty", 1) => Some(b.lam(
            &["c"],
            b.app(
                b.global("Chunk.Core", "is_empty"),
                b.app(b.global("TxRef", "get"), b.loc("c")),
            ),
        )),
        ("isNonEmpty", 1) => Some(b.lam(
            &["c"],
            b.bin(
                BinOp::Gt,
                b.app(
                    b.global("Chunk.Core", "size"),
                    b.app(b.global("TxRef", "get"), b.loc("c")),
                ),
                b.int(0),
            ),
        )),
        ("take", 2) => Some(update_chunk_op(b, "n", "Chunk.Core", "take")),
        ("drop", 2) => Some(update_chunk_op(b, "n", "Chunk.Core", "drop")),
        ("slice", 3) => {
            let sliced = b.app2(
                b.global("Chunk.Core", "take"),
                b.sub_i(b.loc("e"), b.loc("s")),
                b.app2(b.global("Chunk.Core", "drop"), b.loc("s"), b.loc("ch")),
            );
            Some(b.lam(
                &["s", "e", "c"],
                b.app2(
                    b.global("TxRef", "update"),
                    b.lam(&["ch"], sliced),
                    b.loc("c"),
                ),
            ))
        }
        ("map", 2) => Some(update_chunk_op(b, "f", "Chunk.Core", "map")),
        ("filter", 2) => Some(update_chunk_op(b, "f", "Chunk.Ops", "filter")),
        ("appendAll", 2) => Some(update_chunk_op(b, "other", "Chunk.Core", "append_all")),
        ("prependAll", 2) => Some(update_chunk_op(b, "other", "Chunk.Core", "concat")),
        ("concat", 2) => Some(tx_chunk_concat(b)),
        _ => None,
    }
}
