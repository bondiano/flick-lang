//! Audited `PartitionedSemaphore` pure IR adapter (Stage 4B).
//!
//! Fair round-robin per-partition permit pool built on Stage 4A `Ref`,
//! `Deferred`, `Effect.uninterruptible_mask`, `Effect.ensuring`, and
//! `Effect.on_interrupt`.

use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, Local, Lower, LowerError, Pat, Span, Stmt};

impl Lower<'_> {
    pub(super) fn std_part_sem_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        if value.binding.from.as_deref()? != "flick:PartitionedSemaphore" {
            return None;
        }
        let b = ExprBuilder::new(span, "$psem");
        let expr = part_sem_template(
            &b,
            value.binding.name.as_str(),
            value.mode,
            value.stages.len(),
        )?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

// Representation: `(cap : Int, state : Ref (avail : Int, next_id : Int, queues : List (k, List (Int, Int, Deferred none Unit))))`
fn part_sem_template(
    b: &ExprBuilder,
    bind: &str,
    mode: ForeignMode,
    stages: usize,
) -> Option<Expr> {
    if mode == ForeignMode::Pure && bind == "capacity" && stages == 1 {
        return Some(b.lam(
            &["sem"],
            b.match_(
                b.loc("sem"),
                vec![arm(b.tup_pat(&["cap", "_r"]), b.loc("cap"))],
            ),
        ));
    }
    if mode != ForeignMode::Effect {
        return None;
    }
    match (bind, stages) {
        ("make", 1) => Some(b.lam(
            &["opts"],
            b.block(
                vec![b.let_("cap", b.field(b.loc("opts"), "permits"))],
                b.tuple(vec![
                    b.loc("cap"),
                    b.rmake(b.tuple(vec![b.loc("cap"), b.int(0), b.list(vec![])])),
                ]),
            ),
        )),
        ("available", 1) => Some(b.lam(
            &["sem"],
            b.match_(
                b.loc("sem"),
                vec![arm(
                    b.tup_pat(&["_cap", "r"]),
                    b.match_(
                        b.rget(b.loc("r")),
                        vec![arm(b.tup_pat(&["av", "_nid", "_qs"]), b.loc("av"))],
                    ),
                )],
            ),
        )),
        ("take", 3) => Some(take_template(b)),
        ("release", 2) => Some(release_template(b)),
        ("withPermits", 4) => Some(with_permits_template(b)),
        ("withPermit", 3) => Some(b.lam(
            &["sem", "k", "t"],
            b.app4(
                b.global("PartitionedSemaphore", "with_permits"),
                b.loc("sem"),
                b.loc("k"),
                b.int(1),
                b.loc("t"),
            ),
        )),
        ("withPermitsIfAvailable", 3) => Some(with_permits_if_avail_template(b)),
        _ => None,
    }
}

fn enqueue_waiter_expr(b: &ExprBuilder) -> Expr {
    // Appends `(nid, n, d)` to partition `k` in `qs` (or adds `(k, [(nid, n, d)])` at end).
    let entry = b.tuple(vec![b.loc("nid"), b.loc("n"), b.loc("d")]);
    let step = b.for_ctrl(
        "enq",
        "part",
        b.loc("qs"),
        b.match_(
            b.loc("part"),
            vec![arm(
                b.tup_pat(&["pk", "ws"]),
                b.if_(
                    b.bin(BinOp::Eq, b.loc("pk"), b.loc("k")),
                    b.block(
                        vec![
                            b.assign("found", b.bool_(true)),
                            b.assign(
                                "out",
                                b.app2(
                                    b.global("List.Core", "append"),
                                    b.var("out"),
                                    b.list(vec![b.tuple(vec![
                                        b.loc("pk"),
                                        b.app2(
                                            b.global("List.Core", "append"),
                                            b.loc("ws"),
                                            b.list(vec![entry.clone()]),
                                        ),
                                    ])]),
                                ),
                            ),
                        ],
                        b.unit(),
                    ),
                    b.block(
                        vec![b.assign(
                            "out",
                            b.app2(
                                b.global("List.Core", "append"),
                                b.var("out"),
                                b.list(vec![b.tuple(vec![b.loc("pk"), b.loc("ws")])]),
                            ),
                        )],
                        b.unit(),
                    ),
                ),
            )],
        ),
    );
    b.block(
        vec![
            b.var_decl("found", b.bool_(false)),
            b.var_decl("out", b.list(vec![])),
            b.let_("_u", step),
        ],
        b.if_(
            b.var("found"),
            b.var("out"),
            b.app2(
                b.global("List.Core", "append"),
                b.var("out"),
                b.list(vec![b.tuple(vec![b.loc("k"), b.list(vec![entry])])]),
            ),
        ),
    )
}

fn remove_waiter_expr(b: &ExprBuilder) -> Expr {
    // Removes waiter with id `nid` from `r`'s queues if still waiting.
    let filter_ws = b.app2(
        b.global("List.Core", "filter"),
        b.lam(
            &["w"],
            b.match_(
                b.loc("w"),
                vec![arm(
                    b.tup_pat(&["wid", "_wn", "_wd"]),
                    b.not(b.bin(BinOp::Eq, b.loc("wid"), b.loc("nid"))),
                )],
            ),
        ),
        b.loc("ws"),
    );
    let step = b.for_ctrl(
        "rmq",
        "part",
        b.loc("qs2"),
        b.match_(
            b.loc("part"),
            vec![arm(
                b.tup_pat(&["pk", "ws"]),
                b.block(
                    vec![b.let_("ws2", filter_ws)],
                    b.match_(
                        b.loc("ws2"),
                        vec![
                            arm(Pat::List(vec![], None), b.unit()),
                            arm(
                                Pat::Wild,
                                b.block(
                                    vec![b.assign(
                                        "nq",
                                        b.app2(
                                            b.global("List.Core", "append"),
                                            b.var("nq"),
                                            b.list(vec![b.tuple(vec![b.loc("pk"), b.loc("ws2")])]),
                                        ),
                                    )],
                                    b.unit(),
                                ),
                            ),
                        ],
                    ),
                ),
            )],
        ),
    );
    b.match_(
        b.rget(b.loc("r")),
        vec![arm(
            b.tup_pat(&["av2", "nid2", "qs2"]),
            b.block(
                vec![
                    b.var_decl("nq", b.list(vec![])),
                    b.let_("_u", step),
                    b.rset(
                        b.loc("r"),
                        b.tuple(vec![b.loc("av2"), b.loc("nid2"), b.var("nq")]),
                    ),
                ],
                b.unit(),
            ),
        )],
    )
}

fn take_template(b: &ExprBuilder) -> Expr {
    let wait_branch = b.block(
        vec![
            b.let_("d", b.dmake()),
            b.let_("qs1", enqueue_waiter_expr(b)),
            b.rset(
                b.loc("r"),
                b.tuple(vec![
                    b.loc("av"),
                    b.add_i(b.loc("nid"), b.int(1)),
                    b.loc("qs1"),
                ]),
            ),
        ],
        b.app2(
            b.global("Effect.Errors", "on_interrupt"),
            b.lam(&["_c"], b.task(remove_waiter_expr(b))),
            b.app(b.loc("restore"), b.task(b.dawait(b.loc("d")))),
        ),
    );
    let masked_body = b.lam(
        &["restore"],
        b.match_(
            b.rget(b.loc("r")),
            vec![arm(
                b.tup_pat(&["av", "nid", "qs"]),
                b.if_(
                    b.bin(
                        BinOp::And,
                        b.bin(BinOp::Ge, b.loc("av"), b.loc("n")),
                        b.match_(
                            b.loc("qs"),
                            vec![
                                arm(Pat::List(vec![], None), b.bool_(true)),
                                arm(Pat::Wild, b.bool_(false)),
                            ],
                        ),
                    ),
                    b.block(
                        vec![b.rset(
                            b.loc("r"),
                            b.tuple(vec![
                                b.sub_i(b.loc("av"), b.loc("n")),
                                b.loc("nid"),
                                b.loc("qs"),
                            ]),
                        )],
                        b.unit(),
                    ),
                    wait_branch,
                ),
            )],
        ),
    );
    b.lam(
        &["k", "n", "sem"],
        b.if_(
            b.bin(BinOp::Le, b.loc("n"), b.int(0)),
            b.unit(),
            b.match_(
                b.loc("sem"),
                vec![arm(
                    b.tup_pat(&["_cap", "r"]),
                    b.app(
                        b.global("Effect.Concurrency", "uninterruptible_mask"),
                        masked_body,
                    ),
                )],
            ),
        ),
    )
}

fn release_serve_loop(b: &ExprBuilder) -> Expr {
    // Round-robin wake across partitions while front waiter fits available permits.
    b.loop_ctrl(
        "rel",
        b.match_(
            b.var("qs"),
            vec![
                arm(Pat::List(vec![], None), b.break_("rel", b.unit())),
                arm(
                    Pat::List(
                        vec![b.tup_pat(&["pk", "ws"])],
                        Some(Box::new(Pat::Bind(b.name("rest")))),
                    ),
                    b.match_(
                        b.loc("ws"),
                        vec![
                            arm(
                                Pat::List(vec![], None),
                                b.block(vec![b.assign("qs", b.loc("rest"))], b.unit()),
                            ),
                            arm(
                                Pat::List(
                                    vec![b.tup_pat(&["_wid", "need", "wd"])],
                                    Some(Box::new(Pat::Bind(b.name("wtail")))),
                                ),
                                b.if_(
                                    b.bin(BinOp::Le, b.loc("need"), b.var("av")),
                                    b.block(
                                        vec![
                                            b.assign("av", b.sub_i(b.var("av"), b.loc("need"))),
                                            b.assign(
                                                "to_wake",
                                                b.app2(
                                                    b.global("List.Core", "append"),
                                                    b.var("to_wake"),
                                                    b.list(vec![b.loc("wd")]),
                                                ),
                                            ),
                                            b.assign(
                                                "qs",
                                                b.match_(
                                                    b.loc("wtail"),
                                                    vec![
                                                        arm(Pat::List(vec![], None), b.loc("rest")),
                                                        arm(
                                                            Pat::Wild,
                                                            b.app2(
                                                                b.global("List.Core", "append"),
                                                                b.loc("rest"),
                                                                b.list(vec![b.tuple(vec![
                                                                    b.loc("pk"),
                                                                    b.loc("wtail"),
                                                                ])]),
                                                            ),
                                                        ),
                                                    ],
                                                ),
                                            ),
                                        ],
                                        b.unit(),
                                    ),
                                    b.break_("rel", b.unit()),
                                ),
                            ),
                        ],
                    ),
                ),
            ],
        ),
    )
}

fn release_template(b: &ExprBuilder) -> Expr {
    let wake_all = b.for_ctrl(
        "wk",
        "wd",
        b.var("to_wake"),
        b.block(vec![b.dsucceed(b.unit(), b.loc("wd"))], b.unit()),
    );
    b.lam(
        &["n", "sem"],
        b.match_(
            b.loc("sem"),
            vec![arm(
                b.tup_pat(&["cap", "r"]),
                b.match_(
                    b.rget(b.loc("r")),
                    vec![arm(
                        b.tup_pat(&["av0", "nid", "qs0"]),
                        b.block(
                            vec![
                                b.let_("sum", b.add_i(b.loc("av0"), b.loc("n"))),
                                b.var_decl(
                                    "av",
                                    b.if_(
                                        b.bin(BinOp::Gt, b.loc("sum"), b.loc("cap")),
                                        b.loc("cap"),
                                        b.loc("sum"),
                                    ),
                                ),
                                b.var_decl("qs", b.loc("qs0")),
                                b.var_decl("to_wake", b.list(vec![])),
                                b.let_("_u1", release_serve_loop(b)),
                                b.rset(
                                    b.loc("r"),
                                    b.tuple(vec![b.var("av"), b.loc("nid"), b.var("qs")]),
                                ),
                                b.let_("_u2", wake_all),
                            ],
                            b.var("av"),
                        ),
                    )],
                ),
            )],
        ),
    )
}

fn with_permits_template(b: &ExprBuilder) -> Expr {
    let body = b.lam(
        &["restore"],
        b.block(
            vec![Stmt::Expr(b.await_(b.app(
                b.loc("restore"),
                b.task(b.app3(
                    b.global("PartitionedSemaphore", "take"),
                    b.loc("k"),
                    b.loc("n"),
                    b.loc("sem"),
                )),
            )))],
            b.app2(
                b.global("Effect.Errors", "ensuring"),
                b.task(b.block(
                    vec![b.let_(
                        "_r",
                        b.app2(
                            b.global("PartitionedSemaphore", "release"),
                            b.loc("n"),
                            b.loc("sem"),
                        ),
                    )],
                    b.unit(),
                )),
                b.app(b.loc("restore"), b.loc("t")),
            ),
        ),
    );
    b.lam(
        &["sem", "k", "n", "t"],
        b.app(b.global("Effect.Concurrency", "uninterruptible_mask"), body),
    )
}

fn with_permits_if_avail_template(b: &ExprBuilder) -> Expr {
    let body = b.lam(
        &["restore"],
        b.match_(
            b.loc("sem"),
            vec![arm(
                b.tup_pat(&["_cap", "r"]),
                b.match_(
                    b.rget(b.loc("r")),
                    vec![arm(
                        b.tup_pat(&["av", "nid", "qs"]),
                        b.if_(
                            b.bin(BinOp::Lt, b.loc("av"), b.loc("n")),
                            b.none(),
                            b.block(
                                vec![b.rset(
                                    b.loc("r"),
                                    b.tuple(vec![
                                        b.sub_i(b.loc("av"), b.loc("n")),
                                        b.loc("nid"),
                                        b.loc("qs"),
                                    ]),
                                )],
                                b.some(b.app2(
                                    b.global("Effect.Errors", "ensuring"),
                                    b.task(b.block(
                                        vec![b.let_(
                                            "_r",
                                            b.app2(
                                                b.global("PartitionedSemaphore", "release"),
                                                b.loc("n"),
                                                b.loc("sem"),
                                            ),
                                        )],
                                        b.unit(),
                                    )),
                                    b.app(b.loc("restore"), b.loc("t")),
                                )),
                            ),
                        ),
                    )],
                ),
            )],
        ),
    );
    b.lam(
        &["sem", "n", "t"],
        b.app(b.global("Effect.Concurrency", "uninterruptible_mask"), body),
    )
}
