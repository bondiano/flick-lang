//! Collection `$Node` evaluation arms (`$Arrays`, `$Record`).

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt};
use super::super::{Expr, Pat, Stmt};
use super::{add_f, div_f, mul_f, sub_f};

fn arrays_poss_expr(b: &ExprBuilder) -> Expr {
    let no_rid = b.match_(
        b.loc("rid_o"),
        [
            arm(b.some_pat("_"), b.bool_(false)),
            arm(Pat::Wild, b.bool_(true)),
        ],
    );
    let empty_cond = b.bin(
        BinOp::Or,
        b.bin(
            BinOp::Gt,
            add_f(b, b.loc("req"), b.loc("min_rep")),
            b.loc("up"),
        ),
        b.bin(
            BinOp::And,
            no_rid,
            b.bin(BinOp::Gt, b.loc("min_rep"), b.float(0.0)),
        ),
    );
    let cost = b.if_(
        b.bin(BinOp::Gt, b.loc("r"), b.float(0.0)),
        add_f(b, b.var("req_c"), mul_f(b, b.loc("r"), b.loc("head_mc"))),
        b.var("req_c"),
    );
    let step = b.block(
        [
            b.let_("r", b.var("r_cur")),
            b.assign("r_cur", add_f(b, b.var("r_cur"), b.float(1.0))),
            b.let_("cost", cost),
        ],
        b.if_(
            b.bin(BinOp::Le, b.loc("cost"), b.loc("bd")),
            b.yield_(b.loc("r")),
            b.unit(),
        ),
    );
    let gen_poss = b.gen_strict(b.while_(b.bin(BinOp::Le, b.var("r_cur"), b.loc("max_rep")), step));
    b.if_(empty_cond, b.list(vec![]), gen_poss)
}

fn arrays_sample_with_rid(b: &ExprBuilder) -> Expr {
    let rep_step = b.block(
        [b.assign("ri", b.add_i(b.var("ri"), b.int(1)))],
        b.yield_(b.loc("hid")),
    );
    let reps = b.gen_strict(b.while_(b.bin(BinOp::Lt, b.var("ri"), b.loc("rep_i")), rep_step));
    let shape = b.tuple([
        b.loc("req_i"),
        b.int(0),
        b.loc("rep_i"),
        b.int(0),
        b.app(b.global("Int", "truncate"), b.loc("amin")),
    ]);
    let on_sams = b.some(b.app3(
        rt(b, "Helper.arb_array_sample"),
        b.loc("sams"),
        shape,
        b.loc("shk"),
    ));
    let gen_full = b.block(
        [
            b.var_decl("ri", b.int(0)),
            b.let_("reps", reps),
            b.let_(
                "sel",
                b.app2(
                    b.global("List.Core", "append"),
                    b.loc("eids"),
                    b.loc("reps"),
                ),
            ),
        ],
        b.match_(
            b.app3(
                rt(b, "Helper.arb_gen_samples"),
                b.loc("tbl"),
                b.loc("sel"),
                b.loc("ist"),
            ),
            [arm(b.some_pat("sams"), on_sams), arm(Pat::Wild, b.none())],
        ),
    );
    let fast_cond = b.bin(
        BinOp::And,
        b.not(b.loc("shk")),
        b.bin(BinOp::Eq, b.loc("req_i"), b.int(0)),
    );
    let fast = b.app4(
        rt(b, "Helper.arb_gen_repeated"),
        b.loc("tbl"),
        b.loc("hid"),
        b.loc("rep_i"),
        b.loc("ist"),
    );
    b.if_(fast_cond, fast, gen_full)
}

fn arrays_pick_and_gen(b: &ExprBuilder) -> Expr {
    let pidx = b.app(
        b.global("Int", "truncate"),
        b.app3(
            rt(b, "Helper.arb_st_len"),
            b.loc("st"),
            b.float(0.0),
            b.app(
                b.global("Int", "to_float"),
                b.sub_i(b.loc("plen"), b.int(1)),
            ),
        ),
    );
    let grown_st = b.tuple([
        b.loc("rep_f"),
        b.loc("shk"),
        b.loc("bias"),
        b.loc("rr"),
        b.loc("br"),
    ]);
    let ist = b.if_(
        b.bin(BinOp::Lt, b.loc("sz"), b.loc("rep_f")),
        grown_st,
        b.loc("st"),
    );
    let fixed_shape = b.tuple([
        b.loc("req_i"),
        b.int(0),
        b.int(0),
        b.int(0),
        b.app(b.global("Int", "truncate"), b.loc("amin")),
    ]);
    let fixed_sams = b.match_(
        b.app3(
            rt(b, "Helper.arb_gen_samples"),
            b.loc("tbl"),
            b.loc("eids"),
            b.loc("ist"),
        ),
        [
            arm(
                b.some_pat("sams"),
                b.some(b.app3(
                    rt(b, "Helper.arb_array_sample"),
                    b.loc("sams"),
                    fixed_shape,
                    b.loc("shk"),
                )),
            ),
            arm(Pat::Wild, b.none()),
        ],
    );
    let raw_s = b.match_(
        b.loc("rid_o"),
        [
            arm(b.some_pat("hid"), arrays_sample_with_rid(b)),
            arm(Pat::Wild, fixed_sams),
        ],
    );
    let to_tuple = b.match_(
        b.loc("raw_s"),
        [
            arm(
                b.some_pat("s"),
                b.some(b.app2(
                    rt(b, "Helper.arb_map_sample"),
                    b.loc("s"),
                    rt(b, "Schema.TupleFromList"),
                )),
            ),
            arm(Pat::Wild, b.none()),
        ],
    );
    let stmts = [
        b.let_("pidx", pidx),
        b.let_(
            "rep_f",
            b.app2(rt(b, "Helper.list_at"), b.loc("pidx"), b.loc("poss")),
        ),
        b.let_("rep_i", b.app(b.global("Int", "truncate"), b.loc("rep_f"))),
        b.let_("ist", ist),
        b.let_("raw_s", raw_s),
    ];
    b.block(stmts, b.if_(b.loc("is_tup"), to_tuple, b.loc("raw_s")))
}

fn arrays_max_rep(b: &ExprBuilder) -> Expr {
    let diff = sub_f(b, b.loc("up"), b.loc("req"));
    b.match_(
        b.loc("rid_o"),
        [
            arm(
                b.some_pat("_"),
                b.if_(
                    b.bin(BinOp::Gt, diff.clone(), b.loc("min_rep")),
                    diff,
                    b.loc("min_rep"),
                ),
            ),
            arm(Pat::Wild, b.float(0.0)),
        ],
    )
}

fn arrays_head_mc(b: &ExprBuilder) -> Expr {
    b.match_(
        b.loc("rid_o"),
        [
            arm(
                b.some_pat("hid"),
                b.match_(
                    b.app2(rt(b, "Helper.list_at"), b.loc("hid"), b.loc("tbl")),
                    [arm(
                        b.tup_pat(&["mc", "_", "_", "_", "_", "_"]),
                        b.loc("mc"),
                    )],
                ),
            ),
            arm(Pat::Wild, b.float(0.0)),
        ],
    )
}

pub(super) fn arrays_arm(b: &ExprBuilder) -> Expr {
    let sum_req = b.each(
        "eid",
        b.loc("eids"),
        b.match_(
            b.app2(rt(b, "Helper.list_at"), b.loc("eid"), b.loc("tbl")),
            [arm(
                b.tup_pat(&["mc", "_", "_", "_", "_", "_"]),
                b.assign_e("req_c", add_f(b, b.var("req_c"), b.loc("mc"))),
            )],
        ),
    );
    let min_diff = sub_f(b, b.loc("amin"), b.loc("req"));
    let min_rep = b.if_(
        b.bin(BinOp::Gt, min_diff.clone(), b.float(0.0)),
        min_diff,
        b.float(0.0),
    );
    let stmts = [
        b.let_(
            "req_i",
            b.app(b.global("List.Core", "length"), b.loc("eids")),
        ),
        b.let_("req", b.app(b.global("Int", "to_float"), b.loc("req_i"))),
        b.var_decl("req_c", b.float(0.0)),
        Stmt::Expr(sum_req),
        b.let_(
            "m1",
            b.if_(
                b.bin(BinOp::Gt, b.loc("amin"), b.loc("req")),
                b.loc("amin"),
                b.loc("req"),
            ),
        ),
        b.let_(
            "m2",
            b.if_(
                b.bin(BinOp::Gt, b.loc("m1"), b.loc("sz")),
                b.loc("m1"),
                b.loc("sz"),
            ),
        ),
        b.let_(
            "up",
            b.if_(
                b.bin(BinOp::Lt, b.loc("amax"), b.loc("m2")),
                b.loc("amax"),
                b.loc("m2"),
            ),
        ),
        b.let_("min_rep", min_rep),
        b.let_("max_rep", arrays_max_rep(b)),
        b.let_("head_mc", arrays_head_mc(b)),
        b.let_("bd", b.rget(b.loc("br"))),
        b.var_decl("r_cur", b.loc("min_rep")),
        b.let_("poss", arrays_poss_expr(b)),
        b.let_(
            "plen",
            b.app(b.global("List.Core", "length"), b.loc("poss")),
        ),
    ];
    b.block(
        stmts,
        b.if_(
            b.bin(BinOp::Eq, b.loc("plen"), b.int(0)),
            b.none(),
            arrays_pick_and_gen(b),
        ),
    )
}

fn record_key_loop(b: &ExprBuilder) -> Expr {
    let dup_pred = b.lam(
        &["o"],
        b.match_(
            b.loc("o"),
            [arm(
                pair_pat(b, "ov", "_"),
                b.bin(BinOp::Eq, b.loc("ov"), b.loc("kv")),
            )],
        ),
    );
    let is_dup = b.app2(b.global("List.Core", "any"), dup_pred, b.var("ks"));
    let on_retry = b.block(
        [
            b.rset(b.loc("br"), b.loc("sav_bd")),
            b.assign("ret", b.add_i(b.var("ret"), b.int(1))),
        ],
        b.unit(),
    );
    let on_dup = b.if_(
        b.bin(BinOp::Ge, b.var("ret"), b.int(10)),
        b.assign_e("kdone", b.bool_(true)),
        on_retry,
    );
    let on_fresh = b.block(
        [
            b.assign("k_got", b.some(b.loc("ks1"))),
            b.assign("kdone", b.bool_(true)),
        ],
        b.unit(),
    );
    let on_ks1 = b.match_(
        b.loc("ks1"),
        [arm(pair_pat(b, "kv", "_"), b.if_(is_dup, on_dup, on_fresh))],
    );
    let try_key = b.app3(
        rt(b, "Helper.arb_reserving"),
        b.loc("karb"),
        b.loc("kst"),
        add_f(b, b.loc("vmc"), b.loc("fut")),
    );
    b.while_(
        b.not(b.var("kdone")),
        b.match_(
            try_key,
            [
                arm(b.some_pat("ks1"), on_ks1),
                arm(Pat::Wild, b.assign_e("kdone", b.bool_(true))),
            ],
        ),
    )
}

fn record_gen_pair(b: &ExprBuilder) -> Expr {
    let grown_kst = b.tuple([
        b.loc("cnt_f"),
        b.loc("shk"),
        b.loc("bias"),
        b.loc("rr"),
        b.loc("br"),
    ]);
    let kst = b.if_(
        b.bin(BinOp::Lt, b.loc("sz"), b.loc("cnt_f")),
        grown_kst,
        b.loc("st"),
    );
    let on_vf = b.block(
        [
            b.assign(
                "ks",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("ks"),
                    b.list([b.loc("kf")]),
                ),
            ),
            b.assign(
                "vs",
                b.app2(
                    b.global("List.Core", "append"),
                    b.var("vs"),
                    b.list([b.loc("vf")]),
                ),
            ),
            b.assign("pos", b.add_i(b.var("pos"), b.int(1))),
        ],
        b.unit(),
    );
    let match_val = b.match_(
        b.app3(
            rt(b, "Helper.arb_reserving"),
            b.loc("varb"),
            b.loc("st"),
            b.loc("fut"),
        ),
        [
            arm(b.some_pat("vf"), on_vf),
            arm(Pat::Wild, b.assign_e("ok", b.bool_(false))),
        ],
    );
    b.block(
        [
            b.let_("sav_bd", b.rget(b.loc("br"))),
            b.let_("kst", kst),
            b.var_decl("k_got", b.none()),
            b.var_decl("ret", b.int(0)),
            b.var_decl("kdone", b.bool_(false)),
            Stmt::Expr(record_key_loop(b)),
        ],
        b.match_(
            b.var("k_got"),
            [
                arm(b.some_pat("kf"), match_val),
                arm(Pat::Wild, b.assign_e("ok", b.bool_(false))),
            ],
        ),
    )
}

fn record_gen_loop(b: &ExprBuilder) -> Expr {
    let rem_cnt = b.sub_i(b.sub_i(b.loc("cnt"), b.var("pos")), b.int(1));
    let fut = mul_f(
        b,
        b.app(b.global("Int", "to_float"), rem_cnt),
        b.loc("icost"),
    );
    let step = b.block(
        [b.let_("fut", fut)],
        b.if_(
            b.bin(
                BinOp::Gt,
                add_f(b, b.loc("icost"), b.loc("fut")),
                b.rget(b.loc("br")),
            ),
            b.assign_e("ok", b.bool_(false)),
            record_gen_pair(b),
        ),
    );
    let cond = b.bin(
        BinOp::And,
        b.var("ok"),
        b.bin(BinOp::Lt, b.var("pos"), b.loc("cnt")),
    );
    b.block(
        [
            b.var_decl("ks", b.list(vec![])),
            b.var_decl("vs", b.list(vec![])),
            b.var_decl("pos", b.int(0)),
            b.var_decl("ok", b.bool_(true)),
            Stmt::Expr(b.while_(cond, step)),
        ],
        b.if_(
            b.var("ok"),
            b.some(b.app3(
                rt(b, "Helper.arb_entries_sample"),
                b.var("ks"),
                b.var("vs"),
                b.loc("shk"),
            )),
            b.none(),
        ),
    )
}

fn record_gen_entries(b: &ExprBuilder) -> Expr {
    let karb = b.tuple([
        b.loc("kmc"),
        b.lam(
            &["s2"],
            b.app3(
                rt(b, "Helper.arb_eval_node"),
                b.loc("tbl"),
                b.loc("kid"),
                b.loc("s2"),
            ),
        ),
    ]);
    let varb = b.tuple([
        b.loc("vmc"),
        b.lam(
            &["s2"],
            b.app3(
                rt(b, "Helper.arb_eval_node"),
                b.loc("tbl"),
                b.loc("vid"),
                b.loc("s2"),
            ),
        ),
    ]);
    let bad_budget = b.bin(
        BinOp::Or,
        b.bin(
            BinOp::And,
            b.bin(BinOp::Gt, b.loc("cnt"), b.int(0)),
            b.bin(
                BinOp::Gt,
                mul_f(b, b.loc("cnt_f"), b.loc("icost")),
                b.loc("bd"),
            ),
        ),
        b.bin(
            BinOp::And,
            b.bin(BinOp::Eq, b.loc("cnt"), b.int(0)),
            b.bin(BinOp::Lt, b.loc("bd"), b.float(0.0)),
        ),
    );
    b.block(
        [
            b.let_(
                "cnt_f",
                b.app3(
                    rt(b, "Helper.arb_st_len"),
                    b.loc("st"),
                    b.float(0.0),
                    b.loc("aff"),
                ),
            ),
            b.let_("cnt", b.app(b.global("Int", "truncate"), b.loc("cnt_f"))),
            b.let_("karb", karb),
            b.let_("varb", varb),
        ],
        b.if_(bad_budget, b.none(), record_gen_loop(b)),
    )
}

pub(super) fn record_arm(b: &ExprBuilder) -> Expr {
    let inf = div_f(b, b.float(1.0), b.float(0.0));
    let kmc = b.match_(
        b.app2(rt(b, "Helper.list_at"), b.loc("kid"), b.loc("tbl")),
        [arm(b.tup_pat(&["m", "_", "_", "_", "_", "_"]), b.loc("m"))],
    );
    let vmc = b.match_(
        b.app2(rt(b, "Helper.list_at"), b.loc("vid"), b.loc("tbl")),
        [arm(b.tup_pat(&["m", "_", "_", "_", "_", "_"]), b.loc("m"))],
    );
    let fl_bound = b.block(
        [b.let_(
            "fl",
            b.app(
                b.global("Float", "floor"),
                div_f(b, b.loc("bd"), b.loc("icost")),
            ),
        )],
        b.if_(
            b.bin(BinOp::Lt, b.loc("fl"), b.loc("up")),
            b.loc("fl"),
            b.loc("up"),
        ),
    );
    let aff = b.if_(
        b.bin(BinOp::Eq, b.loc("icost"), inf),
        b.float(0.0),
        b.if_(
            b.bin(BinOp::Eq, b.loc("icost"), b.float(0.0)),
            b.loc("up"),
            fl_bound,
        ),
    );
    let stmts = [
        b.let_(
            "up",
            b.if_(
                b.bin(BinOp::Gt, b.loc("sz"), b.float(0.0)),
                b.loc("sz"),
                b.float(0.0),
            ),
        ),
        Stmt::Expr(b.app3(
            rt(b, "Helper.arb_st_int"),
            b.loc("st"),
            b.float(0.0),
            b.float(0.0),
        )),
        b.let_("kmc", kmc),
        b.let_("vmc", vmc),
        b.let_("icost", add_f(b, b.loc("kmc"), b.loc("vmc"))),
        b.let_("bd", b.rget(b.loc("br"))),
        b.let_("aff", aff),
    ];
    b.block(
        stmts,
        b.if_(
            b.bin(BinOp::Lt, b.loc("aff"), b.float(0.0)),
            b.none(),
            record_gen_entries(b),
        ),
    )
}
