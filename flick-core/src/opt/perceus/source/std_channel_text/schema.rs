//! `ChannelSchema` codec decode/encode IR helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{con_app, rt, str_lit};
use super::super::{Expr, ExprKind, Pat, Stmt};

pub(super) fn ch_schema_unreport_helper(b: &ExprBuilder) -> Expr {
    // Strip `, got ...` suffix from `expected ...` messages and normalize `has no JSON form`.
    let starts_expected = b.bin(
        BinOp::And,
        b.bin(BinOp::Ge, b.loc("ml"), b.int(9)),
        b.bin(
            BinOp::Eq,
            b.app3(b.global("String", "slice"), b.int(0), b.int(9), b.loc("m")),
            str_lit(b, "expected "),
        ),
    );
    let strip_got = b.if_(
        starts_expected,
        b.app3(
            b.global("String", "slice"),
            b.int(0),
            b.loc("at"),
            b.loc("m"),
        ),
        b.loc("m"),
    );
    let suffix = b.app3(
        b.global("String", "slice"),
        b.sub_i(b.loc("ml"), b.int(17)),
        b.loc("ml"),
        b.loc("m"),
    );
    let ends_no_json = b.bin(
        BinOp::And,
        b.bin(BinOp::Ge, b.loc("ml"), b.int(17)),
        b.bin(BinOp::Eq, suffix, str_lit(b, " has no JSON form")),
    );
    let norm_json = b.if_(
        ends_no_json,
        str_lit(b, "expected a finite number"),
        b.loc("m"),
    );
    let idx_got = b.app2(
        b.global("String", "index_of"),
        str_lit(b, ", got "),
        b.loc("m"),
    );
    let m2 = b.match_(
        idx_got,
        [arm(b.some_pat("at"), strip_got), arm(Pat::Wild, norm_json)],
    );
    let rec = b.at(ExprKind::Record(vec![
        ("message".into(), b.loc("m2")),
        ("path".into(), b.field(b.loc("is"), "path")),
    ]));
    let clean_msg = b.block(
        [
            b.let_("m", b.field(b.loc("is"), "message")),
            b.let_("ml", b.app(b.global("String", "length"), b.loc("m"))),
            b.let_("m2", m2),
        ],
        rec,
    );
    b.lam(
        &["issues"],
        b.gen_strict(b.each("is", b.loc("issues"), b.yield_(clean_msg))),
    )
}

fn codec_elem_step(b: &ExprBuilder) -> Expr {
    let run_one = b.if_(
        b.loc("is_dec"),
        b.app2(rt(b, "Helper.codec_dec"), b.loc("codec"), b.loc("x")),
        b.app2(rt(b, "Helper.codec_enc"), b.loc("codec"), b.loc("x")),
    );
    let push_v = b.assign_e(
        "out",
        b.app2(
            b.global("List.Core", "append"),
            b.var("out"),
            b.list([b.loc("v")]),
        ),
    );
    let combine_err = b.assign_e(
        "errs",
        b.some(b.app2(rt(b, "Schema.CombineErrors"), b.loc("prev"), b.loc("pe"))),
    );
    let on_err = b.block(
        [b.let_(
            "pe",
            b.app2(rt(b, "Schema.PrefixIssues"), b.loc("ci"), b.loc("se")),
        )],
        b.match_(
            b.var("errs"),
            [
                arm(b.some_pat("prev"), combine_err),
                arm(Pat::Wild, b.assign_e("errs", b.some(b.loc("pe")))),
            ],
        ),
    );
    b.block(
        [
            b.let_("ci", b.var("idx")),
            b.assign("idx", b.add_i(b.loc("ci"), b.int(1))),
        ],
        b.match_(
            run_one,
            [arm(b.ok_pat("v"), push_v), arm(b.err_pat("se"), on_err)],
        ),
    )
}

pub(super) fn ch_schema_codec_helper(b: &ExprBuilder) -> Expr {
    // (is_decode: Bool, codec) -> Channel
    let cleaned_err = con_app(
        b,
        "Prelude",
        "SchemaError",
        "SchemaError",
        vec![b.app(rt(b, "Helper.ch_schema_unreport"), b.loc("iss"))],
    );
    let se_pat = Pat::Con(
        ExprBuilder::con("Prelude", "SchemaError", "SchemaError", 1),
        vec![Pat::Bind(b.name("iss"))],
    );
    let fail_se = b.match_(
        b.loc("se"),
        [arm(se_pat, b.app(b.global("Fail", "fail"), cleaned_err))],
    );
    let finish_chunk = b.match_(
        b.var("errs"),
        [
            arm(b.some_pat("se"), fail_se),
            arm(Pat::Wild, b.ok(b.var("out"))),
        ],
    );
    let on_chunk = b.block(
        [
            b.var_decl("out", b.list(vec![])),
            b.var_decl("errs", b.none()),
            b.var_decl("idx", b.int(0)),
            Stmt::Expr(b.each("x", b.loc("xs"), codec_elem_step(b))),
        ],
        finish_chunk,
    );
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.app(b.loc("up"), b.unit()),
            [
                arm(b.ok_pat("xs"), on_chunk),
                arm(b.err_pat("d"), b.err(b.loc("d"))),
            ],
        ),
    );
    b.lam(&["is_dec", "codec", "up", "_own"], pull)
}
