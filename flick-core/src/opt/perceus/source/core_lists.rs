//! Audited `List`, `NonEmpty`, `Option`, `Result`, and `Validation` foreign adapters.

use flick_ir::memory::ListIntrinsic;
use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{
    BinOp, Code, Env, Expr, Intrinsic, Local, Lower, LowerError, Operation, Pat, Span, Stmt,
};

impl Lower<'_> {
    pub(super) fn core_list_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.mode != ForeignMode::Pure || value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        let (from, bind) = value.binding.module_export()?;
        let stages = value.stages.len();

        if let Some(kind) = list_intrinsic(from, bind, stages) {
            return Some(self.intrinsic(Intrinsic::List(kind), false, vec![], block, span));
        }
        if from == "flick:rt" && bind == "listSortBy" && stages == 2 {
            let expr = super::core_lists_cb::sort_by_template(span);
            return Some(self.expr(&expr, &Env::new(), block).and_then(|function| {
                let argument = self.intrinsic(
                    Intrinsic::List(ListIntrinsic::Length),
                    false,
                    vec![],
                    block,
                    span,
                )?;
                block.emit(Operation::Apply { function, argument }, span)
            }));
        }
        if let Some(expr) = core_data_template(from, bind, stages, span)
            .or_else(|| super::core_lists_cb::template(from, bind, stages, span))
            .or_else(|| super::core_lists_array::template(from, bind, stages, span))
        {
            return Some(self.expr(&expr, &Env::new(), block));
        }
        None
    }
}

fn list_intrinsic(from: &str, bind: &str, stages: usize) -> Option<ListIntrinsic> {
    let kind = match (from, bind) {
        ("flick:rt", "listGet") => ListIntrinsic::Get,
        ("flick:rt", "listSet") => ListIntrinsic::Set,
        ("flick:rt", "listRepeat") => ListIntrinsic::Repeat,
        ("flick:rt", "listReverse") => ListIntrinsic::Reverse,
        ("flick:rt", "listConcat") => ListIntrinsic::Concat,
        ("flick:rt", "listTake") => ListIntrinsic::Take,
        ("flick:rt", "listDrop") => ListIntrinsic::Drop,
        ("flick:rt", "listSlice") => ListIntrinsic::Slice,
        ("flick:rt", "listZip") => ListIntrinsic::Zip,
        ("flick:rt", "listUnzip") => ListIntrinsic::Unzip,
        ("flick:rt", "nonEmptyFromList") => ListIntrinsic::NonEmptyFromList,
        ("flick:rt", "nonEmptyCons") => ListIntrinsic::NonEmptyCons,
        ("flick:rt", "nonEmptyHead") => ListIntrinsic::NonEmptyHead,
        ("flick:Array", "takeRight") => ListIntrinsic::TakeRight,
        ("flick:Array", "dropRight") => ListIntrinsic::DropRight,
        ("flick:Array", "splitAt") => ListIntrinsic::SplitAt,
        ("flick:Array", "split") => ListIntrinsic::Split,
        ("flick:Array", "chunksOf") => ListIntrinsic::ChunksOf,
        ("flick:Array", "window") => ListIntrinsic::Window,
        ("flick:Array", "tail") => ListIntrinsic::Tail,
        ("flick:Array", "init") => ListIntrinsic::Init,
        ("flick:Array", "pad") => ListIntrinsic::Pad,
        ("flick:Array", "rotate") => ListIntrinsic::Rotate,
        ("flick:Array", "insertAt") => ListIntrinsic::InsertAt,
        ("flick:Array", "remove") => ListIntrinsic::Remove,
        ("flick:Array", "replace") => ListIntrinsic::Replace,
        ("flick:Array", "intersperse") => ListIntrinsic::Intersperse,
        ("flick:Array", "appendAll") => ListIntrinsic::AppendAll,
        ("flick:Array", "cartesian") => ListIntrinsic::Cartesian,
        ("flick:Array", "append") => ListIntrinsic::NonEmptyAppend,
        ("flick:Array", "range") => ListIntrinsic::NonEmptyRange,
        ("flick:Array", "replicate") => ListIntrinsic::NonEmptyReplicate,
        ("flick:Array", "lastNonEmpty") => ListIntrinsic::NonEmptyLast,
        ("flick:Array", "tailNonEmpty") => ListIntrinsic::NonEmptyTail,
        ("flick:Array", "initNonEmpty") => ListIntrinsic::NonEmptyInit,
        ("flick:Array", "setHeadNonEmpty") => ListIntrinsic::NonEmptySetHead,
        ("flick:Array", "setLastNonEmpty") => ListIntrinsic::NonEmptySetLast,
        ("flick:Array", "splitAtNonEmpty") => ListIntrinsic::NonEmptySplitAt,
        ("flick:Array", "unappend") => ListIntrinsic::NonEmptyUnappend,
        ("flick:Array", "unprepend") => ListIntrinsic::NonEmptyUnprepend,
        ("flick:Iterable", "flickRepeat") => ListIntrinsic::RepeatAll,
        _ => return None,
    };
    (kind.arity() == stages).then_some(kind)
}

fn core_data_template(from: &str, bind: &str, stages: usize, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$core");
    match (from, bind, stages) {
        ("flick:rt", "identity", 1) => Some(b.lam(&["x"], b.loc("x"))),
        ("flick:rt", "listSingleton", 1) => Some(b.lam(&["x"], b.list(vec![b.loc("x")]))),
        ("flick:rt", "loop", 2) => Some(loop_template(&b)),
        ("flick:rt", "trampolineDone", 1) => Some(b.lam(
            &["x"],
            b.app(
                b.at(super::ExprKind::Con(ExprBuilder::con(
                    "Loop",
                    "Trampoline",
                    "$Done",
                    1,
                ))),
                b.loc("x"),
            ),
        )),
        ("flick:rt", "trampolineSuspend", 1) => Some(b.lam(
            &["k"],
            b.app(
                b.at(super::ExprKind::Con(ExprBuilder::con(
                    "Loop",
                    "Trampoline",
                    "$Suspend",
                    1,
                ))),
                b.loc("k"),
            ),
        )),
        ("flick:rt", "trampolineRun", 1) => Some(trampoline_run_template(&b)),
        ("flick:Array", "join", 2) => Some(b.lam(
            &["sep", "xs"],
            b.app2(b.global("String", "join"), b.loc("sep"), b.loc("xs")),
        )),
        ("flick:Option", "productMany", 2) => {
            Some(super::core_lists_array::option_product_many(&b))
        }
        ("flick:rt", "validationMap2", 4) => Some(validation_map2(&b)),
        ("flick:Result", _, _) => result_template(&b, bind, stages),
        ("flick:Array", _, _) => array_extract_template(&b, bind, stages),
        _ => None,
    }
}

fn result_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("all", 1) => Some(super::core_lists_array::result_all(b)),
        ("failVoid", 0) => Some(b.err(b.unit())),
        ("void", 0) => Some(b.ok(b.unit())),
        ("succeedNone", 0) => Some(b.ok(b.none())),
        ("succeedSome", 1) => Some(b.lam(&["x"], b.ok(b.some(b.loc("x"))))),
        ("flip", 1) => Some(b.lam(
            &["r"],
            b.match_(
                b.loc("r"),
                vec![
                    arm(b.ok_pat("x"), b.err(b.loc("x"))),
                    arm(b.err_pat("e"), b.ok(b.loc("e"))),
                ],
            ),
        )),
        ("getFailure", 1) => Some(b.lam(
            &["r"],
            b.match_(
                b.loc("r"),
                vec![
                    arm(b.err_pat("e"), b.some(b.loc("e"))),
                    arm(Pat::Wild, b.none()),
                ],
            ),
        )),
        ("merge", 1) => Some(b.lam(
            &["r"],
            b.match_(
                b.loc("r"),
                vec![
                    arm(b.ok_pat("x"), b.loc("x")),
                    arm(b.err_pat("e"), b.loc("e")),
                ],
            ),
        )),
        ("filterOrFail", 3) => Some(b.lam(
            &["p", "fail_fn", "r"],
            b.match_(
                b.loc("r"),
                vec![
                    arm(b.err_pat("e"), b.err(b.loc("e"))),
                    arm(
                        b.ok_pat("x"),
                        b.if_(
                            b.app(b.loc("p"), b.loc("x")),
                            b.ok(b.loc("x")),
                            b.err(b.app(b.loc("fail_fn"), b.loc("x"))),
                        ),
                    ),
                ],
            ),
        )),
        ("liftPredicate", 3) => Some(b.lam(
            &["p", "fail_fn", "x"],
            b.if_(
                b.app(b.loc("p"), b.loc("x")),
                b.ok(b.loc("x")),
                b.err(b.app(b.loc("fail_fn"), b.loc("x"))),
            ),
        )),
        ("mapBoth", 2) => Some(result_cases(b, true)),
        ("match", 2) => Some(result_cases(b, false)),
        ("orElse", 2) => Some(b.lam(
            &["that", "r"],
            b.match_(
                b.loc("r"),
                vec![
                    arm(b.err_pat("e"), b.app(b.loc("that"), b.loc("e"))),
                    arm(b.ok_pat("x"), b.ok(b.loc("x"))),
                ],
            ),
        )),
        ("transposeOption", 1) => Some(result_transpose_option(b, false)),
        ("transposeMapOption", 2) => Some(result_transpose_option(b, true)),
        _ => None,
    }
}

fn result_cases(b: &ExprBuilder, wrap: bool) -> Expr {
    let on_err = b.app(b.field(b.loc("cases"), "onFailure"), b.loc("e"));
    let on_ok = b.app(b.field(b.loc("cases"), "onSuccess"), b.loc("x"));
    b.lam(
        &["cases", "r"],
        b.match_(
            b.loc("r"),
            vec![
                arm(b.err_pat("e"), if wrap { b.err(on_err) } else { on_err }),
                arm(b.ok_pat("x"), if wrap { b.ok(on_ok) } else { on_ok }),
            ],
        ),
    )
}

fn result_transpose_option(b: &ExprBuilder, mapped: bool) -> Expr {
    let inner = if mapped {
        b.app(b.loc("f"), b.loc("v"))
    } else {
        b.loc("v")
    };
    let body = b.match_(
        b.loc("opt"),
        vec![
            arm(
                b.some_pat("v"),
                b.match_(
                    inner,
                    vec![
                        arm(b.ok_pat("x"), b.ok(b.some(b.loc("x")))),
                        arm(b.err_pat("e"), b.err(b.loc("e"))),
                    ],
                ),
            ),
            arm(Pat::Wild, b.ok(b.none())),
        ],
    );
    if mapped {
        b.lam(&["f", "opt"], body)
    } else {
        b.lam(&["opt"], body)
    }
}

fn validation_map2(b: &ExprBuilder) -> Expr {
    // `Validation` is `unboxed type Validation e a = Validation (Result e a)`
    // so runtime values are bare `Result e a` without an outer wrapper tag.
    b.lam(
        &["combine", "f", "x", "y"],
        b.match_(
            b.loc("x"),
            vec![
                arm(
                    b.err_pat("ex"),
                    b.match_(
                        b.loc("y"),
                        vec![
                            arm(
                                b.err_pat("ey"),
                                b.err(b.app2(b.loc("combine"), b.loc("ex"), b.loc("ey"))),
                            ),
                            arm(Pat::Wild, b.err(b.loc("ex"))),
                        ],
                    ),
                ),
                arm(
                    b.ok_pat("vx"),
                    b.match_(
                        b.loc("y"),
                        vec![
                            arm(b.err_pat("ey"), b.err(b.loc("ey"))),
                            arm(
                                b.ok_pat("vy"),
                                b.ok(b.app2(b.loc("f"), b.loc("vx"), b.loc("vy"))),
                            ),
                        ],
                    ),
                ),
            ],
        ),
    )
}

fn array_extract_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("getSomes", 1) => Some(b.lam(
            &["xs"],
            b.gen_strict(b.each(
                "o",
                b.loc("xs"),
                b.match_(
                    b.loc("o"),
                    vec![
                        arm(b.some_pat("v"), b.yield_(b.loc("v"))),
                        arm(Pat::Wild, b.unit()),
                    ],
                ),
            )),
        )),
        ("getFailures", 1) => Some(b.lam(
            &["xs"],
            b.gen_strict(b.each(
                "r",
                b.loc("xs"),
                b.match_(
                    b.loc("r"),
                    vec![
                        arm(b.err_pat("e"), b.yield_(b.loc("e"))),
                        arm(Pat::Wild, b.unit()),
                    ],
                ),
            )),
        )),
        ("getSuccesses", 1) => Some(b.lam(
            &["xs"],
            b.gen_strict(b.each(
                "r",
                b.loc("xs"),
                b.match_(
                    b.loc("r"),
                    vec![
                        arm(b.ok_pat("v"), b.yield_(b.loc("v"))),
                        arm(Pat::Wild, b.unit()),
                    ],
                ),
            )),
        )),
        ("separate", 1) => Some(array_separate(b)),
        _ => None,
    }
}

fn array_separate(b: &ExprBuilder) -> Expr {
    b.lam(
        &["xs"],
        b.block(
            vec![
                b.var_decl("ok_xs", b.list(vec![])),
                b.var_decl("err_xs", b.list(vec![])),
                Stmt::Expr(b.each(
                    "r",
                    b.loc("xs"),
                    b.match_(
                        b.loc("r"),
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

fn loop_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["step", "init"],
        b.block(
            vec![b.var_decl("cur", b.loc("init"))],
            b.at(super::ExprKind::ControlLoop {
                id: "$loop".into(),
                condition: None,
                iterable: false,
                body: Box::new(b.match_(
                    b.app(b.loc("step"), b.var("cur")),
                    vec![
                        arm(
                            Pat::Con(
                                ExprBuilder::con("Loop", "Step", "Continue", 1),
                                vec![Pat::Bind(b.name("next"))],
                            ),
                            b.block(vec![b.assign("cur", b.loc("next"))], b.unit()),
                        ),
                        arm(
                            Pat::Con(
                                ExprBuilder::con("Loop", "Step", "Done", 1),
                                vec![Pat::Bind(b.name("val"))],
                            ),
                            b.break_("$loop", b.loc("val")),
                        ),
                    ],
                )),
            }),
        ),
    )
}

fn trampoline_run_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["start"],
        b.block(
            vec![b.var_decl("cur", b.loc("start"))],
            b.at(super::ExprKind::ControlLoop {
                id: "$tramp".into(),
                condition: None,
                iterable: false,
                body: Box::new(b.match_(
                    b.var("cur"),
                    vec![
                        arm(
                            Pat::Con(
                                ExprBuilder::con("Loop", "Trampoline", "$Suspend", 1),
                                vec![Pat::Bind(b.name("k"))],
                            ),
                            b.block(vec![b.assign("cur", b.app(b.loc("k"), b.unit()))], b.unit()),
                        ),
                        arm(
                            Pat::Con(
                                ExprBuilder::con("Loop", "Trampoline", "$Done", 1),
                                vec![Pat::Bind(b.name("val"))],
                            ),
                            b.break_("$tramp", b.loc("val")),
                        ),
                    ],
                )),
            }),
        ),
    )
}
