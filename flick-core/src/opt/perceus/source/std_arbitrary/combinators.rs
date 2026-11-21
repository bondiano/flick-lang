//! Pure combinator templates (`map`, `filter`, `filterMap`, `flatMap`) for `Arbitrary`.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat};
use super::{add_f, st_pat};

pub(super) fn map_gen_template(b: &ExprBuilder) -> Expr {
    let mapped = b.some(b.app2(rt(b, "Helper.arb_map_sample"), b.loc("s"), b.loc("f")));
    let gen_body = b.match_(
        b.app(b.loc("gen"), b.loc("st")),
        [arm(b.some_pat("s"), mapped), arm(Pat::Wild, b.none())],
    );
    let pair = b.tuple([b.loc("mc"), b.lam(&["st"], gen_body)]);
    b.lam(
        &["f", "self"],
        b.match_(b.loc("self"), [arm(pair_pat(b, "mc", "gen"), pair)]),
    )
}

pub(super) fn filter_gen_template(b: &ExprBuilder) -> Expr {
    let keep = b.if_(
        b.app(b.loc("pred"), b.loc("v")),
        b.some(b.loc("s")),
        b.none(),
    );
    let unshrunk = b.match_(b.loc("s"), [arm(pair_pat(b, "v", "_"), keep)]);
    let shrunk = b.app2(rt(b, "Helper.arb_filter_sample"), b.loc("s"), b.loc("pred"));
    let on_some = b.match_(
        b.loc("st"),
        [arm(st_pat(b), b.if_(b.loc("shk"), shrunk, unshrunk))],
    );
    let gen_body = b.match_(
        b.app(b.loc("gen"), b.loc("st")),
        [arm(b.some_pat("s"), on_some), arm(Pat::Wild, b.none())],
    );
    let pair = b.tuple([b.loc("mc"), b.lam(&["st"], gen_body)]);
    b.lam(
        &["pred", "self"],
        b.match_(b.loc("self"), [arm(pair_pat(b, "mc", "gen"), pair)]),
    )
}

pub(super) fn filter_map_gen_template(b: &ExprBuilder) -> Expr {
    let fm_match = b.match_(
        b.app(b.loc("f"), b.loc("v")),
        [
            arm(b.ok_pat("x"), b.some(b.loc("x"))),
            arm(Pat::Wild, b.none()),
        ],
    );
    let fm_let = b.let_("fm", b.lam(&["v"], fm_match));
    let mapped_val = b.some(b.tuple([b.loc("mv"), b.none()]));
    let unshrunk_inner = b.match_(
        b.app(b.loc("fm"), b.loc("v")),
        [arm(b.some_pat("mv"), mapped_val), arm(Pat::Wild, b.none())],
    );
    let unshrunk = b.match_(b.loc("s"), [arm(pair_pat(b, "v", "_"), unshrunk_inner)]);
    let shrunk = b.app2(
        rt(b, "Helper.arb_filter_map_sample"),
        b.loc("s"),
        b.loc("fm"),
    );
    let on_some = b.match_(
        b.loc("st"),
        [arm(st_pat(b), b.if_(b.loc("shk"), shrunk, unshrunk))],
    );
    let gen_body = b.match_(
        b.app(b.loc("gen"), b.loc("st")),
        [arm(b.some_pat("s"), on_some), arm(Pat::Wild, b.none())],
    );
    let body = b.block([fm_let], b.tuple([b.loc("mc"), b.lam(&["st"], gen_body)]));
    b.lam(
        &["f", "self"],
        b.match_(b.loc("self"), [arm(pair_pat(b, "mc", "gen"), body)]),
    )
}

fn flat_map_unshrunk(b: &ExprBuilder) -> Expr {
    let clamped = b.if_(
        b.bin(BinOp::Lt, b.loc("nb"), b.loc("res")),
        b.loc("nb"),
        b.loc("res"),
    );
    let run_tgen = b.block(
        [
            b.let_("res", b.rget(b.loc("br"))),
            b.rset(b.loc("br"), add_f(b, b.loc("res"), b.loc("tmc"))),
            b.let_("s", b.app(b.loc("tgen"), b.loc("st"))),
            b.let_("nb", b.rget(b.loc("br"))),
            b.rset(b.loc("br"), clamped),
        ],
        b.loc("s"),
    );
    let on_sv = b.match_(
        b.app(b.loc("f"), b.loc("sv")),
        [arm(pair_pat(b, "tmc", "tgen"), run_tgen)],
    );
    let on_src = b.match_(b.loc("src"), [arm(pair_pat(b, "sv", "_"), on_sv)]);
    b.match_(
        b.app(b.loc("gen"), b.loc("st")),
        [arm(b.some_pat("src"), on_src), arm(Pat::Wild, b.none())],
    )
}

fn flat_map_shrunk_run(b: &ExprBuilder) -> Expr {
    let on_tgt = b.some(b.app6(
        rt(b, "Helper.arb_flat_map_sample"),
        [
            b.loc("src"),
            b.loc("tgt"),
            b.loc("f"),
            b.loc("ckpt"),
            b.loc("st"),
            b.loc("res"),
        ],
    ));
    let match_t_opt = b.match_(
        b.loc("t_opt"),
        [arm(b.some_pat("tgt"), on_tgt), arm(Pat::Wild, b.none())],
    );
    let tst = b.tuple([
        b.loc("sz"),
        b.loc("shk"),
        b.loc("bias"),
        b.loc("trr"),
        b.loc("tbr"),
    ]);
    let clamped = b.if_(
        b.bin(BinOp::Lt, b.loc("tb"), b.loc("res")),
        b.loc("tb"),
        b.loc("res"),
    );
    b.block(
        [
            b.let_("res", b.rget(b.loc("sbr"))),
            b.let_("ckpt", b.rget(b.loc("srr"))),
            b.let_("trr", b.rmake(b.loc("ckpt"))),
            b.let_("tbr", b.rmake(add_f(b, b.loc("res"), b.loc("tmc")))),
            b.let_("tst", tst),
            b.let_("t_opt", b.app(b.loc("tgen"), b.loc("tst"))),
            b.rset(b.loc("rr"), b.rget(b.loc("trr"))),
            b.let_("tb", b.rget(b.loc("tbr"))),
            b.rset(b.loc("br"), clamped),
        ],
        match_t_opt,
    )
}

fn flat_map_shrunk(b: &ExprBuilder) -> Expr {
    let on_sv = b.match_(
        b.app(b.loc("f"), b.loc("sv")),
        [arm(pair_pat(b, "tmc", "tgen"), flat_map_shrunk_run(b))],
    );
    let on_src = b.match_(b.loc("src"), [arm(pair_pat(b, "sv", "_"), on_sv)]);
    let on_none = b.block(
        [
            b.rset(b.loc("rr"), b.rget(b.loc("srr"))),
            b.rset(b.loc("br"), b.rget(b.loc("sbr"))),
        ],
        b.none(),
    );
    let sst = b.tuple([
        b.loc("sz"),
        b.loc("shk"),
        b.loc("bias"),
        b.loc("srr"),
        b.loc("sbr"),
    ]);
    b.block(
        [
            b.let_("srr", b.rmake(b.rget(b.loc("rr")))),
            b.let_("sbr", b.rmake(b.rget(b.loc("br")))),
            b.let_("sst", sst),
        ],
        b.match_(
            b.app(b.loc("gen"), b.loc("sst")),
            [arm(b.some_pat("src"), on_src), arm(Pat::Wild, on_none)],
        ),
    )
}

pub(super) fn flat_map_gen_template(b: &ExprBuilder) -> Expr {
    let st_branch = b.if_(
        b.not(b.loc("shk")),
        flat_map_unshrunk(b),
        flat_map_shrunk(b),
    );
    let gen_body = b.match_(b.loc("st"), [arm(st_pat(b), st_branch)]);
    let pair = b.tuple([b.loc("mc"), b.lam(&["st"], gen_body)]);
    b.lam(
        &["f", "self"],
        b.match_(b.loc("self"), [arm(pair_pat(b, "mc", "gen"), pair)]),
    )
}
