//! Audited `RequestResolver` constructors and combinators (Stage 4D).

mod cache;
mod span;

use flick_ir::{BinOp, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::std_request::res_pat;
use super::{Expr, Pat, Stmt};

fn make_resolver(
    b: &ExprBuilder,
    delay: Expr,
    bkey: Expr,
    eq_keys: bool,
    pre: Expr,
    coll: Expr,
    run_all: Expr,
) -> Expr {
    b.tuple([
        delay,
        bkey,
        b.bool_(eq_keys),
        pre,
        coll,
        run_all,
        b.rmake(b.tuple([b.list(vec![]), b.bool_(false)])),
        b.rmake(b.list(vec![])),
    ])
}

fn default_delay(b: &ExprBuilder) -> Expr {
    b.lam(
        &["_u"],
        b.app(b.global("Effect.Core", "yield_now"), b.unit()),
    )
}

fn default_key(b: &ExprBuilder) -> Expr {
    b.lam(&["_e"], b.int(0))
}

fn collect_all(b: &ExprBuilder) -> Expr {
    b.lam(&["_es"], b.bool_(true))
}

fn clone_resolver(
    b: &ExprBuilder,
    delay: Option<Expr>,
    bkey: Option<Expr>,
    eq_keys: Option<Expr>,
    pre: Option<Expr>,
    coll: Option<Expr>,
    run_all: Option<Expr>,
) -> Expr {
    b.tuple([
        delay.unwrap_or_else(|| b.loc("r_del")),
        bkey.unwrap_or_else(|| b.loc("r_key")),
        eq_keys.unwrap_or_else(|| b.loc("r_eq")),
        pre.unwrap_or_else(|| b.loc("r_pre")),
        coll.unwrap_or_else(|| b.loc("r_col")),
        run_all.unwrap_or_else(|| b.loc("r_run")),
        b.rmake(b.tuple([b.list(vec![]), b.bool_(false)])),
        b.rmake(b.list(vec![])),
    ])
}

fn yielding_delay(b: &ExprBuilder, delay_task: Expr) -> Expr {
    b.lam(
        &["_u"],
        b.block(
            [Stmt::Expr(
                b.app(b.global("Effect.Core", "yield_now"), b.unit()),
            )],
            b.await_(delay_task),
        ),
    )
}

fn from_effect_template(b: &ExprBuilder) -> Expr {
    let entry_task = b.lam(
        &["e"],
        b.task(b.block(
            [b.let_(
                "ex",
                b.app(
                    b.global("Effect.Core", "exit"),
                    b.task(b.app(b.loc("f"), b.loc("e"))),
                ),
            )],
            b.app2(b.global("Request", "complete"), b.loc("ex"), b.loc("e")),
        )),
    );
    let tasks = b.app2(b.global("List", "map"), entry_task, b.loc("es"));
    let run_all = b.lam(
        &["es", "_k"],
        b.block(
            [
                b.let_("tasks", tasks),
                b.let_(
                    "_xs",
                    b.app(b.global("Prelude", "parallel_exits"), b.loc("tasks")),
                ),
            ],
            b.unit(),
        ),
    );
    b.lam(
        &["f"],
        make_resolver(
            b,
            default_delay(b),
            default_key(b),
            false,
            b.none(),
            collect_all(b),
            run_all,
        ),
    )
}

fn around_template(b: &ExprBuilder) -> Expr {
    let check_aft = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("aex")),
        [
            arm(
                b.some_pat("ac"),
                b.app(b.global("Failure", "rethrow"), b.loc("ac")),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let check_inner = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("iex")),
        [
            arm(
                b.some_pat("ic"),
                b.app(b.global("Failure", "rethrow"), b.loc("ic")),
            ),
            arm(Pat::Wild, check_aft),
        ],
    );
    let run_inner = b.app(
        b.loc("restore"),
        b.task(b.app2(b.loc("r_run"), b.loc("es"), b.loc("k"))),
    );
    let run_aft = b.task(b.app2(b.loc("aft"), b.loc("es"), b.loc("a")));
    let masked = b.lam(
        &["restore"],
        b.block(
            [
                b.let_("a", b.app(b.loc("bef"), b.loc("es"))),
                b.let_("iex", b.app(b.global("Effect.Core", "exit"), run_inner)),
                b.let_("aex", b.app(b.global("Effect.Core", "exit"), run_aft)),
            ],
            check_inner,
        ),
    );
    let wrapped = b.lam(
        &["es", "k"],
        b.app(
            b.global("Effect.Concurrency", "uninterruptible_mask"),
            masked,
        ),
    );
    b.lam(
        &["bef", "aft", "self_r"],
        b.match_(
            b.loc("self_r"),
            [arm(
                res_pat(b),
                clone_resolver(b, None, None, None, None, None, Some(wrapped)),
            )],
        ),
    )
}

fn from_function_batched_template(b: &ExprBuilder) -> Expr {
    let succeed_one = b.block(
        [
            b.assign(
                "rem",
                b.app2(b.global("List.Core", "drop"), b.int(1), b.var("rem")),
            ),
            Stmt::Expr(b.app2(b.global("Request", "succeed"), b.loc("v"), b.loc("e0"))),
        ],
        b.unit(),
    );
    let head = b.app2(b.global("List.Core", "take"), b.int(1), b.var("rem"));
    let step = b.match_(
        head,
        [
            arm(Pat::List(vec![Pat::Bind(b.name("e0"))], None), succeed_one),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let loop_body = b.for_ctrl("rfb", "v", b.app(b.loc("f"), b.loc("es")), step);
    let run_all = b.lam(
        &["es", "_k"],
        b.block([b.var_decl("rem", b.loc("es"))], loop_body),
    );
    b.lam(
        &["f"],
        make_resolver(
            b,
            default_delay(b),
            default_key(b),
            false,
            b.none(),
            collect_all(b),
            run_all,
        ),
    )
}

fn batch_n_template(b: &ExprBuilder) -> Expr {
    let sz_val = b.if_(b.bin(BinOp::Lt, b.loc("n"), b.int(1)), b.int(1), b.loc("n"));
    let coll = b.lam(
        &["es"],
        b.bin(
            BinOp::Lt,
            b.app(b.global("List.Core", "length"), b.loc("es")),
            b.loc("sz"),
        ),
    );
    let body = b.block(
        [b.let_("sz", sz_val)],
        clone_resolver(b, None, None, None, None, Some(coll), None),
    );
    b.lam(
        &["n", "self_r"],
        b.match_(b.loc("self_r"), [arm(res_pat(b), body)]),
    )
}

fn race_template(b: &ExprBuilder) -> Expr {
    let run_all = b.lam(
        &["es", "k"],
        b.app2(
            b.global("Prelude", "race_success"),
            b.task(b.app2(b.loc("ra"), b.loc("es"), b.loc("k"))),
            b.task(b.app2(b.loc("rb"), b.loc("es"), b.loc("k"))),
        ),
    );
    let raced = make_resolver(
        b,
        default_delay(b),
        default_key(b),
        false,
        b.none(),
        collect_all(b),
        run_all,
    );
    let match_that = b.match_(
        b.loc("that"),
        [arm(
            b.tup_pat(&["_a", "_b", "_c", "_d", "_e", "rb", "_g", "_h"]),
            raced,
        )],
    );
    b.lam(
        &["that", "self_r"],
        b.match_(
            b.loc("self_r"),
            [arm(
                b.tup_pat(&["_1", "_2", "_3", "_4", "_5", "ra", "_7", "_8"]),
                match_that,
            )],
        ),
    )
}

fn resolver_ctor_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickMake", 1) => Some(b.lam(
            &["f"],
            make_resolver(
                b,
                default_delay(b),
                default_key(b),
                false,
                b.none(),
                collect_all(b),
                b.lam(&["es", "_k"], b.app(b.loc("f"), b.loc("es"))),
            ),
        )),
        ("flickMakeWith", 1) => Some(b.lam(
            &["o"],
            make_resolver(
                b,
                yielding_delay(b, b.field(b.loc("o"), "delay")),
                b.field(b.loc("o"), "batchKey"),
                false,
                b.field(b.loc("o"), "preCheck"),
                b.field(b.loc("o"), "collectWhile"),
                b.field(b.loc("o"), "runAll"),
            ),
        )),
        ("flickMakeGrouped", 1) => Some(b.lam(
            &["o"],
            make_resolver(
                b,
                default_delay(b),
                b.field(b.loc("o"), "key"),
                true,
                b.none(),
                collect_all(b),
                b.field(b.loc("o"), "resolver"),
            ),
        )),
        ("fromFunction", 1) => {
            let step = b.app2(
                b.global("Request", "succeed"),
                b.app(b.loc("f"), b.loc("e")),
                b.loc("e"),
            );
            let run_all = b.lam(&["es", "_k"], b.for_ctrl("rff", "e", b.loc("es"), step));
            Some(b.lam(
                &["f"],
                make_resolver(
                    b,
                    default_delay(b),
                    default_key(b),
                    false,
                    b.none(),
                    collect_all(b),
                    run_all,
                ),
            ))
        }
        ("fromFunctionBatched", 1) => Some(from_function_batched_template(b)),
        ("fromEffect", 1) => Some(from_effect_template(b)),
        ("never", 0) => Some(make_resolver(
            b,
            default_delay(b),
            default_key(b),
            false,
            b.none(),
            collect_all(b),
            b.lam(
                &["_es", "_k"],
                b.app(b.global("Effect.Core", "never"), b.unit()),
            ),
        )),
        _ => None,
    }
}

pub(super) fn resolver_template(
    b: &ExprBuilder,
    bind: &str,
    mode: ForeignMode,
    stages: usize,
) -> Option<Expr> {
    if mode == ForeignMode::Pure
        && let Some(expr) = resolver_ctor_template(b, bind, stages)
    {
        return Some(expr);
    }
    match (mode, bind, stages) {
        (ForeignMode::Pure, "flickSetDelayEffect", 2) => {
            let cloned = clone_resolver(
                b,
                Some(yielding_delay(b, b.loc("dt"))),
                None,
                None,
                None,
                None,
                None,
            );
            Some(b.lam(
                &["dt", "self_r"],
                b.match_(b.loc("self_r"), [arm(res_pat(b), cloned)]),
            ))
        }
        (ForeignMode::Pure, "setDelay", 2) => {
            let sleep = b.lam(
                &["_u"],
                b.app(b.global("Effect.Core", "sleep"), b.loc("dur")),
            );
            let cloned = clone_resolver(b, Some(sleep), None, None, None, None, None);
            Some(b.lam(
                &["dur", "self_r"],
                b.match_(b.loc("self_r"), [arm(res_pat(b), cloned)]),
            ))
        }
        (ForeignMode::Pure, "around", 3) => Some(around_template(b)),
        (ForeignMode::Pure, "batchN", 2) => Some(batch_n_template(b)),
        (ForeignMode::Pure, "grouped", 2) => {
            let cloned = clone_resolver(
                b,
                None,
                Some(b.loc("kf")),
                Some(b.bool_(true)),
                None,
                None,
                None,
            );
            Some(b.lam(
                &["kf", "self_r"],
                b.match_(b.loc("self_r"), [arm(res_pat(b), cloned)]),
            ))
        }
        (ForeignMode::Pure, "race", 2) => Some(race_template(b)),
        (ForeignMode::Pure, "flickWithSpan", 3) => Some(span::with_span_template(b)),
        (ForeignMode::Effect, "flickWithCache", 2) => Some(cache::with_cache_template(b)),
        (ForeignMode::Effect, "flickAsCache", 2) => Some(b.lam(
            &["opts", "self_r"],
            b.app2(
                b.global("Cache", "make_with"),
                b.lam(
                    &["rq"],
                    b.app2(b.global("Request", "run"), b.loc("self_r"), b.loc("rq")),
                ),
                b.loc("opts"),
            ),
        )),
        _ => None,
    }
}
