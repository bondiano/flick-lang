//! Audited `Latch`, `Semaphore`, and `SynchronizedRef` pure IR adapters (Stage 4B).
//!
//! Built directly on Stage 4A `Ref`, `Deferred`, `Effect.uninterruptible_mask`,
//! and `Effect.ensuring` — zero extra VM state or leak surface.

mod syncref;

use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Env, Expr, ExprKind, Local, Lower, LowerError, Pat, Span, Stmt};

impl Lower<'_> {
    pub(super) fn std_sync_foreign(
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
        let b = ExprBuilder::new(span, "$syn");

        let expr = match from {
            "flick:Latch" => latch_template(&b, bind, value.mode, stages)?,
            "flick:Semaphore" if value.mode == ForeignMode::Effect => {
                semaphore_template(&b, bind, stages)?
            }
            "flick:SynchronizedRef" if value.mode == ForeignMode::Effect => {
                syncref::syncref_template(&b, bind, stages, false)?
            }
            "flick:SubscriptionRef" if value.mode == ForeignMode::Effect => {
                syncref::syncref_template(&b, bind, stages, true)?
            }
            "flick:SubscriptionRef"
                if value.mode == ForeignMode::Pure && bind == "changes" && stages == 1 =>
            {
                b.lam(
                    &["r"],
                    b.match_(
                        b.loc("r"),
                        vec![arm(
                            b.tup_pat(&["_", "_", "hub"]),
                            b.app2(
                                b.global("$rt", "Helper.ch_from_pubsub"),
                                b.bool_(true),
                                b.loc("hub"),
                            ),
                        )],
                    ),
                )
            }
            _ => return None,
        };
        Some(self.expr(&expr, &Env::new(), block))
    }
}

fn rget(b: &ExprBuilder, r: Expr) -> Expr {
    b.app(b.global("Ref", "get"), r)
}

fn rset(b: &ExprBuilder, r: Expr, v: Expr) -> Stmt {
    Stmt::Expr(b.app2(b.global("Ref", "set"), r, v))
}

fn rmake(b: &ExprBuilder, v: Expr) -> Expr {
    b.app(b.global("Ref", "make"), v)
}

fn dmake(b: &ExprBuilder) -> Expr {
    b.app(b.global("Deferred", "make"), b.unit())
}

fn dsucceed(b: &ExprBuilder, d: Expr) -> Stmt {
    b.let_("_dw", b.app2(b.global("Deferred", "succeed"), b.unit(), d))
}

fn dawait(b: &ExprBuilder, d: Expr) -> Expr {
    b.app(b.global("Deferred", "await_"), d)
}

fn task_of(b: &ExprBuilder, body: Expr) -> Expr {
    b.at(ExprKind::Task(Box::new(body)))
}

fn await_of(b: &ExprBuilder, t: Expr) -> Expr {
    b.at(ExprKind::Await(Box::new(t)))
}

fn latch_template(b: &ExprBuilder, bind: &str, mode: ForeignMode, stages: usize) -> Option<Expr> {
    let split = |body: Expr| {
        b.match_(
            rget(b, b.loc("l")),
            vec![arm(
                Pat::Tuple(vec![Pat::Bind(b.name("op")), Pat::Bind(b.name("w"))]),
                body,
            )],
        )
    };
    if mode == ForeignMode::Sync && bind == "isOpen" && stages == 1 {
        return Some(b.lam(&["l"], split(b.loc("op"))));
    }
    if mode != ForeignMode::Effect {
        return None;
    }
    match (bind, stages) {
        ("make", 1) => Some(b.lam(&["op"], rmake(b, b.tuple(vec![b.loc("op"), dmake(b)])))),
        ("open", 1) => Some(b.lam(
            &["l"],
            split(b.if_(
                b.loc("op"),
                b.bool_(false),
                b.block(
                    vec![
                        rset(b, b.loc("l"), b.tuple(vec![b.bool_(true), dmake(b)])),
                        dsucceed(b, b.loc("w")),
                    ],
                    b.bool_(true),
                ),
            )),
        )),
        ("release", 1) => Some(b.lam(
            &["l"],
            split(b.if_(
                b.loc("op"),
                b.bool_(false),
                b.block(
                    vec![
                        rset(b, b.loc("l"), b.tuple(vec![b.bool_(false), dmake(b)])),
                        dsucceed(b, b.loc("w")),
                    ],
                    b.bool_(true),
                ),
            )),
        )),
        ("close", 1) => Some(b.lam(
            &["l"],
            split(b.block(
                vec![rset(
                    b,
                    b.loc("l"),
                    b.tuple(vec![b.bool_(false), b.loc("w")]),
                )],
                b.loc("op"),
            )),
        )),
        ("await", 1) => Some(b.lam(
            &["l"],
            split(b.if_(b.loc("op"), b.unit(), dawait(b, b.loc("w")))),
        )),
        ("whenOpen", 2) => Some(b.lam(
            &["l", "t"],
            b.block(
                vec![Stmt::Expr(b.app(b.global("Latch", "await_"), b.loc("l")))],
                await_of(b, b.loc("t")),
            ),
        )),
        _ => None,
    }
}

fn sem_split(b: &ExprBuilder, body: Expr) -> Expr {
    b.match_(
        rget(b, b.loc("s")),
        vec![arm(
            Pat::Tuple(vec![
                Pat::Bind(b.name("p")),
                Pat::Bind(b.name("tk")),
                Pat::Bind(b.name("w")),
            ]),
            body,
        )],
    )
}

fn sem_ensuring_release(b: &ExprBuilder) -> Expr {
    b.app2(
        b.global("Effect.Errors", "ensuring"),
        task_of(
            b,
            b.block(
                vec![b.let_(
                    "_r",
                    b.app2(b.global("Semaphore", "release"), b.loc("n"), b.loc("s")),
                )],
                b.unit(),
            ),
        ),
        b.app(b.loc("restore"), b.loc("t")),
    )
}

fn sem_resize_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["n", "s"],
        sem_split(
            b,
            b.if_(
                b.bin(BinOp::Ge, b.loc("n"), b.loc("tk")),
                b.block(
                    vec![
                        rset(
                            b,
                            b.loc("s"),
                            b.tuple(vec![b.loc("n"), b.loc("tk"), dmake(b)]),
                        ),
                        dsucceed(b, b.loc("w")),
                    ],
                    b.unit(),
                ),
                b.block(
                    vec![rset(
                        b,
                        b.loc("s"),
                        b.tuple(vec![b.loc("n"), b.loc("tk"), b.loc("w")]),
                    )],
                    b.unit(),
                ),
            ),
        ),
    )
}

fn sem_take_template(b: &ExprBuilder) -> Expr {
    let step = sem_split(
        b,
        b.if_(
            b.bin(BinOp::Ge, b.sub_i(b.loc("p"), b.loc("tk")), b.loc("n")),
            b.block(
                vec![rset(
                    b,
                    b.loc("s"),
                    b.tuple(vec![
                        b.loc("p"),
                        b.add_i(b.loc("tk"), b.loc("n")),
                        b.loc("w"),
                    ]),
                )],
                b.break_("sem", b.loc("n")),
            ),
            dawait(b, b.loc("w")),
        ),
    );
    b.lam(
        &["n", "s"],
        b.at(ExprKind::ControlLoop {
            id: "sem".into(),
            condition: None,
            body: Box::new(step),
            iterable: false,
        }),
    )
}

fn sem_with_permits_template(b: &ExprBuilder) -> Expr {
    let body = b.lam(
        &["restore"],
        b.block(
            vec![b.let_(
                "_tk",
                await_of(
                    b,
                    b.app(
                        b.loc("restore"),
                        task_of(
                            b,
                            b.app2(b.global("Semaphore", "take"), b.loc("n"), b.loc("s")),
                        ),
                    ),
                ),
            )],
            sem_ensuring_release(b),
        ),
    );
    b.lam(
        &["s", "n", "t"],
        b.app(b.global("Effect.Concurrency", "uninterruptible_mask"), body),
    )
}

fn sem_with_permits_if_avail_template(b: &ExprBuilder) -> Expr {
    let body = b.lam(
        &["restore"],
        b.if_(
            b.app2(
                b.global("Semaphore", "take_if_available"),
                b.loc("n"),
                b.loc("s"),
            ),
            b.some(sem_ensuring_release(b)),
            b.none(),
        ),
    );
    b.lam(
        &["s", "n", "t"],
        b.app(b.global("Effect.Concurrency", "uninterruptible_mask"), body),
    )
}

fn semaphore_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("make", 1) => Some(b.lam(
            &["p"],
            rmake(b, b.tuple(vec![b.loc("p"), b.int(0), dmake(b)])),
        )),
        ("release", 2) => Some(b.lam(
            &["n", "s"],
            sem_split(
                b,
                b.block(
                    vec![
                        b.let_("nt", b.sub_i(b.loc("tk"), b.loc("n"))),
                        rset(
                            b,
                            b.loc("s"),
                            b.tuple(vec![b.loc("p"), b.loc("nt"), dmake(b)]),
                        ),
                        dsucceed(b, b.loc("w")),
                    ],
                    b.sub_i(b.loc("p"), b.loc("nt")),
                ),
            ),
        )),
        ("releaseAll", 1) => Some(b.lam(
            &["s"],
            sem_split(
                b,
                b.block(
                    vec![
                        rset(b, b.loc("s"), b.tuple(vec![b.loc("p"), b.int(0), dmake(b)])),
                        dsucceed(b, b.loc("w")),
                    ],
                    b.loc("p"),
                ),
            ),
        )),
        ("resize", 2) => Some(sem_resize_template(b)),
        ("takeIfAvailable", 2) => Some(b.lam(
            &["n", "s"],
            sem_split(
                b,
                b.if_(
                    b.bin(BinOp::Lt, b.sub_i(b.loc("p"), b.loc("tk")), b.loc("n")),
                    b.bool_(false),
                    b.block(
                        vec![rset(
                            b,
                            b.loc("s"),
                            b.tuple(vec![
                                b.loc("p"),
                                b.add_i(b.loc("tk"), b.loc("n")),
                                b.loc("w"),
                            ]),
                        )],
                        b.bool_(true),
                    ),
                ),
            ),
        )),
        ("take", 2) => Some(sem_take_template(b)),
        ("withPermits", 3) => Some(sem_with_permits_template(b)),
        ("withPermit", 2) => Some(b.lam(
            &["s", "t"],
            b.app3(
                b.global("Semaphore", "with_permits"),
                b.loc("s"),
                b.int(1),
                b.loc("t"),
            ),
        )),
        ("withPermitsIfAvailable", 3) => Some(sem_with_permits_if_avail_template(b)),
        _ => None,
    }
}
