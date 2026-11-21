//! Counter, Gauge, Frequency, and Histogram metric hook builders.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_observe_refs::upsert_pair_list;
use super::super::{Expr, ExprKind, Pat};
use super::fadd;
use super::registry::sort_floats_expr;

pub(super) fn counter_hooks(b: &ExprBuilder, inc: Expr) -> Expr {
    let allow = b.bin(
        BinOp::Or,
        b.not(b.loc("inc")),
        b.bin(BinOp::Ge, b.loc("v"), b.float(0.0)),
    );
    let bump = b.no_preempt(b.block(
        [b.rset(b.loc("cr"), fadd(b, b.rget(b.loc("cr")), b.loc("v")))],
        b.unit(),
    ));
    let upd = b.lam(&["v"], b.if_(allow, bump, b.unit()));
    let get = b.lam(
        &["_g"],
        b.at(ExprKind::Record(vec![
            ("count".into(), b.rget(b.loc("cr"))),
            ("incremental".into(), b.loc("inc")),
        ])),
    );
    b.lam(
        &["_u"],
        b.block(
            [
                b.let_("inc", inc),
                b.let_("cr", b.rmake(b.float(0.0))),
                b.let_("uf", upd),
            ],
            b.tuple([get, b.loc("uf"), b.loc("uf")]),
        ),
    )
}

pub(super) fn gauge_hooks(b: &ExprBuilder) -> Expr {
    let get = b.lam(
        &["_g"],
        b.at(ExprKind::Record(vec![(
            "value".into(),
            b.rget(b.loc("gr")),
        )])),
    );
    let set = b.lam(&["v"], b.block([b.rset(b.loc("gr"), b.loc("v"))], b.unit()));
    let modify = b.lam(
        &["v"],
        b.no_preempt(b.block(
            [b.rset(b.loc("gr"), fadd(b, b.rget(b.loc("gr")), b.loc("v")))],
            b.unit(),
        )),
    );
    b.lam(
        &["_u"],
        b.block(
            [b.let_("gr", b.rmake(b.float(0.0)))],
            b.tuple([get, set, modify]),
        ),
    )
}

pub(super) fn frequency_hooks(b: &ExprBuilder, pre: Expr) -> Expr {
    let init_loop = b.for_ctrl(
        "fpre",
        "w",
        pre,
        b.block(
            [b.assign(
                "init_occs",
                upsert_pair_list(b, b.var("init_occs"), b.loc("w"), b.float(0.0)),
            )],
            b.unit(),
        ),
    );
    let inc_word = b.block(
        [
            b.assign("found", b.bool_(true)),
            b.assign(
                "out",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("out"),
                    b.list([b.tuple([b.loc("wk"), fadd(b, b.loc("wc"), b.float(1.0))])]),
                ),
            ),
        ],
        b.unit(),
    );
    let keep_word = b.block(
        [b.assign(
            "out",
            b.app2(
                b.global("List.Core", "append"),
                b.var("out"),
                b.list([b.loc("p")]),
            ),
        )],
        b.unit(),
    );
    let bump_step = b.match_(
        b.loc("p"),
        [arm(
            b.tup_pat(&["wk", "wc"]),
            b.if_(
                b.bin(BinOp::Eq, b.loc("wk"), b.loc("w")),
                inc_word,
                keep_word,
            ),
        )],
    );
    let bump = b.for_ctrl("fup", "p", b.rget(b.loc("fr")), bump_step);
    let append_new = b.app2(
        b.global("List.Core", "append"),
        b.var("out"),
        b.list([b.tuple([b.loc("w"), b.float(1.0)])]),
    );
    let upd = b.lam(
        &["w"],
        b.no_preempt(b.block(
            [
                b.var_decl("out", b.list(vec![])),
                b.var_decl("found", b.bool_(false)),
                b.let_("_bl", bump),
                b.let_("fin", b.if_(b.var("found"), b.var("out"), append_new)),
                b.rset(b.loc("fr"), b.loc("fin")),
            ],
            b.unit(),
        )),
    );
    let get = b.lam(
        &["_g"],
        b.at(ExprKind::Record(vec![(
            "occurrences".into(),
            b.rget(b.loc("fr")),
        )])),
    );
    b.lam(
        &["_u"],
        b.block(
            [
                b.var_decl("init_occs", b.list(vec![])),
                b.let_("_il", init_loop),
                b.let_("fr", b.rmake(b.var("init_occs"))),
                b.let_("uf", upd),
            ],
            b.tuple([get, b.loc("uf"), b.loc("uf")]),
        ),
    )
}

fn hist_init_counts(b: &ExprBuilder) -> Expr {
    let push_zero = b.block(
        [b.assign(
            "zs",
            b.app2(
                b.global("List.Core", "append"),
                b.var("zs"),
                b.list([b.float(0.0)]),
            ),
        )],
        b.unit(),
    );
    b.block(
        [
            b.var_decl("zs", b.list([b.float(0.0)])),
            b.let_("_zl", b.for_ctrl("hz", "_b", b.loc("sbounds"), push_zero)),
        ],
        b.var("zs"),
    )
}

fn hist_find_slot(b: &ExprBuilder) -> Expr {
    let step = b.if_(
        b.bin(BinOp::Le, b.loc("v"), b.loc("bd")),
        b.break_("hslot", b.unit()),
        b.block(
            [b.assign("slot", b.add_i(b.var("slot"), b.int(1)))],
            b.unit(),
        ),
    );
    b.block(
        [
            b.var_decl("slot", b.int(0)),
            b.let_("_fl", b.for_ctrl("hslot", "bd", b.loc("sbounds"), step)),
        ],
        b.var("slot"),
    )
}

fn hist_inc_slot(b: &ExprBuilder) -> Expr {
    let nv = b.if_(
        b.bin(BinOp::Eq, b.var("ci"), b.loc("sl")),
        fadd(b, b.loc("cv"), b.float(1.0)),
        b.loc("cv"),
    );
    let step = b.block(
        [
            b.let_("nv", nv),
            b.assign(
                "nc",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("nc"),
                    b.list([b.loc("nv")]),
                ),
            ),
            b.assign("ci", b.add_i(b.var("ci"), b.int(1))),
        ],
        b.unit(),
    );
    b.block(
        [
            b.var_decl("ci", b.int(0)),
            b.var_decl("nc", b.list(vec![])),
            b.let_("_cl", b.for_ctrl("hinc", "cv", b.loc("cts"), step)),
        ],
        b.var("nc"),
    )
}

fn hist_build_buckets(b: &ExprBuilder) -> Expr {
    let push_bucket = b.block(
        [
            b.assign("cum", fadd(b, b.var("cum"), b.loc("c0"))),
            b.assign(
                "bks",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("bks"),
                    b.list([b.tuple([b.loc("bd"), b.var("cum")])]),
                ),
            ),
            b.assign(
                "rem",
                b.app2(b.global("List.Core", "drop"), b.int(1), b.var("rem")),
            ),
        ],
        b.unit(),
    );
    let head = b.app2(b.global("List.Core", "take"), b.int(1), b.var("rem"));
    let step = b.match_(
        head,
        [
            arm(Pat::List(vec![Pat::Bind(b.name("c0"))], None), push_bucket),
            arm(Pat::Wild, b.unit()),
        ],
    );
    b.block(
        [
            b.var_decl("bks", b.list(vec![])),
            b.var_decl("cum", b.float(0.0)),
            b.var_decl("rem", b.loc("cts")),
            b.let_("_bkl", b.for_ctrl("hbk", "bd", b.loc("sbounds"), step)),
        ],
        b.var("bks"),
    )
}

pub(super) fn histogram_hooks(b: &ExprBuilder, bounds: Expr) -> Expr {
    let next_state = b.tuple([
        b.loc("ncts"),
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
    let upd_body = b.block(
        [
            b.let_("sl", hist_find_slot(b)),
            b.let_("ncts", hist_inc_slot(b)),
            b.rset(b.loc("hr"), next_state),
        ],
        b.unit(),
    );
    let upd = b.lam(
        &["v"],
        b.no_preempt(b.match_(
            b.rget(b.loc("hr")),
            [arm(b.tup_pat(&["cts", "cnt", "sum", "lo", "hi"]), upd_body)],
        )),
    );
    let snapshot_rec = b.at(ExprKind::Record(vec![
        ("buckets".into(), hist_build_buckets(b)),
        ("count".into(), b.loc("cnt")),
        ("min".into(), b.loc("lo")),
        ("max".into(), b.loc("hi")),
        ("sum".into(), b.loc("sum")),
    ]));
    let get = b.lam(
        &["_g"],
        b.match_(
            b.rget(b.loc("hr")),
            [arm(
                b.tup_pat(&["cts", "cnt", "sum", "lo", "hi"]),
                snapshot_rec,
            )],
        ),
    );
    let init_hr = b.rmake(b.tuple([
        hist_init_counts(b),
        b.float(0.0),
        b.float(0.0),
        b.float(f64::MAX),
        b.float(-f64::MAX),
    ]));
    b.lam(
        &["_u"],
        b.block(
            [
                b.let_("sbounds", sort_floats_expr(b, bounds)),
                b.let_("hr", init_hr),
                b.let_("uf", upd),
            ],
            b.tuple([get, b.loc("uf"), b.loc("uf")]),
        ),
    )
}
