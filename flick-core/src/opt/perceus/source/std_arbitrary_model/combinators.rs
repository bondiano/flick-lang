//! Filter, filter-map, and flat-map shrink-tree helpers for `Arbitrary`.

use flick_ir::{Arith, BinOp, Num};

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat, Stmt};
use super::rset_e;

fn filter_map_on_sample(b: &ExprBuilder) -> Expr {
    let nsh = b.match_(
        b.loc("sh_opt"),
        [
            arm(
                b.some_pat("sh"),
                b.some(b.app2(rt(b, "Helper.arb_filter_map_pull"), b.loc("sh"), b.loc("f"))),
            ),
            arm(Pat::Wild, b.none()),
        ],
    );
    let on_mv = b.block(
        [
            b.let_("nsh", nsh),
            b.assign("out", b.some(b.some(b.tuple([b.loc("mv"), b.loc("nsh")])))),
            b.assign("done", b.bool_(true)),
        ],
        b.unit(),
    );
    let push_sh = rset_e(
        b,
        b.loc("sr"),
        b.app2(
            b.global("List.Core", "append"),
            b.loc("stk"),
            b.list([b.loc("sh")]),
        ),
    );
    let on_reject = b.block(
        [
            Stmt::Expr(b.match_(
                b.loc("sh_opt"),
                [arm(b.some_pat("sh"), push_sh), arm(Pat::Wild, b.unit())],
            )),
            b.assign("out", b.some(b.none())),
            b.assign("done", b.bool_(true)),
        ],
        b.unit(),
    );
    let check_f = b.match_(
        b.app(b.loc("f"), b.loc("v")),
        [arm(b.some_pat("mv"), on_mv), arm(Pat::Wild, on_reject)],
    );
    b.match_(b.loc("s"), [arm(pair_pat(b, "v", "sh_opt"), check_f)])
}

pub(super) fn filter_map_pull_helper(b: &ExprBuilder) -> Expr {
    let on_skip = b.block(
        [
            b.assign("out", b.some(b.none())),
            b.assign("done", b.bool_(true)),
        ],
        b.unit(),
    );
    let on_opt_s = b.match_(
        b.loc("opt_s"),
        [
            arm(b.some_pat("s"), filter_map_on_sample(b)),
            arm(Pat::Wild, on_skip),
        ],
    );
    let pop_stk = rset_e(
        b,
        b.loc("sr"),
        b.app2(
            b.global("List.Core", "take"),
            b.sub_i(b.loc("n"), b.int(1)),
            b.loc("stk"),
        ),
    );
    let step_body = b.block(
        [
            b.let_("stk", b.rget(b.loc("sr"))),
            b.let_("n", b.app(b.global("List.Core", "length"), b.loc("stk"))),
            b.let_(
                "top",
                b.app2(
                    rt(b, "Helper.list_at"),
                    b.sub_i(b.loc("n"), b.int(1)),
                    b.loc("stk"),
                ),
            ),
        ],
        b.match_(
            b.app(b.loc("top"), b.unit()),
            [arm(b.some_pat("opt_s"), on_opt_s), arm(Pat::Wild, pop_stk)],
        ),
    );
    let cond = b.bin(
        BinOp::And,
        b.not(b.var("done")),
        b.bin(
            BinOp::Gt,
            b.app(b.global("List.Core", "length"), b.rget(b.loc("sr"))),
            b.int(0),
        ),
    );
    let pull = b.lam(
        &["_u"],
        b.block(
            [
                b.var_decl("out", b.none()),
                b.var_decl("done", b.bool_(false)),
                Stmt::Expr(b.while_(cond, step_body)),
            ],
            b.var("out"),
        ),
    );
    b.lam(
        &["src", "f"],
        b.block([b.let_("sr", b.rmake(b.list([b.loc("src")])))], pull),
    )
}

pub(super) fn filter_map_sample_helper(b: &ExprBuilder) -> Expr {
    let sh_mapped = b.match_(
        b.loc("sh_opt"),
        [
            arm(
                b.some_pat("sh"),
                b.some(b.app2(rt(b, "Helper.arb_filter_map_pull"), b.loc("sh"), b.loc("f"))),
            ),
            arm(Pat::Wild, b.none()),
        ],
    );
    let check_f = b.match_(
        b.app(b.loc("f"), b.loc("v")),
        [
            arm(b.some_pat("mv"), b.some(b.tuple([b.loc("mv"), sh_mapped]))),
            arm(Pat::Wild, b.none()),
        ],
    );
    b.lam(
        &["s", "f"],
        b.match_(b.loc("s"), [arm(pair_pat(b, "v", "sh_opt"), check_f)]),
    )
}

pub(super) fn filter_sample_helper(b: &ExprBuilder) -> Expr {
    let pred = b.lam(
        &["v"],
        b.if_(
            b.app(b.loc("keep"), b.loc("v")),
            b.some(b.loc("v")),
            b.none(),
        ),
    );
    b.lam(
        &["s", "keep"],
        b.app2(rt(b, "Helper.arb_filter_map_sample"), b.loc("s"), pred),
    )
}

pub(super) fn flat_map_sample_helper(b: &ExprBuilder) -> Expr {
    let srcs_let = b.let_(
        "srcs",
        b.app5(
            rt(b, "Helper.arb_flat_map_source_pull"),
            b.loc("sp"),
            b.loc("f"),
            b.loc("ckpt"),
            b.loc("st"),
            b.loc("res"),
        ),
    );
    let combined = b.match_(
        b.loc("tsh"),
        [
            arm(
                b.some_pat("tp"),
                b.app(
                    rt(b, "Helper.arb_concat_pulls"),
                    b.list([b.loc("srcs"), b.loc("tp")]),
                ),
            ),
            arm(Pat::Wild, b.loc("srcs")),
        ],
    );
    let with_sp = b.match_(
        b.loc("tgt"),
        [arm(
            pair_pat(b, "tv", "tsh"),
            b.block([srcs_let], b.tuple([b.loc("tv"), b.some(combined)])),
        )],
    );
    let match_ssh = b.match_(
        b.loc("ssh"),
        [arm(b.some_pat("sp"), with_sp), arm(Pat::Wild, b.loc("tgt"))],
    );
    b.lam(
        &["src", "tgt", "f", "ckpt", "st", "res"],
        b.match_(b.loc("src"), [arm(pair_pat(b, "_sv", "ssh"), match_ssh)]),
    )
}

fn flat_map_source_on_sample(b: &ExprBuilder) -> Expr {
    let st_pat = b.tup_pat(&["sz", "shk", "bias", "_rr", "_br"]);
    let nst = b.tuple([
        b.loc("sz"),
        b.loc("shk"),
        b.loc("bias"),
        b.rmake(b.loc("ckpt")),
        b.rmake(b.bin(
            BinOp::Arith(Arith::Add, Num::Float),
            b.loc("res"),
            b.loc("tmc"),
        )),
    ]);
    let fm_sample = b.app6(
        rt(b, "Helper.arb_flat_map_sample"),
        [
            b.loc("s"),
            b.loc("t"),
            b.loc("f"),
            b.loc("ckpt"),
            b.loc("st"),
            b.loc("res"),
        ],
    );
    let on_t = b.block(
        [
            b.assign("out", b.some(b.some(fm_sample))),
            b.assign("done", b.bool_(true)),
        ],
        b.unit(),
    );
    let push_sp2 = rset_e(
        b,
        b.loc("sr"),
        b.app2(
            b.global("List.Core", "append"),
            b.loc("stk"),
            b.list([b.loc("sp2")]),
        ),
    );
    let on_no_t = b.block(
        [
            Stmt::Expr(b.match_(
                b.loc("ssh"),
                [arm(b.some_pat("sp2"), push_sp2), arm(Pat::Wild, b.unit())],
            )),
            b.assign("out", b.some(b.none())),
            b.assign("done", b.bool_(true)),
        ],
        b.unit(),
    );
    let run_tgen = b.block(
        [b.let_("nst", nst)],
        b.match_(
            b.app(b.loc("tgen"), b.loc("nst")),
            [arm(b.some_pat("t"), on_t), arm(Pat::Wild, on_no_t)],
        ),
    );
    let on_tgen = b.match_(b.loc("st"), [arm(st_pat, run_tgen)]);
    let on_sv = b.match_(
        b.app(b.loc("f"), b.loc("sv")),
        [arm(pair_pat(b, "tmc", "tgen"), on_tgen)],
    );
    b.match_(b.loc("s"), [arm(pair_pat(b, "sv", "ssh"), on_sv)])
}

pub(super) fn flat_map_source_pull_helper(b: &ExprBuilder) -> Expr {
    let on_skip = b.block(
        [
            b.assign("out", b.some(b.none())),
            b.assign("done", b.bool_(true)),
        ],
        b.unit(),
    );
    let on_opt_s = b.match_(
        b.loc("opt_s"),
        [
            arm(b.some_pat("s"), flat_map_source_on_sample(b)),
            arm(Pat::Wild, on_skip),
        ],
    );
    let pop_stk = rset_e(
        b,
        b.loc("sr"),
        b.app2(
            b.global("List.Core", "take"),
            b.sub_i(b.loc("n"), b.int(1)),
            b.loc("stk"),
        ),
    );
    let step = b.block(
        [
            b.let_("stk", b.rget(b.loc("sr"))),
            b.let_("n", b.app(b.global("List.Core", "length"), b.loc("stk"))),
            b.let_(
                "top",
                b.app2(
                    rt(b, "Helper.list_at"),
                    b.sub_i(b.loc("n"), b.int(1)),
                    b.loc("stk"),
                ),
            ),
        ],
        b.match_(
            b.app(b.loc("top"), b.unit()),
            [arm(b.some_pat("opt_s"), on_opt_s), arm(Pat::Wild, pop_stk)],
        ),
    );
    let cond = b.bin(
        BinOp::And,
        b.not(b.var("done")),
        b.bin(
            BinOp::Gt,
            b.app(b.global("List.Core", "length"), b.rget(b.loc("sr"))),
            b.int(0),
        ),
    );
    let pull = b.lam(
        &["_u"],
        b.block(
            [
                b.var_decl("out", b.none()),
                b.var_decl("done", b.bool_(false)),
                Stmt::Expr(b.while_(cond, step)),
            ],
            b.var("out"),
        ),
    );
    b.lam(
        &["src_p", "f", "ckpt", "st", "res"],
        b.block([b.let_("sr", b.rmake(b.list([b.loc("src_p")])))], pull),
    )
}
