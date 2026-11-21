//! Streaming UTF-8 text decode and encode channel IR helpers.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt, str_lit};
use super::super::{Expr, Pat, Stmt};

fn utf8_need_len(b: &ExprBuilder, lead: Expr) -> Expr {
    let three_or_four = b.if_(
        b.bin(BinOp::Lt, lead.clone(), b.int(0xF0)),
        b.int(3),
        b.int(4),
    );
    let two_or_more = b.if_(
        b.bin(BinOp::Lt, lead.clone(), b.int(0xE0)),
        b.int(2),
        three_or_four,
    );
    b.if_(b.bin(BinOp::Lt, lead, b.int(0xC0)), b.int(1), two_or_more)
}

fn byte_eq(b: &ExprBuilder, idx: i32, val: i32) -> Expr {
    let at = b.app2(rt(b, "Helper.list_at"), b.int(idx), b.var("bs"));
    b.bin(BinOp::Eq, at, b.int(val))
}

fn partial_bom_cond(b: &ExprBuilder) -> Expr {
    let second_ok = b.if_(
        b.bin(BinOp::Eq, b.loc("bl"), b.int(2)),
        byte_eq(b, 1, 0xBB),
        b.bool_(true),
    );
    let prefix_ok = b.bin(BinOp::And, byte_eq(b, 0, 0xEF), second_ok);
    let len_lt3 = b.bin(
        BinOp::And,
        b.bin(BinOp::Lt, b.loc("bl"), b.int(3)),
        prefix_ok,
    );
    let len_pos = b.bin(BinOp::And, b.bin(BinOp::Gt, b.loc("bl"), b.int(0)), len_lt3);
    b.bin(BinOp::And, b.not(b.var("st2")), len_pos)
}

fn strip_bom_stmt(b: &ExprBuilder) -> Stmt {
    let bom_tail = b.bin(BinOp::And, byte_eq(b, 1, 0xBB), byte_eq(b, 2, 0xBF));
    let bom_bytes = b.bin(BinOp::And, byte_eq(b, 0, 0xEF), bom_tail);
    let has_bom = b.bin(
        BinOp::And,
        b.bin(BinOp::Ge, b.loc("bl"), b.int(3)),
        bom_bytes,
    );
    let drop_bom = b.if_(
        has_bom,
        b.assign_e(
            "bs",
            b.app2(b.global("List.Core", "drop"), b.int(3), b.var("bs")),
        ),
        b.unit(),
    );
    let start_block = b.block(
        [b.assign("st2", b.bool_(true)), Stmt::Expr(drop_bom)],
        b.unit(),
    );
    let first_non_empty = b.bin(
        BinOp::And,
        b.not(b.var("st2")),
        b.bin(BinOp::Gt, b.loc("bl"), b.int(0)),
    );
    Stmt::Expr(b.if_(first_non_empty, start_block, b.unit()))
}

fn find_utf8_cut_loop(b: &ExprBuilder) -> Stmt {
    let is_cont = b.bin(
        BinOp::And,
        b.bin(BinOp::Ge, b.loc("c"), b.int(0x80)),
        b.bin(BinOp::Lt, b.loc("c"), b.int(0xC0)),
    );
    let incomplete = b.bin(
        BinOp::And,
        b.bin(BinOp::Ge, b.loc("c"), b.int(0xC0)),
        b.bin(
            BinOp::Lt,
            b.sub_i(b.loc("n"), b.var("k")),
            utf8_need_len(b, b.loc("c")),
        ),
    );
    let on_lead = b.block(
        [
            Stmt::Expr(b.if_(incomplete, b.assign_e("cut", b.var("k")), b.unit())),
            b.assign("go", b.bool_(false)),
        ],
        b.unit(),
    );
    let step = b.block(
        [b.let_(
            "c",
            b.app2(rt(b, "Helper.list_at"), b.var("k"), b.var("bs")),
        )],
        b.if_(
            is_cont,
            b.assign_e("k", b.sub_i(b.var("k"), b.int(1))),
            on_lead,
        ),
    );
    let cond = b.bin(
        BinOp::And,
        b.var("go"),
        b.bin(BinOp::Ge, b.var("k"), b.loc("lo")),
    );
    Stmt::Expr(b.while_(cond, step))
}

pub(super) fn ch_utf8_stream_step_helper(b: &ExprBuilder) -> Expr {
    // (state_ref : Ref (List Int, Bool), bytes : List Int) -> String
    // Streaming UTF-8 decoder: drops leading UTF-8 BOM, buffers trailing incomplete multibyte sequence.
    let wait_bom = b.block(
        [b.rset(b.loc("st"), b.tuple([b.var("bs"), b.bool_(false)]))],
        str_lit(b, ""),
    );
    let n_minus_3 = b.sub_i(b.loc("n"), b.int(3));
    let lo_expr = b.if_(
        b.bin(BinOp::Gt, n_minus_3.clone(), b.int(0)),
        n_minus_3,
        b.int(0),
    );
    let decoded = b.match_(
        b.app(rt(b, "Bytes.Utf8Decode"), b.loc("ready")),
        [
            arm(b.some_pat("txt"), b.loc("txt")),
            arm(Pat::Wild, str_lit(b, "\u{FFFD}")),
        ],
    );
    let decode_body = b.block(
        [
            strip_bom_stmt(b),
            b.let_("n", b.app(b.global("List.Core", "length"), b.var("bs"))),
            b.var_decl("cut", b.loc("n")),
            b.var_decl("k", b.sub_i(b.loc("n"), b.int(1))),
            b.let_("lo", lo_expr),
            b.var_decl("go", b.bool_(true)),
            find_utf8_cut_loop(b),
            b.let_(
                "ready",
                b.app2(b.global("List.Core", "take"), b.var("cut"), b.var("bs")),
            ),
            b.let_(
                "next_pend",
                b.app2(b.global("List.Core", "drop"), b.var("cut"), b.var("bs")),
            ),
            b.rset(b.loc("st"), b.tuple([b.loc("next_pend"), b.var("st2")])),
        ],
        decoded,
    );
    let merged = b.app2(
        b.global("List.Core", "append"),
        b.loc("pend"),
        b.loc("chunk"),
    );
    let body = b.block(
        [
            b.var_decl("bs", merged),
            b.var_decl("st2", b.loc("started")),
            b.let_("bl", b.app(b.global("List.Core", "length"), b.var("bs"))),
        ],
        b.if_(partial_bom_cond(b), wait_bom, decode_body),
    );
    b.lam(
        &["st", "chunk"],
        b.match_(
            b.rget(b.loc("st")),
            [arm(pair_pat(b, "pend", "started"), body)],
        ),
    )
}

pub(super) fn ch_decode_text_helper(b: &ExprBuilder) -> Expr {
    let decode_each = b.gen_strict(b.each(
        "bs",
        b.loc("xs"),
        b.yield_(b.app2(
            rt(b, "Helper.ch_utf8_stream_step"),
            b.loc("st"),
            b.loc("bs"),
        )),
    ));
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.app(b.loc("up"), b.unit()),
            [
                arm(b.ok_pat("xs"), b.ok(decode_each)),
                arm(b.err_pat("d"), b.err(b.loc("d"))),
            ],
        ),
    );
    let init_st = b.rmake(b.tuple([b.list(vec![]), b.bool_(false)]));
    b.lam(&["up", "_own"], b.block([b.let_("st", init_st)], pull))
}

pub(super) fn ch_encode_text_helper(b: &ExprBuilder) -> Expr {
    let encode_each = b.gen_strict(b.each(
        "s",
        b.loc("xs"),
        b.yield_(b.app(rt(b, "Bytes.Utf8Encode"), b.loc("s"))),
    ));
    let pull = b.lam(
        &["_u"],
        b.match_(
            b.app(b.loc("up"), b.unit()),
            [
                arm(b.ok_pat("xs"), b.ok(encode_each)),
                arm(b.err_pat("d"), b.err(b.loc("d"))),
            ],
        ),
    );
    b.lam(&["up", "_own"], pull)
}
