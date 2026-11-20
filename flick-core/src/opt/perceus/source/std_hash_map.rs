//! Audited `EqHash` and `HashMap` pure IR adapters preserving exact HAMT walk order.

use flick_ir::memory::HashIntrinsic;
use flick_ir::{BinOp, Dict, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{
    Code, Env, Expr, ExprKind, Intrinsic, Local, Lower, LowerError, Operation, Pat, Span, Stmt,
};

impl Lower<'_> {
    pub(super) fn std_hash_map_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.mode != ForeignMode::Pure || !has_standard_shape(value) {
            return None;
        }
        let from = value.binding.from.as_deref()?;
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$hm");

        // Dictionary-taking constructors (`dicts == 1`).
        if value.dicts == 1 && stages == 0 {
            let expr = match (from, bind) {
                ("flick:rt", "eqHashCanonical") => Some(eq_hash_canonical(&b)),
                ("flick:rt", "hashMapEmpty") => Some(b.lam(
                    &["hdict"],
                    make_map(&b, witness_from_dict(&b, "hdict"), b.list(vec![])),
                )),
                ("flick:rt", "hashSetEmpty") => Some(super::std_hash_set::empty_from_dict(&b)),
                _ => None,
            }?;
            return Some(self.expr(&expr, &Env::new(), block));
        }
        if value.dicts != 0 {
            return None;
        }
        // Operations that need `HashIntrinsic::WalkCompare` receive it via outer application.
        if let Some(expr) = hash_map_ordered(&b, from, bind, stages) {
            return Some(self.apply_walk_compare(&expr, block, span));
        }
        let expr = hash_map_plain(&b, from, bind, stages)
            .or_else(|| super::std_hash_map_ops::template(&b, from, bind, stages))
            .or_else(|| super::std_hash_set::plain_template(&b, from, bind, stages))?;
        Some(self.expr(&expr, &Env::new(), block))
    }

    fn apply_walk_compare(
        &mut self,
        expr: &Expr,
        block: &mut Code,
        span: Span,
    ) -> Result<Local, LowerError> {
        let function = self.expr(expr, &Env::new(), block)?;
        let argument = self.intrinsic(
            Intrinsic::Hash(HashIntrinsic::WalkCompare),
            false,
            vec![],
            block,
            span,
        )?;
        block.emit(Operation::Apply { function, argument }, span)
    }
}

pub(super) fn witness_con() -> flick_ir::Constructor {
    ExprBuilder::con("EqHash", "EqHash", "$Witness", 2)
}

pub(super) fn map_con() -> flick_ir::Constructor {
    ExprBuilder::con("HashMap", "HashMap", "$Map", 2)
}

pub(super) fn witness_from_dict(b: &ExprBuilder, dict_local: &str) -> Expr {
    let eq_fn = b.at(ExprKind::Method {
        dict: Dict::Super {
            dict: Box::new(Dict::Field(b.name(dict_local))),
            class: "Eq".into(),
        },
        name: "eq".into(),
    });
    let hash_fn = b.at(ExprKind::Method {
        dict: Dict::Field(b.name(dict_local)),
        name: "hash".into(),
    });
    b.app2(b.at(ExprKind::Con(witness_con())), eq_fn, hash_fn)
}

fn eq_hash_canonical(b: &ExprBuilder) -> Expr {
    b.lam(&["hdict"], witness_from_dict(b, "hdict"))
}

pub(super) fn make_map(b: &ExprBuilder, w: Expr, entries: Expr) -> Expr {
    b.app2(b.at(ExprKind::Con(map_con())), w, entries)
}

pub(super) fn unwrap_map(b: &ExprBuilder, m: Expr, w: &str, es: &str, body: Expr) -> Expr {
    b.match_(
        m,
        vec![arm(
            Pat::Con(map_con(), vec![Pat::Bind(b.name(w)), Pat::Bind(b.name(es))]),
            body,
        )],
    )
}

pub(super) fn unwrap_witness(b: &ExprBuilder, w: Expr, eq: &str, hf: &str, body: Expr) -> Expr {
    b.match_(
        w,
        vec![arm(
            Pat::Con(
                witness_con(),
                vec![Pat::Bind(b.name(eq)), Pat::Bind(b.name(hf))],
            ),
            body,
        )],
    )
}

/// Inserts or replaces `(h, k, v)` in `entries` keeping exact HAMT walk order.
pub(super) fn insert_entry_ordered(
    b: &ExprBuilder,
    wcmp: Expr,
    eq_fn: Expr,
    h: Expr,
    k: Expr,
    v: Expr,
    entries: Expr,
) -> Expr {
    let e_pat = Pat::Tuple(vec![
        Pat::Bind(b.name("eh")),
        Pat::Bind(b.name("ek")),
        Pat::Bind(b.name("ev")),
    ]);
    let new_entry = || b.tuple(vec![b.loc("nh"), b.loc("nk"), b.loc("nv")]);
    b.gen_strict(b.block(
        vec![
            b.let_("nh", h),
            b.let_("nk", k),
            b.let_("nv", v),
            b.var_decl("done", b.bool_(false)),
            Stmt::Expr(b.each(
                "ent",
                entries,
                b.match_(
                    b.loc("ent"),
                    vec![arm(
                        e_pat,
                        b.if_(
                            b.var("done"),
                            b.yield_(b.tuple(vec![b.loc("eh"), b.loc("ek"), b.loc("ev")])),
                            b.match_(
                                b.app2(wcmp, b.loc("nh"), b.loc("eh")),
                                vec![
                                    arm(
                                        Pat::Con(
                                            ExprBuilder::con("Prelude", "Ordering", "LT", 0),
                                            vec![],
                                        ),
                                        b.block(
                                            vec![
                                                b.assign("done", b.bool_(true)),
                                                Stmt::Expr(b.yield_(new_entry())),
                                            ],
                                            b.yield_(b.tuple(vec![
                                                b.loc("eh"),
                                                b.loc("ek"),
                                                b.loc("ev"),
                                            ])),
                                        ),
                                    ),
                                    arm(
                                        Pat::Con(
                                            ExprBuilder::con("Prelude", "Ordering", "EQ", 0),
                                            vec![],
                                        ),
                                        b.if_(
                                            b.app2(eq_fn, b.loc("ek"), b.loc("nk")),
                                            b.block(
                                                vec![b.assign("done", b.bool_(true))],
                                                b.yield_(b.tuple(vec![
                                                    b.loc("eh"),
                                                    b.loc("nk"),
                                                    b.loc("nv"),
                                                ])),
                                            ),
                                            b.yield_(b.tuple(vec![
                                                b.loc("eh"),
                                                b.loc("ek"),
                                                b.loc("ev"),
                                            ])),
                                        ),
                                    ),
                                    arm(
                                        Pat::Wild,
                                        b.yield_(b.tuple(vec![
                                            b.loc("eh"),
                                            b.loc("ek"),
                                            b.loc("ev"),
                                        ])),
                                    ),
                                ],
                            ),
                        ),
                    )],
                ),
            )),
        ],
        b.if_(b.var("done"), b.unit(), b.yield_(new_entry())),
    ))
}

fn hash_map_ordered(b: &ExprBuilder, from: &str, bind: &str, stages: usize) -> Option<Expr> {
    match (from, bind, stages) {
        ("flick:rt", "hashMapInsert", 3) => Some(b.lam(
            &["wcmp", "k", "v", "m"],
            unwrap_map(
                b,
                b.loc("m"),
                "w",
                "es",
                unwrap_witness(
                    b,
                    b.loc("w"),
                    "eq",
                    "hf",
                    make_map(
                        b,
                        b.loc("w"),
                        insert_entry_ordered(
                            b,
                            b.loc("wcmp"),
                            b.loc("eq"),
                            b.app(b.loc("hf"), b.loc("k")),
                            b.loc("k"),
                            b.loc("v"),
                            b.loc("es"),
                        ),
                    ),
                ),
            ),
        )),
        ("flick:HashMap", "flickModifyHash", 4) => Some(modify_hash(b, true)),
        ("flick:HashMap", "flickModifyAt", 3) => Some(modify_hash(b, false)),
        _ => None,
    }
}

fn modify_hash(b: &ExprBuilder, explicit_hash: bool) -> Expr {
    let params: &[&str] = if explicit_hash {
        &["wcmp", "k", "h", "f", "m"]
    } else {
        &["wcmp", "k", "f", "m"]
    };
    let h_expr = if explicit_hash {
        b.loc("h")
    } else {
        b.app(b.loc("hf"), b.loc("k"))
    };
    let body = unwrap_map(
        b,
        b.loc("m"),
        "w",
        "es",
        unwrap_witness(
            b,
            b.loc("w"),
            "eq",
            "hf",
            b.block(
                vec![
                    b.let_("kh", h_expr),
                    b.let_(
                        "cur",
                        b.app3(
                            b.global("HashMap", "get_hash"),
                            b.loc("k"),
                            b.loc("kh"),
                            b.loc("m"),
                        ),
                    ),
                ],
                b.match_(
                    b.app(b.loc("f"), b.loc("cur")),
                    vec![
                        arm(
                            b.some_pat("nv"),
                            make_map(
                                b,
                                b.loc("w"),
                                insert_entry_ordered(
                                    b,
                                    b.loc("wcmp"),
                                    b.loc("eq"),
                                    b.loc("kh"),
                                    b.loc("k"),
                                    b.loc("nv"),
                                    b.loc("es"),
                                ),
                            ),
                        ),
                        arm(
                            Pat::Wild,
                            make_map(
                                b,
                                b.loc("w"),
                                remove_by_hash(
                                    b,
                                    b.loc("eq"),
                                    b.loc("kh"),
                                    b.loc("k"),
                                    b.loc("es"),
                                ),
                            ),
                        ),
                    ],
                ),
            ),
        ),
    );
    b.lam(params, body)
}

fn remove_by_hash(b: &ExprBuilder, eq_fn: Expr, h: Expr, k: Expr, entries: Expr) -> Expr {
    let e_pat = Pat::Tuple(vec![
        Pat::Bind(b.name("eh")),
        Pat::Bind(b.name("ek")),
        Pat::Bind(b.name("ev")),
    ]);
    b.gen_strict(b.each(
        "ent",
        entries,
        b.match_(
            b.loc("ent"),
            vec![arm(
                e_pat,
                b.if_(
                    b.bin(
                        BinOp::And,
                        b.bin(BinOp::Eq, b.loc("eh"), h),
                        b.app2(eq_fn, b.loc("ek"), k),
                    ),
                    b.unit(),
                    b.yield_(b.tuple(vec![b.loc("eh"), b.loc("ek"), b.loc("ev")])),
                ),
            )],
        ),
    ))
}

fn hash_map_plain(b: &ExprBuilder, from: &str, bind: &str, stages: usize) -> Option<Expr> {
    let e_pat = || {
        Pat::Tuple(vec![
            Pat::Bind(b.name("eh")),
            Pat::Bind(b.name("ek")),
            Pat::Bind(b.name("ev")),
        ])
    };
    let project_entries = |yielded: Expr| {
        b.lam(
            &["m"],
            unwrap_map(
                b,
                b.loc("m"),
                "_w",
                "es",
                b.gen_strict(b.each(
                    "ent",
                    b.loc("es"),
                    b.match_(b.loc("ent"), vec![arm(e_pat(), b.yield_(yielded))]),
                )),
            ),
        )
    };
    match (from, bind, stages) {
        ("flick:rt", "eqHashWith", 3) => Some(b.lam(
            &["eq", "hf", "body"],
            b.app(
                b.loc("body"),
                b.app2(b.at(ExprKind::Con(witness_con())), b.loc("eq"), b.loc("hf")),
            ),
        )),
        ("flick:rt", "hashMapWith", 1) => {
            Some(b.lam(&["w"], make_map(b, b.loc("w"), b.list(vec![]))))
        }
        ("flick:rt", "hashMapSize", 1) => Some(b.lam(
            &["m"],
            unwrap_map(
                b,
                b.loc("m"),
                "_w",
                "es",
                b.app(b.global("List.Core", "length"), b.loc("es")),
            ),
        )),
        ("flick:HashMap", "flickIsEmpty", 1) => Some(b.lam(
            &["m"],
            b.bin(
                BinOp::Eq,
                b.app(b.global("HashMap", "size"), b.loc("m")),
                b.int(0),
            ),
        )),
        ("flick:rt", "hashMapToList", 1) => {
            Some(project_entries(b.tuple(vec![b.loc("ek"), b.loc("ev")])))
        }
        ("flick:HashMap", "flickKeys", 1) => Some(project_entries(b.loc("ek"))),
        ("flick:HashMap", "flickValues", 1) => Some(project_entries(b.loc("ev"))),
        _ => hash_map_lookup(b, from, bind, stages),
    }
}

fn hash_map_lookup(b: &ExprBuilder, from: &str, bind: &str, stages: usize) -> Option<Expr> {
    match (from, bind, stages) {
        ("flick:rt", "hashMapRemove", 2) => Some(b.lam(
            &["k", "m"],
            unwrap_map(
                b,
                b.loc("m"),
                "w",
                "es",
                unwrap_witness(
                    b,
                    b.loc("w"),
                    "eq",
                    "hf",
                    make_map(
                        b,
                        b.loc("w"),
                        remove_by_hash(
                            b,
                            b.loc("eq"),
                            b.app(b.loc("hf"), b.loc("k")),
                            b.loc("k"),
                            b.loc("es"),
                        ),
                    ),
                ),
            ),
        )),
        ("flick:rt", "hashMapGet", 2) => Some(b.lam(
            &["k", "m"],
            unwrap_map(
                b,
                b.loc("m"),
                "w",
                "_es",
                unwrap_witness(
                    b,
                    b.loc("w"),
                    "_eq",
                    "hf",
                    b.app3(
                        b.global("HashMap", "get_hash"),
                        b.loc("k"),
                        b.app(b.loc("hf"), b.loc("k")),
                        b.loc("m"),
                    ),
                ),
            ),
        )),
        ("flick:HashMap", "flickGetHash", 3) => Some(get_by_hash(b)),
        ("flick:HashMap", "flickHas", 2) => Some(b.lam(
            &["k", "m"],
            b.match_(
                b.app2(b.global("HashMap", "get"), b.loc("k"), b.loc("m")),
                vec![
                    arm(b.some_pat("_v"), b.bool_(true)),
                    arm(Pat::Wild, b.bool_(false)),
                ],
            ),
        )),
        ("flick:HashMap", "flickHasHash", 3) => Some(b.lam(
            &["k", "h", "m"],
            b.match_(
                b.app3(
                    b.global("HashMap", "get_hash"),
                    b.loc("k"),
                    b.loc("h"),
                    b.loc("m"),
                ),
                vec![
                    arm(b.some_pat("_v"), b.bool_(true)),
                    arm(Pat::Wild, b.bool_(false)),
                ],
            ),
        )),
        _ => None,
    }
}

fn get_by_hash(b: &ExprBuilder) -> Expr {
    let e_pat = Pat::Tuple(vec![
        Pat::Bind(b.name("eh")),
        Pat::Bind(b.name("ek")),
        Pat::Bind(b.name("ev")),
    ]);
    b.lam(
        &["k", "h", "m"],
        unwrap_map(
            b,
            b.loc("m"),
            "w",
            "es",
            unwrap_witness(
                b,
                b.loc("w"),
                "eq",
                "_hf",
                b.block(
                    vec![
                        b.var_decl("found", b.none()),
                        Stmt::Expr(b.for_ctrl(
                            "$hmget",
                            "ent",
                            b.loc("es"),
                            b.match_(
                                b.loc("ent"),
                                vec![arm(
                                    e_pat,
                                    b.if_(
                                        b.bin(
                                            BinOp::And,
                                            b.bin(BinOp::Eq, b.loc("eh"), b.loc("h")),
                                            b.app2(b.loc("eq"), b.loc("ek"), b.loc("k")),
                                        ),
                                        b.block(
                                            vec![b.assign("found", b.some(b.loc("ev")))],
                                            b.break_("$hmget", b.unit()),
                                        ),
                                        b.unit(),
                                    ),
                                )],
                            ),
                        )),
                    ],
                    b.var("found"),
                ),
            ),
        ),
    )
}
