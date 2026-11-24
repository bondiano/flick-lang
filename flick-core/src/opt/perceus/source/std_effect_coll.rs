//! Audited `Effect.Collections` adapters (`packages/std/src/Effect/Collections.fl`, Stage 4A).

use flick_ir::memory::AsyncIntrinsic;
use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, ExprKind, Intrinsic, Local, Lower, LowerError, Pat, Span};

impl Lower<'_> {
    pub(super) fn std_effect_coll_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || value.mode != ForeignMode::Effect || !has_standard_shape(value) {
            return None;
        }
        let from = value.binding.from()?;
        if from != "flick:Effect" {
            return None;
        }
        let bind = value.binding.export_name().unwrap_or("");
        let stages = value.stages.len();
        if bind == "flickAll" && stages == 2 {
            return Some(self.intrinsic(
                Intrinsic::Async(AsyncIntrinsic::EffectAll),
                false,
                vec![],
                block,
                span,
            ));
        }
        let b = ExprBuilder::new(span, "$ec");
        let expr = effect_coll_template(&b, bind, stages)?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

fn effect_coll_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickForEach", 3) => Some(b.lam(
            &["conc", "f", "xs"],
            b.app2(
                b.global("Effect.Collections", "all"),
                b.loc("conc"),
                b.app2(
                    b.global("List", "map"),
                    b.lam(
                        &["x"],
                        b.at(ExprKind::Task(Box::new(b.app(b.loc("f"), b.loc("x"))))),
                    ),
                    b.loc("xs"),
                ),
            ),
        )),
        ("flickPartition", 3) => Some(partition_template(b)),
        ("flickValidate", 3) => Some(validate_template(b)),
        ("flickFilter", 3) => Some(filter_template(b)),
        ("filterMap", 2) => Some(filter_map_sync_template(b)),
        ("flickFilterMapEffect", 3) => Some(filter_map_effect_template(b)),
        ("findFirst", 2) => Some(find_first_template(b)),
        ("findFirstFilter", 2) => Some(find_first_filter_template(b)),
        ("firstSuccessOf", 1) => Some(first_success_of_template(b)),
        ("flickHead", 1) => Some(head_template(b)),
        ("flickReduce", 3) => Some(reduce_template(b)),
        ("filterOrElse", 3) => Some(b.lam(
            &["p", "fb", "t"],
            b.block(
                vec![b.let_("v", b.at(ExprKind::Await(Box::new(b.loc("t")))))],
                b.if_(
                    b.app(b.loc("p"), b.loc("v")),
                    b.loc("v"),
                    b.app(b.loc("fb"), b.loc("v")),
                ),
            ),
        )),
        ("filterMapOrElse", 3) => Some(b.lam(
            &["filt", "fb", "t"],
            b.block(
                vec![b.let_("v", b.at(ExprKind::Await(Box::new(b.loc("t")))))],
                b.match_(
                    b.app(b.loc("filt"), b.loc("v")),
                    vec![
                        arm(b.ok_pat("y"), b.loc("y")),
                        arm(b.err_pat("x"), b.app(b.loc("fb"), b.loc("x"))),
                    ],
                ),
            ),
        )),
        ("filterOrFail", 3) => Some(b.lam(
            &["p", "mk_err", "t"],
            b.block(
                vec![b.let_("v", b.at(ExprKind::Await(Box::new(b.loc("t")))))],
                b.if_(
                    b.app(b.loc("p"), b.loc("v")),
                    b.loc("v"),
                    b.app(b.global("Fail", "fail"), b.app(b.loc("mk_err"), b.loc("v"))),
                ),
            ),
        )),
        ("filterMapOrFail", 3) => Some(b.lam(
            &["filt", "mk_err", "t"],
            b.block(
                vec![b.let_("v", b.at(ExprKind::Await(Box::new(b.loc("t")))))],
                b.match_(
                    b.app(b.loc("filt"), b.loc("v")),
                    vec![
                        arm(b.ok_pat("y"), b.loc("y")),
                        arm(
                            b.err_pat("x"),
                            b.app(b.global("Fail", "fail"), b.app(b.loc("mk_err"), b.loc("x"))),
                        ),
                    ],
                ),
            ),
        )),
        _ => None,
    }
}

fn wrap_result_task(b: &ExprBuilder) -> Expr {
    let on_cause = b.lam(
        &["c"],
        b.match_(
            b.app(b.global("Cause", "find_error_option"), b.loc("c")),
            vec![
                arm(b.some_pat("e"), b.err(b.loc("e"))),
                arm(Pat::Wild, b.app(b.global("Failure", "rethrow"), b.loc("c"))),
            ],
        ),
    );
    b.lam(
        &["x"],
        b.at(ExprKind::Task(Box::new(b.app2(
            b.global("Effect.Errors", "catch_cause"),
            on_cause,
            b.at(ExprKind::Task(Box::new(
                b.ok(b.app(b.loc("f"), b.loc("x"))),
            ))),
        )))),
    )
}

fn partition_template(b: &ExprBuilder) -> Expr {
    let tasks = b.app2(b.global("List", "map"), wrap_result_task(b), b.loc("xs"));
    let split = b.for_ctrl(
        "part",
        "r",
        b.loc("res"),
        b.match_(
            b.loc("r"),
            vec![
                arm(
                    b.ok_pat("v"),
                    b.block(
                        vec![b.assign(
                            "oks",
                            b.app2(
                                b.global("List.Core", "append"),
                                b.var("oks"),
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
                            "errs",
                            b.app2(
                                b.global("List.Core", "append"),
                                b.var("errs"),
                                b.list(vec![b.loc("e")]),
                            ),
                        )],
                        b.unit(),
                    ),
                ),
            ],
        ),
    );
    b.lam(
        &["conc", "f", "xs"],
        b.block(
            vec![
                b.let_(
                    "res",
                    b.app2(b.global("Effect.Collections", "all"), b.loc("conc"), tasks),
                ),
                b.var_decl("oks", b.list(vec![])),
                b.var_decl("errs", b.list(vec![])),
                b.let_("_u", split),
            ],
            b.tuple(vec![b.var("oks"), b.var("errs")]),
        ),
    )
}

fn validate_template(b: &ExprBuilder) -> Expr {
    let val_err = b.at(ExprKind::Con(ExprBuilder::con(
        "Effect.Collections",
        "ValidationError",
        "ValidationError",
        1,
    )));
    let check_errs = b.match_(
        b.loc("errs"),
        vec![
            arm(Pat::List(vec![], None), b.loc("oks")),
            arm(
                Pat::List(
                    vec![Pat::Bind(b.name("h"))],
                    Some(Box::new(Pat::Bind(b.name("t")))),
                ),
                b.app(
                    b.global("Fail", "fail"),
                    b.app(
                        val_err,
                        b.app2(b.global("NonEmpty", "cons"), b.loc("h"), b.loc("t")),
                    ),
                ),
            ),
        ],
    );
    b.lam(
        &["conc", "f", "xs"],
        b.match_(
            b.app3(
                b.global("Effect.Collections", "partition"),
                b.loc("conc"),
                b.loc("f"),
                b.loc("xs"),
            ),
            vec![arm(
                Pat::Tuple(vec![Pat::Bind(b.name("oks")), Pat::Bind(b.name("errs"))]),
                check_errs,
            )],
        ),
    )
}

fn filter_template(b: &ExprBuilder) -> Expr {
    let tasks = b.app2(
        b.global("List", "map"),
        b.lam(
            &["x"],
            b.at(ExprKind::Task(Box::new(
                b.tuple(vec![b.loc("x"), b.app(b.loc("f"), b.loc("x"))]),
            ))),
        ),
        b.loc("xs"),
    );
    let body = b.each(
        "pair",
        b.loc("res"),
        b.match_(
            b.loc("pair"),
            vec![arm(
                Pat::Tuple(vec![Pat::Bind(b.name("x")), Pat::Bind(b.name("keep"))]),
                b.if_(b.loc("keep"), b.yield_(b.loc("x")), b.unit()),
            )],
        ),
    );
    b.lam(
        &["conc", "f", "xs"],
        b.block(
            vec![b.let_(
                "res",
                b.app2(b.global("Effect.Collections", "all"), b.loc("conc"), tasks),
            )],
            b.gen_strict(body),
        ),
    )
}

fn filter_map_sync_template(b: &ExprBuilder) -> Expr {
    let body = b.each(
        "x",
        b.loc("xs"),
        b.match_(
            b.app(b.loc("f"), b.loc("x")),
            vec![
                arm(b.ok_pat("v"), b.yield_(b.loc("v"))),
                arm(Pat::Wild, b.unit()),
            ],
        ),
    );
    b.lam(&["f", "xs"], b.gen_strict(body))
}

fn filter_map_effect_template(b: &ExprBuilder) -> Expr {
    let tasks = b.app2(
        b.global("List", "map"),
        b.lam(
            &["x"],
            b.at(ExprKind::Task(Box::new(b.app(b.loc("f"), b.loc("x"))))),
        ),
        b.loc("xs"),
    );
    let body = b.each(
        "r",
        b.loc("res"),
        b.match_(
            b.loc("r"),
            vec![
                arm(b.ok_pat("v"), b.yield_(b.loc("v"))),
                arm(Pat::Wild, b.unit()),
            ],
        ),
    );
    b.lam(
        &["conc", "f", "xs"],
        b.block(
            vec![b.let_(
                "res",
                b.app2(b.global("Effect.Collections", "all"), b.loc("conc"), tasks),
            )],
            b.gen_strict(body),
        ),
    )
}

fn find_first_template(b: &ExprBuilder) -> Expr {
    let body = b.for_ctrl(
        "ff",
        "x",
        b.loc("xs"),
        b.if_(
            b.app(b.loc("f"), b.loc("x")),
            b.block(
                vec![b.assign("found", b.some(b.loc("x")))],
                b.break_("ff", b.unit()),
            ),
            b.unit(),
        ),
    );
    b.lam(
        &["f", "xs"],
        b.block(
            vec![b.var_decl("found", b.none()), b.let_("_ff", body)],
            b.var("found"),
        ),
    )
}

fn find_first_filter_template(b: &ExprBuilder) -> Expr {
    let body = b.for_ctrl(
        "fff",
        "x",
        b.loc("xs"),
        b.match_(
            b.app(b.loc("f"), b.loc("x")),
            vec![
                arm(
                    b.ok_pat("v"),
                    b.block(
                        vec![b.assign("found", b.some(b.loc("v")))],
                        b.break_("fff", b.unit()),
                    ),
                ),
                arm(Pat::Wild, b.unit()),
            ],
        ),
    );
    b.lam(
        &["f", "xs"],
        b.block(
            vec![b.var_decl("found", b.none()), b.let_("_fff", body)],
            b.var("found"),
        ),
    )
}

fn first_success_of_template(b: &ExprBuilder) -> Expr {
    let save_err = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("ex")),
        vec![
            arm(
                b.some_pat("c"),
                b.if_(
                    b.app(b.global("Cause", "has_dies"), b.loc("c")),
                    b.app(b.global("Failure", "rethrow"), b.loc("c")),
                    b.if_(
                        b.app(b.global("Cause", "has_interrupts_only"), b.loc("c")),
                        b.app(b.global("Failure", "rethrow"), b.loc("c")),
                        b.block(vec![b.assign("last", b.some(b.loc("c")))], b.unit()),
                    ),
                ),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let step = b.for_ctrl(
        "fso",
        "t",
        b.loc("tasks"),
        b.block(
            vec![b.let_("ex", b.app(b.global("Effect.Core", "exit"), b.loc("t")))],
            b.match_(
                b.app(b.global("Exit", "value"), b.loc("ex")),
                vec![
                    arm(
                        b.some_pat("v"),
                        b.block(
                            vec![b.assign("found", b.some(b.loc("v")))],
                            b.break_("fso", b.unit()),
                        ),
                    ),
                    arm(Pat::Wild, save_err),
                ],
            ),
        ),
    );
    let after = b.match_(
        b.var("last"),
        vec![
            arm(
                b.some_pat("c"),
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
            arm(
                Pat::Wild,
                b.app(
                    b.global("Effect.Core", "die"),
                    b.at(ExprKind::Lit(flick_ir::Lit::String(
                        "Error: Received an empty collection of effects".into(),
                    ))),
                ),
            ),
        ],
    );
    b.lam(
        &["tasks"],
        b.block(
            vec![
                b.var_decl("last", b.none()),
                b.var_decl("found", b.none()),
                b.let_("_fso", step),
            ],
            b.match_(
                b.var("found"),
                vec![arm(b.some_pat("v"), b.loc("v")), arm(Pat::Wild, after)],
            ),
        ),
    )
}

fn head_template(b: &ExprBuilder) -> Expr {
    let no_elem = b.app(
        b.at(ExprKind::Con(ExprBuilder::con(
            "Cause",
            "NoSuchElementError",
            "NoSuchElementError",
            1,
        ))),
        b.str_(""),
    );
    b.lam(
        &["t"],
        b.match_(
            b.at(ExprKind::Await(Box::new(b.loc("t")))),
            vec![
                arm(
                    Pat::List(vec![Pat::Bind(b.name("h"))], Some(Box::new(Pat::Wild))),
                    b.loc("h"),
                ),
                arm(Pat::Wild, b.app(b.global("Fail", "fail"), no_elem)),
            ],
        ),
    )
}

fn reduce_template(b: &ExprBuilder) -> Expr {
    let step = b.for_ctrl(
        "red",
        "x",
        b.loc("xs"),
        b.block(
            vec![b.assign("acc", b.app2(b.loc("f"), b.var("acc"), b.loc("x")))],
            b.unit(),
        ),
    );
    b.lam(
        &["zero", "f", "xs"],
        b.block(
            vec![b.var_decl("acc", b.loc("zero")), b.let_("_u", step)],
            b.var("acc"),
        ),
    )
}
