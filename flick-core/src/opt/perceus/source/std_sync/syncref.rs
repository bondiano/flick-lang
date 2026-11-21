//! Audited `SynchronizedRef` and `SubscriptionRef` pure IR templates.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, ExprKind, Pat, Stmt};
use super::{rget, rmake, rset, task_of};

fn syncref_with_lock(b: &ExprBuilder, body: Expr, with_hub: bool) -> Expr {
    let pat = if with_hub {
        b.tup_pat(&["cell", "sem", "hub"])
    } else {
        b.tup_pat(&["cell", "sem"])
    };
    b.match_(
        b.loc("r"),
        vec![arm(
            pat,
            b.app2(
                b.global("Semaphore", "with_permit"),
                b.loc("sem"),
                task_of(b, body),
            ),
        )],
    )
}

fn syncref_set_and_pub(b: &ExprBuilder, val: Expr, with_hub: bool) -> Vec<Stmt> {
    if with_hub {
        vec![
            b.let_("nv_pub", val),
            rset(b, b.loc("cell"), b.loc("nv_pub")),
            b.let_(
                "_pb",
                b.app2(b.global("PubSub", "publish"), b.loc("nv_pub"), b.loc("hub")),
            ),
        ]
    } else {
        vec![rset(b, b.loc("cell"), val)]
    }
}

fn syncref_update(b: &ExprBuilder, keep: Expr, with_hub: bool) -> Expr {
    let mut stmts = vec![
        b.let_("old", rget(b, b.loc("cell"))),
        b.let_("nxt", b.app(b.loc("f"), b.loc("old"))),
    ];
    stmts.extend(syncref_set_and_pub(b, b.loc("nxt"), with_hub));
    b.lam(
        &["f", "r"],
        syncref_with_lock(b, b.block(stmts, keep), with_hub),
    )
}

fn syncref_update_some(b: &ExprBuilder, on_some: Expr, on_none: Expr, with_hub: bool) -> Expr {
    b.lam(
        &["f", "r"],
        syncref_with_lock(
            b,
            b.block(
                vec![b.let_("old", rget(b, b.loc("cell")))],
                b.match_(
                    b.app(b.loc("f"), b.loc("old")),
                    vec![
                        arm(
                            b.some_pat("nxt"),
                            b.block(syncref_set_and_pub(b, b.loc("nxt"), with_hub), on_some),
                        ),
                        arm(Pat::Wild, on_none),
                    ],
                ),
            ),
            with_hub,
        ),
    )
}

fn syncref_make(b: &ExprBuilder, with_hub: bool) -> Expr {
    if with_hub {
        b.lam(
            &["v"],
            b.block(
                vec![
                    b.let_(
                        "hub",
                        b.app(
                            b.global("PubSub", "unbounded"),
                            b.at(ExprKind::Record(vec![("replay".into(), b.some(b.int(1)))])),
                        ),
                    ),
                    b.let_(
                        "_p0",
                        b.app2(b.global("PubSub", "publish"), b.loc("v"), b.loc("hub")),
                    ),
                ],
                b.tuple(vec![
                    rmake(b, b.loc("v")),
                    b.app(b.global("Semaphore", "make"), b.int(1)),
                    b.loc("hub"),
                ]),
            ),
        )
    } else {
        b.lam(
            &["v"],
            b.tuple(vec![
                rmake(b, b.loc("v")),
                b.app(b.global("Semaphore", "make"), b.int(1)),
            ]),
        )
    }
}

fn syncref_modify_some(b: &ExprBuilder, with_hub: bool) -> Expr {
    b.lam(
        &["f", "r"],
        syncref_with_lock(
            b,
            b.match_(
                b.app(b.loc("f"), rget(b, b.loc("cell"))),
                vec![arm(
                    Pat::Tuple(vec![Pat::Bind(b.name("out")), Pat::Bind(b.name("opt"))]),
                    b.match_(
                        b.loc("opt"),
                        vec![
                            arm(
                                b.some_pat("nxt"),
                                b.block(
                                    syncref_set_and_pub(b, b.loc("nxt"), with_hub),
                                    b.loc("out"),
                                ),
                            ),
                            arm(Pat::Wild, b.loc("out")),
                        ],
                    ),
                )],
            ),
            with_hub,
        ),
    )
}

pub(super) fn syncref_template(
    b: &ExprBuilder,
    bind: &str,
    stages: usize,
    with_hub: bool,
) -> Option<Expr> {
    match (bind, stages) {
        ("make", 1) => Some(syncref_make(b, with_hub)),
        ("get", 1) => Some(b.lam(
            &["r"],
            syncref_with_lock(b, rget(b, b.loc("cell")), with_hub),
        )),
        ("set", 2) => Some(b.lam(
            &["v", "r"],
            syncref_with_lock(
                b,
                b.block(syncref_set_and_pub(b, b.loc("v"), with_hub), b.unit()),
                with_hub,
            ),
        )),
        ("getAndSet", 2) => {
            let mut stmts = vec![b.let_("old", rget(b, b.loc("cell")))];
            stmts.extend(syncref_set_and_pub(b, b.loc("v"), with_hub));
            Some(b.lam(
                &["v", "r"],
                syncref_with_lock(b, b.block(stmts, b.loc("old")), with_hub),
            ))
        }
        ("setAndGet", 2) => Some(b.lam(
            &["v", "r"],
            syncref_with_lock(
                b,
                b.block(syncref_set_and_pub(b, b.loc("v"), with_hub), b.loc("v")),
                with_hub,
            ),
        )),
        ("getAndUpdate" | "getAndUpdateEffect", 2) => {
            Some(syncref_update(b, b.loc("old"), with_hub))
        }
        ("update" | "updateEffect", 2) => Some(syncref_update(b, b.unit(), with_hub)),
        ("updateAndGet" | "updateAndGetEffect", 2) => {
            Some(syncref_update(b, b.loc("nxt"), with_hub))
        }
        ("getAndUpdateSome" | "getAndUpdateSomeEffect", 2) => {
            Some(syncref_update_some(b, b.loc("old"), b.loc("old"), with_hub))
        }
        ("updateSome" | "updateSomeEffect", 2) => {
            Some(syncref_update_some(b, b.unit(), b.unit(), with_hub))
        }
        ("updateSomeAndGet" | "updateSomeAndGetEffect", 2) => {
            Some(syncref_update_some(b, b.loc("nxt"), b.loc("old"), with_hub))
        }
        ("modify" | "modifyEffect", 2) => Some(b.lam(
            &["f", "r"],
            syncref_with_lock(
                b,
                b.match_(
                    b.app(b.loc("f"), rget(b, b.loc("cell"))),
                    vec![arm(
                        Pat::Tuple(vec![Pat::Bind(b.name("out")), Pat::Bind(b.name("nxt"))]),
                        b.block(syncref_set_and_pub(b, b.loc("nxt"), with_hub), b.loc("out")),
                    )],
                ),
                with_hub,
            ),
        )),
        ("modifySome" | "modifySomeEffect", 2) => Some(syncref_modify_some(b, with_hub)),
        _ => None,
    }
}
