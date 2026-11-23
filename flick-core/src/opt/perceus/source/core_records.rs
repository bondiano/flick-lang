//! Audited `flick:Record` and `flick:Struct:Record` dictionary adapters.

use flick_ir::memory::RecordIntrinsic;
use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, Intrinsic, Local, Lower, LowerError, Pat, Span, Stmt};

impl Lower<'_> {
    pub(super) fn core_record_foreign(
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

        if let Some(kind) = record_intrinsic(from, bind, stages) {
            return Some(self.intrinsic(Intrinsic::Record(kind), false, vec![], block, span));
        }
        if from != "flick:Record" {
            return None;
        }
        let b = ExprBuilder::new(span, "$rec");
        let expr = record_transform(&b, bind, stages)
            .or_else(|| record_query(&b, bind, stages))
            .or_else(|| super::core_records_ops::template(&b, bind, stages))?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

fn record_intrinsic(from: &str, bind: &str, stages: usize) -> Option<RecordIntrinsic> {
    let kind = match (from, bind) {
        ("flick:Record", "empty") => RecordIntrinsic::Empty,
        ("flick:Record", "isEmptyReadonlyRecord") => RecordIntrinsic::IsEmpty,
        ("flick:Record", "fromEntries") => RecordIntrinsic::FromEntries,
        ("flick:Struct", "Record") => RecordIntrinsic::FromKeys,
        ("flick:Record", "singleton") => RecordIntrinsic::Singleton,
        ("flick:Record", "toEntries") => RecordIntrinsic::ToEntries,
        ("flick:Record", "keys") => RecordIntrinsic::Keys,
        ("flick:Record", "values") => RecordIntrinsic::Values,
        ("flick:Record", "size") => RecordIntrinsic::Size,
        ("flick:Record", "has") => RecordIntrinsic::Has,
        ("flick:Record", "get") => RecordIntrinsic::Get,
        ("flick:Record", "replace") => RecordIntrinsic::Replace,
        ("flick:Record", "set") => RecordIntrinsic::Set,
        ("flick:Record", "remove") => RecordIntrinsic::Remove,
        ("flick:Record", "pop") => RecordIntrinsic::Pop,
        ("flick:Record", "difference") => RecordIntrinsic::Difference,
        _ => return None,
    };
    (kind.arity() == stages).then_some(kind)
}

fn map_entries(b: &ExprBuilder, body: Expr) -> Expr {
    let kv_pat = Pat::Tuple(vec![Pat::Bind(b.name("k")), Pat::Bind(b.name("v"))]);
    b.app(
        b.global("Record", "from_entries"),
        b.gen_strict(b.each(
            "pair",
            b.app(b.global("Record", "to_entries"), b.loc("r")),
            b.match_(b.loc("pair"), vec![arm(kv_pat, body)]),
        )),
    )
}

fn record_transform(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    let from_entries = |xs: Expr| b.app(b.global("Record", "from_entries"), xs);
    match (bind, stages) {
        ("fromIterableWith", 2) => Some(b.lam(
            &["f", "xs"],
            from_entries(b.gen_strict(b.each(
                "x",
                b.loc("xs"),
                b.yield_(b.app(b.loc("f"), b.loc("x"))),
            ))),
        )),
        ("fromIterableBy", 2) => Some(b.lam(
            &["f", "xs"],
            from_entries(b.gen_strict(b.each(
                "x",
                b.loc("xs"),
                b.yield_(b.tuple(vec![b.app(b.loc("f"), b.loc("x")), b.loc("x")])),
            ))),
        )),
        ("map", 2) => Some(b.lam(
            &["f", "r"],
            map_entries(
                b,
                b.yield_(b.tuple(vec![b.loc("k"), b.app2(b.loc("f"), b.loc("v"), b.loc("k"))])),
            ),
        )),
        ("mapKeys", 2) => Some(b.lam(
            &["f", "r"],
            map_entries(
                b,
                b.yield_(b.tuple(vec![b.app2(b.loc("f"), b.loc("k"), b.loc("v")), b.loc("v")])),
            ),
        )),
        ("mapEntries", 2) => Some(b.lam(
            &["f", "r"],
            map_entries(b, b.yield_(b.app2(b.loc("f"), b.loc("v"), b.loc("k")))),
        )),
        ("filter", 2) => Some(b.lam(
            &["p", "r"],
            map_entries(
                b,
                b.if_(
                    b.app2(b.loc("p"), b.loc("v"), b.loc("k")),
                    b.yield_(b.tuple(vec![b.loc("k"), b.loc("v")])),
                    b.unit(),
                ),
            ),
        )),
        ("filterMap", 2) => Some(b.lam(
            &["f", "r"],
            map_entries(
                b,
                b.match_(
                    b.app2(b.loc("f"), b.loc("v"), b.loc("k")),
                    vec![
                        arm(
                            b.ok_pat("y"),
                            b.yield_(b.tuple(vec![b.loc("k"), b.loc("y")])),
                        ),
                        arm(Pat::Wild, b.unit()),
                    ],
                ),
            ),
        )),
        ("getSomes", 1) => Some(b.lam(
            &["r"],
            map_entries(
                b,
                b.match_(
                    b.loc("v"),
                    vec![
                        arm(
                            b.some_pat("y"),
                            b.yield_(b.tuple(vec![b.loc("k"), b.loc("y")])),
                        ),
                        arm(Pat::Wild, b.unit()),
                    ],
                ),
            ),
        )),
        _ => None,
    }
}

fn record_query(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    let entries = |r: Expr| b.app(b.global("Record", "to_entries"), r);
    let kv_pat = || Pat::Tuple(vec![Pat::Bind(b.name("k")), Pat::Bind(b.name("v"))]);
    match (bind, stages) {
        ("collect", 2) => Some(b.lam(
            &["f", "r"],
            b.gen_strict(b.each(
                "pair",
                entries(b.loc("r")),
                b.match_(
                    b.loc("pair"),
                    vec![arm(
                        kv_pat(),
                        b.yield_(b.app2(b.loc("f"), b.loc("k"), b.loc("v"))),
                    )],
                ),
            )),
        )),
        ("modify", 3) => Some(b.lam(
            &["k", "f", "r"],
            b.match_(
                b.app2(b.global("Record", "get_key"), b.loc("k"), b.loc("r")),
                vec![
                    arm(
                        b.some_pat("v"),
                        b.some(b.app3(
                            b.global("Record", "set"),
                            b.loc("k"),
                            b.app(b.loc("f"), b.loc("v")),
                            b.loc("r"),
                        )),
                    ),
                    arm(Pat::Wild, b.none()),
                ],
            ),
        )),
        ("reduce", 3) => Some(b.lam(
            &["zero", "f", "r"],
            b.block(
                vec![
                    b.var_decl("acc", b.loc("zero")),
                    Stmt::Expr(b.each(
                        "pair",
                        entries(b.loc("r")),
                        b.match_(
                            b.loc("pair"),
                            vec![arm(
                                kv_pat(),
                                b.block(
                                    vec![b.assign(
                                        "acc",
                                        b.app3(b.loc("f"), b.var("acc"), b.loc("v"), b.loc("k")),
                                    )],
                                    b.unit(),
                                ),
                            )],
                        ),
                    )),
                ],
                b.var("acc"),
            ),
        )),
        ("findFirst", 2) => Some(b.lam(
            &["p", "r"],
            b.block(
                vec![
                    b.var_decl("found", b.none()),
                    Stmt::Expr(b.for_ctrl(
                        "$rff",
                        "pair",
                        entries(b.loc("r")),
                        b.match_(
                            b.loc("pair"),
                            vec![arm(
                                kv_pat(),
                                b.if_(
                                    b.app2(b.loc("p"), b.loc("v"), b.loc("k")),
                                    b.block(
                                        vec![b.assign(
                                            "found",
                                            b.some(b.tuple(vec![b.loc("k"), b.loc("v")])),
                                        )],
                                        b.break_("$rff", b.unit()),
                                    ),
                                    b.unit(),
                                ),
                            )],
                        ),
                    )),
                ],
                b.var("found"),
            ),
        )),
        _ => None,
    }
}
