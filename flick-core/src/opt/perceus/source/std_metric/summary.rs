//! Summary metric quantile calculation and observation ring buffer hooks.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::{Expr, ExprKind, Pat};
use super::registry::sort_floats_expr;
use super::{fadd, fmul, fsub};

fn check_quantiles_expr(b: &ExprBuilder) -> Expr {
    let out_of_range = b.bin(
        BinOp::Or,
        b.bin(BinOp::Lt, b.loc("q"), b.float(0.0)),
        b.bin(BinOp::Gt, b.loc("q"), b.float(1.0)),
    );
    let err_msg = b.bin(
        BinOp::Concat,
        b.str_("Error: Quantile must be between 0 and 1, found: "),
        b.app(b.global("Float", "to_string"), b.loc("q")),
    );
    let step = b.if_(
        out_of_range,
        b.app(b.global("Effect.Core", "die"), err_msg),
        b.unit(),
    );
    b.for_ctrl("sqc", "q", b.loc("qs"), step)
}

fn get_at(b: &ExprBuilder, xs: Expr, idx: Expr) -> Expr {
    let head = b.app2(
        b.global("List.Core", "take"),
        b.int(1),
        b.app2(b.global("List.Core", "drop"), idx, xs),
    );
    b.match_(
        head,
        [
            arm(
                Pat::List(vec![Pat::Bind(b.name("qv"))], None),
                b.some(b.loc("qv")),
            ),
            arm(Pat::Wild, b.none()),
        ],
    )
}

fn collect_samples_expr(b: &ExprBuilder) -> Expr {
    let in_window = b.bin(
        BinOp::And,
        b.bin(BinOp::Ge, b.loc("age"), b.float(0.0)),
        b.bin(BinOp::Le, b.loc("age"), b.loc("mage")),
    );
    let push_sample = b.block(
        [b.assign(
            "samps",
            b.app2(
                b.global("List.Core", "append"),
                b.var("samps"),
                b.list([b.loc("ov")]),
            ),
        )],
        b.unit(),
    );
    let on_obs = b.block(
        [b.let_("age", fsub(b, b.loc("now"), b.loc("ots")))],
        b.if_(in_window, push_sample, b.unit()),
    );
    let step = b.match_(b.loc("ob"), [arm(b.tup_pat(&["ov", "ots"]), on_obs)]);
    b.block(
        [
            b.let_(
                "now",
                b.app(b.global("Clock", "current_time_millis"), b.unit()),
            ),
            b.var_decl("samps", b.list(vec![])),
            b.let_("_sl", b.for_ctrl("ssmp", "ob", b.loc("obs"), step)),
        ],
        sort_floats_expr(b, b.var("samps")),
    )
}

fn calc_quants_expr(b: &ExprBuilder) -> Expr {
    let rank_f = fmul(
        b,
        b.loc("q"),
        b.app(b.global("Int", "to_float"), b.loc("sn")),
    );
    let mid_idx = b.sub_i(
        b.app(
            b.global("Int", "truncate"),
            b.app(b.global("Float", "ceil"), rank_f),
        ),
        b.int(1),
    );
    let at_hi = get_at(b, b.loc("ss"), b.sub_i(b.loc("sn"), b.int(1)));
    let at_mid = get_at(b, b.loc("ss"), mid_idx);
    let ge_one = b.if_(b.bin(BinOp::Ge, b.loc("q"), b.float(1.0)), at_hi, at_mid);
    let le_zero = b.if_(
        b.bin(BinOp::Le, b.loc("q"), b.float(0.0)),
        get_at(b, b.loc("ss"), b.int(0)),
        ge_one,
    );
    let qval = b.if_(b.bin(BinOp::Eq, b.loc("sn"), b.int(0)), b.none(), le_zero);
    let step = b.block(
        [
            b.let_("qval", qval),
            b.assign(
                "qout",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("qout"),
                    b.list([b.tuple([b.loc("q"), b.loc("qval")])]),
                ),
            ),
        ],
        b.unit(),
    );
    b.block(
        [
            b.let_("ss", collect_samples_expr(b)),
            b.let_("sn", b.app(b.global("List.Core", "length"), b.loc("ss"))),
            b.var_decl("qout", b.list(vec![])),
            b.let_("_ql", b.for_ctrl("sqo", "q", b.loc("qs"), step)),
        ],
        b.var("qout"),
    )
}

fn summary_upd_expr(b: &ExprBuilder) -> Expr {
    let appended = b.app2(
        b.global("List.Core", "append"),
        b.loc("obs"),
        b.list([b.tuple([b.loc("v"), b.loc("ts")])]),
    );
    let trimmed = b.app2(
        b.global("List.Core", "drop"),
        b.sub_i(b.loc("alen"), b.loc("msz")),
        b.loc("app"),
    );
    let add_obs = b.block(
        [
            b.let_("app", appended),
            b.let_("alen", b.app(b.global("List.Core", "length"), b.loc("app"))),
        ],
        b.if_(
            b.bin(BinOp::Gt, b.loc("alen"), b.loc("msz")),
            trimmed,
            b.loc("app"),
        ),
    );
    let nobs = b.if_(
        b.bin(BinOp::Gt, b.loc("msz"), b.int(0)),
        add_obs,
        b.loc("obs"),
    );
    let next_state = b.tuple([
        b.loc("nobs"),
        fadd(b, b.loc("cnt"), b.float(1.0)),
        fadd(b, b.loc("sum"), b.loc("v")),
        b.if_(
            b.bin(BinOp::Lt, b.loc("v"), b.loc("lo")),
            b.loc("v"),
            b.loc("lo"),
        ),
        b.if_(
            b.bin(BinOp::Gt, b.loc("v"), b.loc("hi")),
            b.loc("v"),
            b.loc("hi"),
        ),
    ]);
    let on_state = b.block(
        [b.let_("nobs", nobs), b.rset(b.loc("sr"), next_state)],
        b.unit(),
    );
    let on_pair = b.match_(
        b.rget(b.loc("sr")),
        [arm(b.tup_pat(&["obs", "cnt", "sum", "lo", "hi"]), on_state)],
    );
    b.lam(
        &["pair"],
        b.no_preempt(b.match_(b.loc("pair"), [arm(b.tup_pat(&["v", "ts"]), on_pair)])),
    )
}

pub(super) fn summary_hooks(b: &ExprBuilder, max_age: Expr, max_size: Expr, quants: Expr) -> Expr {
    let get_rec = b.at(ExprKind::Record(vec![
        ("quantiles".into(), calc_quants_expr(b)),
        ("count".into(), b.loc("cnt")),
        ("min".into(), b.loc("lo")),
        ("max".into(), b.loc("hi")),
        ("sum".into(), b.loc("sum")),
    ]));
    let get = b.lam(
        &["_g"],
        b.match_(
            b.rget(b.loc("sr")),
            [arm(b.tup_pat(&["obs", "cnt", "sum", "lo", "hi"]), get_rec)],
        ),
    );
    let clamp_ms = b.if_(
        b.bin(BinOp::Lt, b.loc("ms"), b.float(0.0)),
        b.float(0.0),
        b.loc("ms"),
    );
    let mage = b.match_(
        b.app(b.global("Duration", "to_millis"), max_age),
        [
            arm(b.some_pat("ms"), clamp_ms),
            arm(Pat::Wild, b.global("Float", "infinity")),
        ],
    );
    let init_sr = b.rmake(b.tuple([
        b.list(vec![]),
        b.float(0.0),
        b.float(0.0),
        b.float(f64::MAX),
        b.float(-f64::MAX),
    ]));
    b.lam(
        &["_u"],
        b.block(
            [
                b.let_("qs", sort_floats_expr(b, quants)),
                b.let_("_qc", check_quantiles_expr(b)),
                b.let_("mage", mage),
                b.let_("msz", max_size),
                b.let_("sr", init_sr),
                b.let_("uf", summary_upd_expr(b)),
            ],
            b.tuple([get, b.loc("uf"), b.loc("uf")]),
        ),
    )
}
