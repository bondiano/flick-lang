//! Audited `Schedule` constructors (`packages/std/src/Schedule.fl`).
//!
//! Each `Schedule r e i o` lowers to `Unit -> (Float -> i -> (o, Duration, Bool))`:
//! calling it allocates a run's fresh `Ref` state and returns a stateful step
//! function `(now_ms, input) -> (output, delay, done)`.

use flick_ir::{Arith, BinOp, Foreign, ForeignMode, Num};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::std_schedule_comb::{
    add_delay_template, combined_template, concat_template, cron_template, jittered_template,
    map_template, modify_delay_template, passthrough_template, tap_template, up_to_template,
    while_template,
};
use super::std_schedule_run::{
    effect_schedule_template, from_millis, meta_ref, millis_of, zero_millis,
};
use super::{Code, Env, Expr, Local, Lower, LowerError, Pat, Span, Stmt};

impl Lower<'_> {
    pub(super) fn std_schedule_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        let (from, bind) = value.binding.module_export()?;
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$sch");

        if from == "flick:Effect" && value.mode == ForeignMode::Effect {
            let expr = effect_schedule_template(&b, bind, stages)?;
            return Some(self.expr(&expr, &Env::new(), block));
        }
        if from != "flick:Schedule" {
            return None;
        }
        if value.mode == ForeignMode::Effect && bind == "flickCurrentMetadata" && stages == 1 {
            let expr = b.lam(&["_u"], b.app(b.global("Context", "read"), meta_ref(&b)));
            return Some(self.expr(&expr, &Env::new(), block));
        }
        if value.mode != ForeignMode::Pure {
            return None;
        }
        let expr = schedule_template(&b, bind, stages)?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

fn schedule_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("forever", 0) => Some(b.app(b.global("Schedule", "spaced"), b.global("Duration", "zero"))),
        ("once", 0) => Some(b.app2(
            b.global("Schedule", "map"),
            b.lam(&["_m"], b.unit()),
            b.app(b.global("Schedule", "recurs"), b.int(1)),
        )),
        ("identity", 1) => Some(b.lam(
            &["_u"],
            b.lam(
                &["_s"],
                b.lam(
                    &["_now", "inp"],
                    b.tuple(vec![
                        b.loc("inp"),
                        b.global("Duration", "zero"),
                        b.bool_(false),
                    ]),
                ),
            ),
        )),
        ("recurs", 1) => Some(recurs_template(b)),
        ("spaced", 1) => Some(spaced_template(b)),
        ("fixed", 1) => Some(fixed_template(b)),
        ("windowed", 1) => Some(windowed_template(b)),
        ("duration", 1) => Some(duration_template(b)),
        ("during", 1) => Some(during_template(b)),
        ("exponential", 2) => Some(exponential_template(b)),
        ("fibonacci", 1) => Some(fibonacci_template(b)),
        ("jittered", 1) => Some(jittered_template(b)),
        ("passthrough", 1) => Some(passthrough_template(b)),
        ("concat", 2) => Some(concat_template(b, false)),
        ("concatResult", 2) => Some(concat_template(b, true)),
        ("max", 1) => Some(combined_template(b, true)),
        ("min", 1) => Some(combined_template(b, false)),
        ("flickUpTo", 2) => Some(up_to_template(b)),
        ("map", 2) => Some(map_template(b)),
        ("tap", 2) => Some(tap_template(b)),
        ("while", 2) => Some(while_template(b)),
        ("addDelay", 2) => Some(add_delay_template(b)),
        ("modifyDelay", 2) => Some(modify_delay_template(b)),
        ("cron", 1) => Some(cron_template(b)),
        _ => None,
    }
}

pub(super) fn ref_get(b: &ExprBuilder, r: Expr) -> Expr {
    b.app(b.global("Ref", "get"), r)
}

pub(super) fn ref_set(b: &ExprBuilder, r: Expr, v: Expr) -> Stmt {
    Stmt::Expr(b.app2(b.global("Ref", "set"), r, v))
}

pub(super) fn ref_make(b: &ExprBuilder, v: Expr) -> Expr {
    b.app(b.global("Ref", "make"), v)
}

pub(super) fn fsub(b: &ExprBuilder, l: Expr, r: Expr) -> Expr {
    b.bin(BinOp::Arith(Arith::Sub, Num::Float), l, r)
}

pub(super) fn fadd(b: &ExprBuilder, l: Expr, r: Expr) -> Expr {
    b.bin(BinOp::Arith(Arith::Add, Num::Float), l, r)
}

pub(super) fn fmul(b: &ExprBuilder, l: Expr, r: Expr) -> Expr {
    b.bin(BinOp::Arith(Arith::Mul, Num::Float), l, r)
}

fn frem(b: &ExprBuilder, l: Expr, r: Expr) -> Expr {
    b.bin(BinOp::Arith(Arith::Rem, Num::Float), l, r)
}

fn recurs_template(b: &ExprBuilder) -> Expr {
    let step = b.lam(
        &["_now", "_inp"],
        b.block(
            vec![
                b.let_("n", ref_get(b, b.loc("c"))),
                b.let_("next", b.add_i(b.loc("n"), b.int(1))),
                ref_set(b, b.loc("c"), b.loc("next")),
            ],
            b.tuple(vec![
                b.loc("n"),
                b.global("Duration", "zero"),
                b.bin(BinOp::Gt, b.loc("next"), b.loc("times")),
            ]),
        ),
    );
    b.lam(
        &["times"],
        b.lam(
            &["_s"],
            b.block(vec![b.let_("c", ref_make(b, b.int(0)))], step),
        ),
    )
}

fn spaced_template(b: &ExprBuilder) -> Expr {
    let step = b.lam(
        &["_now", "_inp"],
        b.block(
            vec![
                b.let_("n", ref_get(b, b.loc("c"))),
                ref_set(b, b.loc("c"), b.add_i(b.loc("n"), b.int(1))),
            ],
            b.tuple(vec![b.loc("n"), b.loc("d"), b.bool_(false)]),
        ),
    );
    b.lam(
        &["d"],
        b.lam(
            &["_s"],
            b.block(vec![b.let_("c", ref_make(b, b.int(0)))], step),
        ),
    )
}

fn duration_template(b: &ExprBuilder) -> Expr {
    let step = b.lam(
        &["_now", "_inp"],
        b.block(
            vec![
                b.let_("n", ref_get(b, b.loc("c"))),
                ref_set(b, b.loc("c"), b.add_i(b.loc("n"), b.int(1))),
            ],
            b.if_(
                b.bin(BinOp::Eq, b.loc("n"), b.int(0)),
                b.tuple(vec![b.loc("d"), b.loc("d"), b.bool_(false)]),
                b.tuple(vec![
                    b.global("Duration", "zero"),
                    b.global("Duration", "zero"),
                    b.bool_(true),
                ]),
            ),
        ),
    );
    b.lam(
        &["d"],
        b.lam(
            &["_s"],
            b.block(vec![b.let_("c", ref_make(b, b.int(0)))], step),
        ),
    )
}

fn during_template(b: &ExprBuilder) -> Expr {
    let body = b.block(
        vec![
            b.let_(
                "start",
                b.if_(
                    b.bin(BinOp::Eq, b.loc("n"), b.int(0)),
                    b.loc("now"),
                    b.loc("s0"),
                ),
            ),
            ref_set(
                b,
                b.loc("c"),
                b.tuple(vec![b.add_i(b.loc("n"), b.int(1)), b.loc("start")]),
            ),
            b.let_("el", fsub(b, b.loc("now"), b.loc("start"))),
        ],
        b.tuple(vec![
            from_millis(b, b.loc("el")),
            b.global("Duration", "zero"),
            b.bin(BinOp::Gt, b.loc("el"), b.loc("limit")),
        ]),
    );
    let step = b.lam(
        &["now", "_inp"],
        b.match_(
            ref_get(b, b.loc("c")),
            vec![arm(
                Pat::Tuple(vec![Pat::Bind(b.name("n")), Pat::Bind(b.name("s0"))]),
                body,
            )],
        ),
    );
    b.lam(
        &["d"],
        b.block(
            vec![b.let_("limit", millis_of(b, b.loc("d")))],
            b.lam(
                &["_s"],
                b.block(
                    vec![b.let_("c", ref_make(b, b.tuple(vec![b.int(0), b.float(0.0)])))],
                    step,
                ),
            ),
        ),
    )
}

fn exponential_template(b: &ExprBuilder) -> Expr {
    let step = b.lam(
        &["_now", "_inp"],
        b.block(
            vec![
                b.let_("p", ref_get(b, b.loc("c"))),
                ref_set(b, b.loc("c"), fmul(b, b.loc("p"), b.loc("factor"))),
                b.let_("d", from_millis(b, fmul(b, b.loc("bm"), b.loc("p")))),
            ],
            b.tuple(vec![b.loc("d"), b.loc("d"), b.bool_(false)]),
        ),
    );
    b.lam(
        &["base", "factor"],
        b.block(
            vec![b.let_("bm", millis_of(b, b.loc("base")))],
            b.lam(
                &["_s"],
                b.block(vec![b.let_("c", ref_make(b, b.float(1.0)))], step),
            ),
        ),
    )
}

fn fibonacci_template(b: &ExprBuilder) -> Expr {
    let body = b.block(
        vec![
            ref_set(
                b,
                b.loc("c"),
                b.tuple(vec![b.loc("fb"), fadd(b, b.loc("fa"), b.loc("fb"))]),
            ),
            b.let_("d", from_millis(b, b.loc("fb"))),
        ],
        b.tuple(vec![b.loc("d"), b.loc("d"), b.bool_(false)]),
    );
    let step = b.lam(
        &["_now", "_inp"],
        b.match_(
            ref_get(b, b.loc("c")),
            vec![arm(
                Pat::Tuple(vec![Pat::Bind(b.name("fa")), Pat::Bind(b.name("fb"))]),
                body,
            )],
        ),
    );
    b.lam(
        &["one"],
        b.block(
            vec![b.let_("om", millis_of(b, b.loc("one")))],
            b.lam(
                &["_s"],
                b.block(
                    vec![b.let_("c", ref_make(b, b.tuple(vec![b.float(0.0), b.loc("om")])))],
                    step,
                ),
            ),
        ),
    )
}

fn fixed_subsequent_step(b: &ExprBuilder) -> Expr {
    b.block(
        vec![
            b.let_(
                "behind",
                b.bin(
                    BinOp::Gt,
                    b.loc("now"),
                    fadd(b, b.loc("last"), b.loc("win")),
                ),
            ),
            b.let_(
                "bdry",
                fsub(
                    b,
                    b.loc("win"),
                    frem(b, fsub(b, b.loc("now"), b.loc("st")), b.loc("win")),
                ),
            ),
            b.let_(
                "dms",
                b.if_(
                    b.loc("behind"),
                    b.float(0.0),
                    b.if_(zero_millis(b, b.loc("bdry")), b.loc("win"), b.loc("bdry")),
                ),
            ),
            b.let_(
                "nlast",
                b.if_(
                    b.loc("behind"),
                    b.loc("now"),
                    fadd(b, b.loc("now"), b.loc("dms")),
                ),
            ),
            ref_set(
                b,
                b.loc("c"),
                b.tuple(vec![
                    b.add_i(b.loc("n"), b.int(1)),
                    b.loc("st"),
                    b.loc("nlast"),
                ]),
            ),
        ],
        b.tuple(vec![
            b.loc("n"),
            from_millis(b, b.loc("dms")),
            b.bool_(false),
        ]),
    )
}

fn fixed_template(b: &ExprBuilder) -> Expr {
    let first_step = b.block(
        vec![ref_set(
            b,
            b.loc("c"),
            b.tuple(vec![
                b.int(1),
                b.loc("now"),
                fadd(b, b.loc("now"), b.loc("win")),
            ]),
        )],
        b.tuple(vec![b.int(0), from_millis(b, b.loc("win")), b.bool_(false)]),
    );
    let zero_win = b.block(
        vec![ref_set(
            b,
            b.loc("c"),
            b.tuple(vec![
                b.add_i(b.loc("n"), b.int(1)),
                b.loc("st"),
                b.loc("last"),
            ]),
        )],
        b.tuple(vec![
            b.loc("n"),
            b.global("Duration", "zero"),
            b.bool_(false),
        ]),
    );
    let body = b.if_(
        b.bin(BinOp::Eq, b.loc("win"), b.float(0.0)),
        zero_win,
        b.if_(
            b.bin(BinOp::Eq, b.loc("n"), b.int(0)),
            first_step,
            fixed_subsequent_step(b),
        ),
    );
    let step = b.lam(
        &["now", "_inp"],
        b.match_(
            ref_get(b, b.loc("c")),
            vec![arm(
                Pat::Tuple(vec![
                    Pat::Bind(b.name("n")),
                    Pat::Bind(b.name("st")),
                    Pat::Bind(b.name("last")),
                ]),
                body,
            )],
        ),
    );
    b.lam(
        &["iv"],
        b.block(
            vec![b.let_("win", millis_of(b, b.loc("iv")))],
            b.lam(
                &["_s"],
                b.block(
                    vec![b.let_(
                        "c",
                        ref_make(b, b.tuple(vec![b.int(0), b.float(0.0), b.float(0.0)])),
                    )],
                    step,
                ),
            ),
        ),
    )
}

fn windowed_template(b: &ExprBuilder) -> Expr {
    let nonzero = b.block(
        vec![
            b.let_("el", fsub(b, b.loc("now"), b.loc("st"))),
            b.let_(
                "bdry",
                fsub(b, b.loc("win"), frem(b, b.loc("el"), b.loc("win"))),
            ),
            b.let_(
                "dms",
                b.if_(zero_millis(b, b.loc("bdry")), b.loc("win"), b.loc("bdry")),
            ),
        ],
        b.tuple(vec![
            b.loc("n"),
            from_millis(b, b.loc("dms")),
            b.bool_(false),
        ]),
    );
    let body = b.block(
        vec![
            b.let_(
                "st",
                b.if_(
                    b.bin(BinOp::Eq, b.loc("n"), b.int(0)),
                    b.loc("now"),
                    b.loc("s0"),
                ),
            ),
            ref_set(
                b,
                b.loc("c"),
                b.tuple(vec![b.add_i(b.loc("n"), b.int(1)), b.loc("st")]),
            ),
        ],
        b.if_(
            b.bin(BinOp::Eq, b.loc("win"), b.float(0.0)),
            b.tuple(vec![
                b.loc("n"),
                b.global("Duration", "zero"),
                b.bool_(false),
            ]),
            nonzero,
        ),
    );
    let step = b.lam(
        &["now", "_inp"],
        b.match_(
            ref_get(b, b.loc("c")),
            vec![arm(
                Pat::Tuple(vec![Pat::Bind(b.name("n")), Pat::Bind(b.name("s0"))]),
                body,
            )],
        ),
    );
    b.lam(
        &["iv"],
        b.block(
            vec![b.let_("win", millis_of(b, b.loc("iv")))],
            b.lam(
                &["_s"],
                b.block(
                    vec![b.let_("c", ref_make(b, b.tuple(vec![b.int(0), b.float(0.0)])))],
                    step,
                ),
            ),
        ),
    )
}
