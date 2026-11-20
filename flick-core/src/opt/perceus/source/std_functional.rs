//! Audited `Bool`, `Function`, `Predicate`, `Filter`, `Redacted`, `Hash`, and `Bytes` adapters.

use flick_ir::memory::{BytesIntrinsic, HashIntrinsic};
use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{
    Code, Env, Expr, ExprKind, Intrinsic, Lit, Local, Lower, LowerError, Operation, Pat, Span,
};

impl Lower<'_> {
    pub(super) fn std_functional_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        let from = value.binding.from.as_deref()?;
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();

        // `Redacted` uses `ForeignMode::Sync`.
        if value.mode == ForeignMode::Sync {
            let expr = redacted_template(from, bind, stages, span)?;
            return Some(self.expr(&expr, &Env::new(), block));
        }
        if value.mode != ForeignMode::Pure {
            return None;
        }
        if from == "flick:rt" && bind == "bytesEmpty" && stages == 0 {
            return Some(block.emit(
                Operation::Intrinsic {
                    kind: Intrinsic::Bytes(BytesIntrinsic::Empty),
                    args: vec![],
                },
                span,
            ));
        }
        if let Some(kind) = bytes_or_hash_intrinsic(from, bind, stages) {
            return Some(self.intrinsic(kind, false, vec![], block, span));
        }
        let b = ExprBuilder::new(span, "$fn");
        let expr = match from {
            "flick:Boolean" => bool_template(&b, bind, stages),
            "flick:Function" => function_template(&b, bind, stages),
            "flick:Predicate" => predicate_template(&b, bind, stages),
            "flick:Filter" => filter_template(&b, bind, stages),
            _ => None,
        }?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

fn bytes_or_hash_intrinsic(from: &str, bind: &str, stages: usize) -> Option<Intrinsic> {
    let kind = match (from, bind) {
        ("flick:rt", "bytesLength") => Intrinsic::Bytes(BytesIntrinsic::Length),
        ("flick:rt", "bytesGet") => Intrinsic::Bytes(BytesIntrinsic::Get),
        ("flick:rt", "bytesSlice") => Intrinsic::Bytes(BytesIntrinsic::Slice),
        ("flick:rt", "bytesFromList") => Intrinsic::Bytes(BytesIntrinsic::FromList),
        ("flick:rt", "bytesToList") => Intrinsic::Bytes(BytesIntrinsic::ToList),
        ("flick:rt", "bytesEncodeUtf8") => Intrinsic::Bytes(BytesIntrinsic::EncodeUtf8),
        ("flick:rt", "bytesDecodeUtf8") => Intrinsic::Bytes(BytesIntrinsic::DecodeUtf8),
        ("flick:rt", "bytesEncodeBase64Url") => Intrinsic::Bytes(BytesIntrinsic::EncodeBase64Url),
        ("flick:rt", "bytesDecodeBase64Url") => Intrinsic::Bytes(BytesIntrinsic::DecodeBase64Url),
        ("flick:rt", "bytesEqualConstantTime") => {
            Intrinsic::Bytes(BytesIntrinsic::EqualConstantTime)
        }
        ("flick:Crypto", "flickUtf8Encode") => Intrinsic::Bytes(BytesIntrinsic::LegacyUtf8Encode),
        ("flick:Crypto", "flickUtf8Decode") => Intrinsic::Bytes(BytesIntrinsic::LegacyUtf8Decode),
        ("flick:Crypto", "flickBase64UrlEncode") => {
            Intrinsic::Bytes(BytesIntrinsic::LegacyBase64UrlEncode)
        }
        ("flick:Crypto", "flickBase64UrlDecode") => {
            Intrinsic::Bytes(BytesIntrinsic::LegacyBase64UrlDecode)
        }
        ("flick:Hash", "combine") => Intrinsic::Hash(HashIntrinsic::Combine),
        ("flick:Hash", "optimize") => Intrinsic::Hash(HashIntrinsic::Optimize),
        _ => return None,
    };
    (kind.arity() == stages).then_some(kind)
}

fn redacted_template(from: &str, bind: &str, stages: usize, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$red");
    let wrap = || ExprBuilder::con("Redacted", "Redacted", "$Redacted", 1);
    match (from, bind, stages) {
        ("flick:rt", "redactedMake", 1) => {
            Some(b.lam(&["v"], b.app(b.at(ExprKind::Con(wrap())), b.loc("v"))))
        }
        ("flick:rt", "redactedReveal", 1) => Some(b.lam(
            &["r"],
            b.match_(
                b.loc("r"),
                vec![arm(
                    Pat::Con(wrap(), vec![Pat::Bind(b.name("v"))]),
                    b.loc("v"),
                )],
            ),
        )),
        ("flick:Redacted", "flickEquivalent", 3) => Some(b.lam(
            &["eq", "a", "b"],
            b.match_(
                b.loc("a"),
                vec![arm(
                    Pat::Con(wrap(), vec![Pat::Bind(b.name("av"))]),
                    b.match_(
                        b.loc("b"),
                        vec![arm(
                            Pat::Con(wrap(), vec![Pat::Bind(b.name("bv"))]),
                            b.app2(b.loc("eq"), b.loc("av"), b.loc("bv")),
                        )],
                    ),
                )],
            ),
        )),
        _ => None,
    }
}

fn bool_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("not", 1) => Some(b.lam(&["x"], b.not(b.loc("x")))),
        ("and", 2) => Some(b.lam(
            &["that", "self"],
            b.bin(BinOp::And, b.loc("self"), b.loc("that")),
        )),
        ("nand", 2) => Some(b.lam(
            &["that", "self"],
            b.not(b.bin(BinOp::And, b.loc("self"), b.loc("that"))),
        )),
        ("or", 2) => Some(b.lam(
            &["that", "self"],
            b.bin(BinOp::Or, b.loc("self"), b.loc("that")),
        )),
        ("nor", 2) => Some(b.lam(
            &["that", "self"],
            b.not(b.bin(BinOp::Or, b.loc("self"), b.loc("that"))),
        )),
        ("xor", 2) => Some(b.lam(
            &["that", "self"],
            b.bin(BinOp::NotEq, b.loc("self"), b.loc("that")),
        )),
        ("eqv", 2) => Some(b.lam(
            &["that", "self"],
            b.bin(BinOp::Eq, b.loc("self"), b.loc("that")),
        )),
        ("implies", 2) => Some(b.lam(
            &["that", "self"],
            b.bin(BinOp::Or, b.not(b.loc("self")), b.loc("that")),
        )),
        ("every", 1) => Some(b.lam(
            &["xs"],
            b.app2(
                b.global("List.Core", "all"),
                b.lam(&["x"], b.loc("x")),
                b.loc("xs"),
            ),
        )),
        ("some", 1) => Some(b.lam(
            &["xs"],
            b.app2(
                b.global("List.Core", "any"),
                b.lam(&["x"], b.loc("x")),
                b.loc("xs"),
            ),
        )),
        ("match", 2) => Some(b.lam(
            &["cases", "v"],
            b.if_(
                b.loc("v"),
                b.app(b.field(b.loc("cases"), "onTrue"), b.unit()),
                b.app(b.field(b.loc("cases"), "onFalse"), b.unit()),
            ),
        )),
        ("ReducerAnd", 0) => Some(super::std_combine::make_reducer_val(
            b,
            b.lam(&["x", "y"], b.bin(BinOp::And, b.loc("x"), b.loc("y"))),
            b.bool_(true),
            None,
        )),
        ("ReducerOr", 0) => Some(super::std_combine::make_reducer_val(
            b,
            b.lam(&["x", "y"], b.bin(BinOp::Or, b.loc("x"), b.loc("y"))),
            b.bool_(false),
            None,
        )),
        _ => None,
    }
}

fn function_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("constant", 2) => Some(b.lam(&["v", "_u"], b.loc("v"))),
        ("constTrue", 1) => Some(b.lam(&["_u"], b.bool_(true))),
        ("constFalse", 1) => Some(b.lam(&["_u"], b.bool_(false))),
        ("constVoid", 1) => Some(b.lam(&["_u"], b.unit())),
        ("flickFlip", 3) => {
            Some(b.lam(&["f", "y", "x"], b.app2(b.loc("f"), b.loc("x"), b.loc("y"))))
        }
        ("compose", 3) => Some(b.lam(
            &["g", "f", "x"],
            b.app(b.loc("g"), b.app(b.loc("f"), b.loc("x"))),
        )),
        ("absurd", 1) => Some(b.lam(&["x"], b.loc("x"))),
        ("tupled", 2) => Some(b.lam(
            &["f", "pair"],
            b.match_(
                b.loc("pair"),
                vec![arm(
                    Pat::Tuple(vec![Pat::Bind(b.name("x")), Pat::Bind(b.name("y"))]),
                    b.app2(b.loc("f"), b.loc("x"), b.loc("y")),
                )],
            ),
        )),
        ("flickUntupled", 3) => Some(b.lam(
            &["f", "x", "y"],
            b.app(b.loc("f"), b.tuple(vec![b.loc("x"), b.loc("y")])),
        )),
        ("SK", 2) => Some(b.lam(&["_x", "y"], b.loc("y"))),
        _ => None,
    }
}

fn predicate_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    let px = || b.app(b.loc("p"), b.loc("x"));
    let qx = || b.app(b.loc("q"), b.loc("x"));
    match (bind, stages) {
        ("mapInput", 3) => Some(b.lam(
            &["f", "p", "x"],
            b.app(b.loc("p"), b.app(b.loc("f"), b.loc("x"))),
        )),
        ("not", 2) => Some(b.lam(&["p", "x"], b.not(px()))),
        ("and", 3) => Some(b.lam(&["q", "p", "x"], b.bin(BinOp::And, px(), qx()))),
        ("or", 3) => Some(b.lam(&["q", "p", "x"], b.bin(BinOp::Or, px(), qx()))),
        ("xor", 3) => Some(b.lam(&["q", "p", "x"], b.bin(BinOp::NotEq, px(), qx()))),
        ("eqv", 3) => Some(b.lam(&["q", "p", "x"], b.bin(BinOp::Eq, px(), qx()))),
        ("implies", 3) => Some(b.lam(&["q", "p", "x"], b.bin(BinOp::Or, b.not(px()), qx()))),
        ("nand", 3) => Some(b.lam(&["q", "p", "x"], b.not(b.bin(BinOp::And, px(), qx())))),
        ("nor", 3) => Some(b.lam(&["q", "p", "x"], b.not(b.bin(BinOp::Or, px(), qx())))),
        ("every", 2) => Some(b.lam(
            &["ps", "x"],
            b.app2(
                b.global("List.Core", "all"),
                b.lam(&["p"], px()),
                b.loc("ps"),
            ),
        )),
        ("some", 2) => Some(b.lam(
            &["ps", "x"],
            b.app2(
                b.global("List.Core", "any"),
                b.lam(&["p"], px()),
                b.loc("ps"),
            ),
        )),
        _ => None,
    }
}

fn filter_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("make" | "makeEffect" | "toResult", 2) => Some(b.lam(&["f"], b.loc("f"))),
        ("mapFail", 3) => Some(b.lam(
            &["g", "f", "x"],
            b.match_(
                b.app(b.loc("f"), b.loc("x")),
                vec![
                    arm(b.ok_pat("v"), b.ok(b.loc("v"))),
                    arm(b.err_pat("e"), b.err(b.app(b.loc("g"), b.loc("e")))),
                ],
            ),
        )),
        ("fromPredicate", 2) => Some(b.lam(
            &["p", "x"],
            b.if_(
                b.app(b.loc("p"), b.loc("x")),
                b.ok(b.loc("x")),
                b.err(b.loc("x")),
            ),
        )),
        ("fromPredicateOption", 2) => Some(b.lam(
            &["f", "x"],
            b.match_(
                b.app(b.loc("f"), b.loc("x")),
                vec![
                    arm(b.some_pat("v"), b.ok(b.loc("v"))),
                    arm(Pat::Wild, b.err(b.loc("x"))),
                ],
            ),
        )),
        ("toPredicate", 2) => Some(b.lam(
            &["f", "x"],
            b.match_(
                b.app(b.loc("f"), b.loc("x")),
                vec![
                    arm(b.ok_pat("_v"), b.bool_(true)),
                    arm(Pat::Wild, b.bool_(false)),
                ],
            ),
        )),
        ("toOption", 2) => Some(b.lam(
            &["f", "x"],
            b.match_(
                b.app(b.loc("f"), b.loc("x")),
                vec![
                    arm(b.ok_pat("v"), b.some(b.loc("v"))),
                    arm(Pat::Wild, b.none()),
                ],
            ),
        )),
        ("equals", 2) => Some(b.lam(
            &["v", "x"],
            b.if_(
                b.bin(BinOp::Eq, b.loc("x"), b.loc("v")),
                b.ok(b.loc("x")),
                b.err(b.loc("x")),
            ),
        )),
        ("equalsStrict", 2) => Some(b.lam(
            &["v", "x"],
            b.if_(
                b.bin(BinOp::Same, b.loc("x"), b.loc("v")),
                b.ok(b.loc("x")),
                b.err(b.loc("x")),
            ),
        )),
        ("or", 3) => Some(b.lam(
            &["that", "self", "x"],
            b.match_(
                b.app(b.loc("self"), b.loc("x")),
                vec![
                    arm(b.ok_pat("v"), b.ok(b.loc("v"))),
                    arm(Pat::Wild, b.app(b.loc("that"), b.loc("x"))),
                ],
            ),
        )),
        _ => filter_compose_template(b, bind, stages),
    }
}

fn filter_compose_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    let both_ok = |out: Expr| {
        b.match_(
            b.app(b.loc("left"), b.loc("x")),
            vec![
                arm(b.err_pat("e"), b.err(b.loc("e"))),
                arm(
                    b.ok_pat("av"),
                    b.match_(
                        b.app(b.loc("right"), b.loc("x")),
                        vec![
                            arm(b.err_pat("e2"), b.err(b.loc("e2"))),
                            arm(b.ok_pat("bv"), b.ok(out)),
                        ],
                    ),
                ),
            ],
        )
    };
    match (bind, stages) {
        ("zipWith", 4) => Some(b.lam(
            &["right", "f", "left", "x"],
            both_ok(b.app2(b.loc("f"), b.loc("av"), b.loc("bv"))),
        )),
        ("zip", 3) => Some(b.lam(
            &["right", "left", "x"],
            both_ok(b.tuple(vec![b.loc("av"), b.loc("bv")])),
        )),
        ("andLeft", 3) => Some(b.lam(&["right", "left", "x"], both_ok(b.loc("av")))),
        ("andRight", 3) => Some(b.lam(&["right", "left", "x"], both_ok(b.loc("bv")))),
        ("compose", 3) => Some(b.lam(
            &["right", "left", "x"],
            b.match_(
                b.app(b.loc("left"), b.loc("x")),
                vec![
                    arm(b.err_pat("e"), b.err(b.loc("e"))),
                    arm(b.ok_pat("av"), b.app(b.loc("right"), b.loc("av"))),
                ],
            ),
        )),
        ("composePassthrough", 3) => Some(b.lam(
            &["right", "left", "x"],
            b.match_(
                b.app(b.loc("left"), b.loc("x")),
                vec![
                    arm(
                        b.ok_pat("av"),
                        b.match_(
                            b.app(b.loc("right"), b.loc("av")),
                            vec![
                                arm(b.ok_pat("bv"), b.ok(b.loc("bv"))),
                                arm(Pat::Wild, b.err(b.loc("x"))),
                            ],
                        ),
                    ),
                    arm(Pat::Wild, b.err(b.loc("x"))),
                ],
            ),
        )),
        _ => None,
    }
}

pub(super) fn str_lit(b: &ExprBuilder, s: &str) -> Expr {
    b.at(ExprKind::Lit(Lit::String(s.into())))
}
