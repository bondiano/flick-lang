//! Audited `Test` and `Gen` pure IR adapters (`packages/std/src/{Test,Gen}.fl`).
//!
//! `Gen a` is represented as a pure state-passing closure `Int -> (Int, a)`
//! threading a 32-bit mulberry32 seed, so `Gen.map` and `Gen.function` stay
//! ordinary counted closures with zero VM re-entrancy. `Test` values are pure
//! tagged descriptors walked by the counted test runner.

use flick_ir::memory::GenIntrinsic;
use flick_ir::{Arith, BinOp, Dict, Foreign, ForeignMode, Num};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, ExprKind, Intrinsic, Local, Lower, LowerError, Operation, Pat, Span};

impl Lower<'_> {
    pub(super) fn std_test_gen_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.mode != ForeignMode::Pure || !has_standard_shape(value) {
            return None;
        }
        let from = value.binding.from.as_deref()?;
        if from != "flick:rt" {
            return None;
        }
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$tg");

        if value.dicts == 1 && bind == "genFunction" && stages == 1 {
            let expr = gen_function_template(&b);
            return Some(self.apply_gen_intrinsic(&expr, GenIntrinsic::Step, block, span));
        }
        if value.dicts != 0 {
            return None;
        }
        if let Some(expr) = gen_step_template(&b, bind, stages) {
            return Some(self.apply_gen_intrinsic(&expr, GenIntrinsic::Step, block, span));
        }
        if bind == "genString" && stages == 1 {
            return Some(self.intrinsic(
                Intrinsic::Gen(GenIntrinsic::DrawString),
                false,
                vec![],
                block,
                span,
            ));
        }
        if bind == "genOneOf" && stages == 1 {
            let expr = gen_one_of_template(&b);
            return Some(
                self.expr(&expr, &Env::new(), block)
                    .and_then(|f1| {
                        let draw = self.intrinsic(
                            Intrinsic::Gen(GenIntrinsic::DrawOneOf),
                            false,
                            vec![],
                            block,
                            span,
                        )?;
                        block.emit(
                            Operation::Apply {
                                function: f1,
                                argument: draw,
                            },
                            span,
                        )
                    })
                    .and_then(|f2| {
                        let check = self.intrinsic(
                            Intrinsic::Gen(GenIntrinsic::OneOfCheck),
                            false,
                            vec![],
                            block,
                            span,
                        )?;
                        block.emit(
                            Operation::Apply {
                                function: f2,
                                argument: check,
                            },
                            span,
                        )
                    }),
            );
        }
        let expr = plain_test_gen_template(&b, bind, stages)?;
        Some(self.expr(&expr, &Env::new(), block))
    }

    fn apply_gen_intrinsic(
        &mut self,
        expr: &Expr,
        op: GenIntrinsic,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let function = self.expr(expr, &Env::new(), block)?;
        let argument = self.intrinsic(Intrinsic::Gen(op), false, vec![], block, span)?;
        block.emit(Operation::Apply { function, argument }, span)
    }
}

fn pair_pat(b: &ExprBuilder, a: &str, c: &str) -> Pat {
    Pat::Tuple(vec![Pat::Bind(b.name(a)), Pat::Bind(b.name(c))])
}

fn gen_step_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("genNumber", 2) => Some(b.lam(
            &["step", "lo", "hi", "s"],
            b.match_(
                b.app(b.loc("step"), b.loc("s")),
                vec![arm(
                    pair_pat(b, "s1", "r"),
                    b.tuple(vec![
                        b.loc("s1"),
                        b.bin(
                            BinOp::Arith(Arith::Add, Num::Float),
                            b.loc("lo"),
                            b.bin(
                                BinOp::Arith(Arith::Mul, Num::Float),
                                b.loc("r"),
                                b.bin(
                                    BinOp::Arith(Arith::Sub, Num::Float),
                                    b.loc("hi"),
                                    b.loc("lo"),
                                ),
                            ),
                        ),
                    ]),
                )],
            ),
        )),
        ("genInt", 2) => Some(b.lam(
            &["step", "lo", "hi", "s"],
            b.match_(
                b.app(b.loc("step"), b.loc("s")),
                vec![arm(
                    pair_pat(b, "s1", "r"),
                    b.tuple(vec![
                        b.loc("s1"),
                        b.app(
                            b.global("Int", "truncate"),
                            b.bin(
                                BinOp::Arith(Arith::Add, Num::Float),
                                b.app(b.global("Int", "to_float"), b.loc("lo")),
                                b.app(
                                    b.global("Float", "floor"),
                                    b.bin(
                                        BinOp::Arith(Arith::Mul, Num::Float),
                                        b.loc("r"),
                                        b.bin(
                                            BinOp::Arith(Arith::Add, Num::Float),
                                            b.bin(
                                                BinOp::Arith(Arith::Sub, Num::Float),
                                                b.app(b.global("Int", "to_float"), b.loc("hi")),
                                                b.app(b.global("Int", "to_float"), b.loc("lo")),
                                            ),
                                            b.float(1.0),
                                        ),
                                    ),
                                ),
                            ),
                        ),
                    ]),
                )],
            ),
        )),
        ("genBool", 0) => Some(b.lam(
            &["step", "s"],
            b.match_(
                b.app(b.loc("step"), b.loc("s")),
                vec![arm(
                    pair_pat(b, "s1", "r"),
                    b.tuple(vec![
                        b.loc("s1"),
                        b.bin(BinOp::Lt, b.loc("r"), b.float(0.5)),
                    ]),
                )],
            ),
        )),
        ("genList", 2) => Some(gen_list_template(b)),
        _ => None,
    }
}

fn gen_list_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["step", "max", "g", "s"],
        b.match_(
            b.app(b.loc("step"), b.loc("s")),
            vec![arm(
                pair_pat(b, "s1", "r"),
                b.block(
                    vec![
                        b.let_(
                            "len",
                            b.app(
                                b.global("Int", "truncate"),
                                b.app(
                                    b.global("Float", "floor"),
                                    b.bin(
                                        BinOp::Arith(Arith::Mul, Num::Float),
                                        b.loc("r"),
                                        b.bin(
                                            BinOp::Arith(Arith::Add, Num::Float),
                                            b.app(b.global("Int", "to_float"), b.loc("max")),
                                            b.float(1.0),
                                        ),
                                    ),
                                ),
                            ),
                        ),
                        b.var_decl("i", b.int(0)),
                        b.var_decl("st", b.loc("s1")),
                        b.let_(
                            "xs",
                            b.gen_strict(b.while_(
                                b.bin(BinOp::Lt, b.var("i"), b.loc("len")),
                                b.match_(
                                    b.app(b.loc("g"), b.var("st")),
                                    vec![arm(
                                        pair_pat(b, "st2", "v"),
                                        b.block(
                                            vec![
                                                b.assign("st", b.loc("st2")),
                                                b.assign("i", b.add_i(b.var("i"), b.int(1))),
                                            ],
                                            b.yield_(b.loc("v")),
                                        ),
                                    )],
                                ),
                            )),
                        ),
                    ],
                    b.tuple(vec![b.var("st"), b.loc("xs")]),
                ),
            )],
        ),
    )
}

fn gen_one_of_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["draw", "check", "xs"],
        b.block(
            vec![b.let_("checked", b.app(b.loc("check"), b.loc("xs")))],
            b.lam(&["s"], b.app2(b.loc("draw"), b.loc("checked"), b.loc("s"))),
        ),
    )
}

fn gen_function_template(b: &ExprBuilder) -> Expr {
    let hash_fn = b.at(ExprKind::Method {
        dict: Dict::Field(b.name("hdict")),
        name: "hash".into(),
        type_args: Vec::new(),
    });
    b.lam(
        &["step", "hdict", "g", "s"],
        b.match_(
            b.app(b.loc("step"), b.loc("s")),
            vec![arm(
                pair_pat(b, "s1", "r"),
                b.block(
                    vec![b.let_(
                        "base",
                        b.app(
                            b.global("Int", "truncate"),
                            b.app(
                                b.global("Float", "floor"),
                                b.bin(
                                    BinOp::Arith(Arith::Mul, Num::Float),
                                    b.loc("r"),
                                    b.float(4_294_967_296.0),
                                ),
                            ),
                        ),
                    )],
                    b.tuple(vec![
                        b.loc("s1"),
                        b.lam(
                            &["a"],
                            b.match_(
                                b.app(
                                    b.loc("g"),
                                    b.app2(
                                        b.global("Int", "bit_xor"),
                                        b.loc("base"),
                                        b.app(hash_fn, b.loc("a")),
                                    ),
                                ),
                                vec![arm(pair_pat(b, "_s2", "v"), b.loc("v"))],
                            ),
                        ),
                    ]),
                ),
            )],
        ),
    )
}

fn test_con(b: &ExprBuilder, ctor: &str, params: &[&str]) -> Expr {
    let c = b.at(ExprKind::Con(ExprBuilder::con(
        "Test",
        "Test",
        ctor,
        params.len(),
    )));
    let applied = params.iter().fold(c, |acc, &p| b.app(acc, b.loc(p)));
    b.lam(params, applied)
}

fn plain_test_gen_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("genConstant", 1) => Some(b.lam(&["v", "s"], b.tuple(vec![b.loc("s"), b.loc("v")]))),
        ("genPair", 2) => Some(b.lam(
            &["ga", "gb", "s"],
            b.match_(
                b.app(b.loc("ga"), b.loc("s")),
                vec![arm(
                    pair_pat(b, "s1", "a"),
                    b.match_(
                        b.app(b.loc("gb"), b.loc("s1")),
                        vec![arm(
                            pair_pat(b, "s2", "b"),
                            b.tuple(vec![b.loc("s2"), b.tuple(vec![b.loc("a"), b.loc("b")])]),
                        )],
                    ),
                )],
            ),
        )),
        ("genMap", 2) => Some(b.lam(
            &["f", "g", "s"],
            b.match_(
                b.app(b.loc("g"), b.loc("s")),
                vec![arm(
                    pair_pat(b, "s1", "a"),
                    b.tuple(vec![b.loc("s1"), b.app(b.loc("f"), b.loc("a"))]),
                )],
            ),
        )),
        ("testPure", 2) => Some(test_con(b, "$Pure", &["name", "body"])),
        ("testEffect", 2) => Some(test_con(b, "$Effect", &["name", "task"])),
        ("testClock", 2) => Some(test_con(b, "$Clock", &["name", "task"])),
        ("testProperty", 3) => Some(test_con(b, "$Property", &["name", "gen", "check"])),
        ("testGroup", 2) => Some(test_con(b, "$Group", &["name", "xs"])),
        ("testSuite", 3) => Some(test_con(b, "$Suite", &["name", "layer", "xs"])),
        ("testSkip", 1) => Some(test_con(b, "$Skip", &["t"])),
        ("testOnly", 1) => Some(test_con(b, "$Only", &["t"])),
        ("testBeforeAll", 1) => Some(test_con(b, "$BeforeAll", &["t"])),
        ("testBeforeEach", 1) => Some(test_con(b, "$BeforeEach", &["t"])),
        ("testAfterEach", 1) => Some(test_con(b, "$AfterEach", &["t"])),
        ("testAfterAll", 1) => Some(test_con(b, "$AfterAll", &["t"])),
        ("testSerial", 1) => Some(test_con(b, "$Serial", &["t"])),
        ("testTimeout", 2) => Some(test_con(b, "$Timeout", &["ms", "t"])),
        ("testFixture", 2) => Some(test_con(b, "$Fixture", &["k", "t"])),
        ("testShared", 2) => Some(test_con(b, "$Shared", &["k", "t"])),
        ("testShare", 1) => Some(test_con(b, "$Share", &["f"])),
        _ => None,
    }
}
