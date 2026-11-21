//! Replay-token check runner (`arb_replay_check`) for `Arbitrary`.

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{con_app, pair_pat, rt, str_lit};
use super::super::{Expr, Pat};
use super::sample::{FalsifiedRecord, falsified_con};

fn mismatch(b: &ExprBuilder, msg: Expr) -> Expr {
    con_app(b, "Arbitrary", "CheckResult", "ReplayMismatch", vec![msg])
}

fn replay_follow_fail(b: &ExprBuilder) -> Expr {
    let falsified = falsified_con(
        b,
        FalsifiedRecord {
            init_v: b.loc("iv"),
            shrunk_v: b.loc("shv"),
            fail_v: b.loc("last_f"),
            runs: b.int(1),
            disc: b.int(0),
            shrinks: b.app(b.global("List.Core", "length"), b.loc("p4")),
            replay: b.loc("replay"),
        },
    );
    let on_sh_s = b.match_(b.loc("sh_s"), [arm(pair_pat(b, "shv", "_"), falsified)]);
    let on_follow = b.if_(
        b.bin(BinOp::NotEq, b.loc("mm"), str_lit(b, "")),
        mismatch(b, b.loc("mm")),
        on_sh_s,
    );
    let follow = b.app4(
        rt(b, "Helper.arb_follow_replay"),
        b.loc("sm"),
        b.loc("f0"),
        b.loc("p4"),
        b.loc("prop"),
    );
    let follow_match = b.match_(
        follow,
        [arm(b.tup_pat(&["sh_s", "last_f", "mm"]), on_follow)],
    );
    b.match_(b.loc("fail0"), [arm(b.some_pat("f0"), follow_match)])
}

fn replay_on_sample(b: &ExprBuilder) -> Expr {
    let check_tag = b.if_(
        b.bin(BinOp::NotEq, b.loc("ftag"), b.loc("p5")),
        mismatch(b, str_lit(b, "ShrinkPassed")),
        replay_follow_fail(b),
    );
    let eval_iv = b.block(
        [
            b.let_(
                "fail0",
                b.app2(rt(b, "Helper.arb_evaluate"), b.loc("prop"), b.loc("iv")),
            ),
            b.let_("ftag", b.app(rt(b, "Helper.arb_prop_tag"), b.loc("fail0"))),
        ],
        b.if_(
            b.bin(BinOp::Eq, b.loc("ftag"), str_lit(b, "")),
            mismatch(b, str_lit(b, "PropertyPassed")),
            check_tag,
        ),
    );
    b.match_(b.loc("sm"), [arm(pair_pat(b, "iv", "_"), eval_iv)])
}

fn replay_valid_parts(b: &ExprBuilder) -> Expr {
    let decoded_num = b.match_(
        b.app(rt(b, "Config.DecodeNumber"), b.loc("p1")),
        [
            arm(b.some_pat("n"), b.loc("n")),
            arm(Pat::Wild, b.float(0.0)),
        ],
    );
    let seed = b.if_(
        b.bin(BinOp::Eq, b.loc("p0"), b.float(0.0)),
        decoded_num,
        b.loc("p1"),
    );
    let attempt = b.app5(
        rt(b, "Helper.arb_gen_attempt"),
        b.loc("arb"),
        b.loc("hashed"),
        b.loc("p2"),
        b.loc("p3"),
        b.bool_(true),
    );
    let stmts = [
        b.let_(
            "p0",
            b.app2(rt(b, "Helper.list_at"), b.int(0), b.loc("parts")),
        ),
        b.let_(
            "p1",
            b.app2(rt(b, "Helper.list_at"), b.int(1), b.loc("parts")),
        ),
        b.let_(
            "p2",
            b.app2(rt(b, "Helper.list_at"), b.int(2), b.loc("parts")),
        ),
        b.let_(
            "p3",
            b.app2(rt(b, "Helper.list_at"), b.int(3), b.loc("parts")),
        ),
        b.let_(
            "p4",
            b.app2(rt(b, "Helper.list_at"), b.int(4), b.loc("parts")),
        ),
        b.let_(
            "p5",
            b.app2(rt(b, "Helper.list_at"), b.int(5), b.loc("parts")),
        ),
        b.let_("seed", seed),
        b.let_("hashed", b.app(rt(b, "Arbitrary.HashSeed"), b.loc("seed"))),
    ];
    let body = b.match_(
        attempt,
        [
            arm(b.some_pat("sm"), replay_on_sample(b)),
            arm(Pat::Wild, mismatch(b, str_lit(b, "AttemptDiscarded"))),
        ],
    );
    b.block(stmts, body)
}

pub(super) fn arb_replay_check_helper(b: &ExprBuilder) -> Expr {
    let bad = || {
        b.app(
            rt(b, "Schema.Defect"),
            str_lit(b, "SyntaxError: Invalid Arbitrary replay token"),
        )
    };
    let on_parts = b.if_(
        b.bin(
            BinOp::NotEq,
            b.app(b.global("List.Core", "length"), b.loc("parts")),
            b.int(6),
        ),
        bad(),
        replay_valid_parts(b),
    );
    let on_parsed = b.match_(
        b.app(rt(b, "Schema.ListFromWire"), b.loc("parsed")),
        [arm(b.some_pat("parts"), on_parts), arm(Pat::Wild, bad())],
    );
    let body = b.match_(
        b.app(rt(b, "Json.Parse"), b.loc("replay")),
        [arm(b.ok_pat("parsed"), on_parsed), arm(Pat::Wild, bad())],
    );
    b.lam(&["arb", "replay", "prop"], body)
}
