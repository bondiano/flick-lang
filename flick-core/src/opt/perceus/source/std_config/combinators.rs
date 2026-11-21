//! Composite `Config` combinators (`orElse`, `all`, `array`, `record`).

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{rt, str_lit};
use super::super::{Expr, Pat, Stmt};
use super::{fail_cfg, load_node, node_value_opt, push_seg, schema_err_expr};

pub(super) fn cfg_or_else(b: &ExprBuilder) -> Expr {
    let on_res = b.match_(
        b.loc("res"),
        [
            arm(b.ok_pat("v"), b.ok(b.loc("v"))),
            arm(
                b.err_pat("e"),
                b.app2(b.app(b.loc("f"), b.loc("e")), b.loc("p"), b.loc("pref")),
            ),
        ],
    );
    let pure_fail = b.bin(
        BinOp::And,
        b.app(b.global("Cause", "has_fails"), b.loc("c")),
        b.bin(
            BinOp::And,
            b.not(b.app(b.global("Cause", "has_dies"), b.loc("c"))),
            b.not(b.app(b.global("Cause", "has_interrupts"), b.loc("c"))),
        ),
    );
    let rethrow = b.app(b.global("Failure", "rethrow"), b.loc("c"));
    let recover_err = b.match_(
        b.app(b.global("Cause", "find_error_option"), b.loc("c")),
        [
            arm(
                b.some_pat("err"),
                b.app2(b.app(b.loc("f"), b.loc("err")), b.loc("p"), b.loc("pref")),
            ),
            arm(Pat::Wild, rethrow.clone()),
        ],
    );
    let on_fail = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("ex")),
        [arm(b.some_pat("c"), b.if_(pure_fail, recover_err, rethrow))],
    );
    let run_exit = b.app(
        b.global("Effect.Core", "exit"),
        b.task(b.app2(b.loc("self"), b.loc("p"), b.loc("pref"))),
    );
    b.lam(
        &["f", "self", "p", "pref"],
        b.block(
            [b.let_("ex", run_exit)],
            b.match_(
                b.app(b.global("Exit", "value"), b.loc("ex")),
                [arm(b.some_pat("res"), on_res), arm(Pat::Wild, on_fail)],
            ),
        ),
    )
}

pub(super) fn cfg_all(b: &ExprBuilder) -> Expr {
    let record_miss = b.match_(
        b.var("first_miss"),
        [
            arm(b.some_pat("_"), b.unit()),
            arm(Pat::Wild, b.assign_e("first_miss", b.some(b.loc("e")))),
        ],
    );
    let step = b.match_(
        b.app2(b.loc("c"), b.loc("p"), b.loc("pref")),
        [
            arm(b.ok_pat("v"), b.yield_(b.loc("v"))),
            arm(
                b.err_pat("e"),
                b.block([Stmt::Expr(record_miss)], b.yield_(b.unit())),
            ),
        ],
    );
    let finish = b.match_(
        b.var("first_miss"),
        [
            arm(b.some_pat("e"), b.err(b.loc("e"))),
            arm(Pat::Wild, b.ok(b.loc("out"))),
        ],
    );
    b.lam(
        &["cfgs", "p", "pref"],
        b.block(
            [
                b.var_decl("first_miss", b.none()),
                b.let_("out", b.gen_strict(b.each("c", b.loc("cfgs"), step))),
            ],
            finish,
        ),
    )
}

fn opt_or(b: &ExprBuilder, opt: Expr, fallback: &str) -> Expr {
    b.match_(
        opt,
        [
            arm(b.some_pat("s"), b.loc("s")),
            arm(Pat::Wild, str_lit(b, fallback)),
        ],
    )
}

pub(super) fn cfg_array(b: &ExprBuilder) -> Expr {
    let nc = |c: &str, n: usize| ExprBuilder::con("ConfigProvider", "Node", c, n);
    let dec_ok = b.block(
        [b.assign("idx", b.add_i(b.var("idx"), b.int(1)))],
        b.yield_(b.loc("v")),
    );
    let dec_err = fail_cfg(
        b,
        schema_err_expr(b, push_seg(b, b.loc("path"), b.var("idx")), b.loc("iss")),
    );
    let dec_part = b.match_(
        b.app2(rt(b, "Helper.cfg_schema_dec"), b.loc("item"), b.loc("part")),
        [arm(b.ok_pat("v"), dec_ok), arm(b.err_pat("iss"), dec_err)],
    );
    let split_decode = b.block(
        [
            b.let_(
                "parts",
                b.app2(b.global("String", "split"), b.loc("sep"), b.loc("txt")),
            ),
            b.var_decl("idx", b.int(0)),
            b.let_(
                "out",
                b.gen_strict(b.each("part", b.loc("parts"), dec_part)),
            ),
        ],
        b.ok(b.loc("out")),
    );
    let miss_err = b.err(schema_err_expr(
        b,
        b.loc("path"),
        str_lit(b, "Expected string | array"),
    ));
    let decode_array = b.app4(
        rt(b, "Helper.cfg_decode_array"),
        b.loc("p"),
        b.loc("item"),
        b.loc("path"),
        b.loc("len"),
    );
    let check_arr_node = b.match_(
        b.loc("n"),
        [
            arm(
                Pat::Con(
                    nc("ArrayNode", 2),
                    vec![Pat::Bind(b.name("len")), Pat::Wild],
                ),
                decode_array,
            ),
            arm(Pat::Wild, miss_err.clone()),
        ],
    );
    let fallback_node = b.match_(
        b.loc("nd"),
        [
            arm(b.some_pat("n"), check_arr_node),
            arm(Pat::Wild, miss_err),
        ],
    );
    let body = b.match_(
        node_value_opt(b, b.loc("nd")),
        [
            arm(b.some_pat("txt"), split_decode),
            arm(Pat::Wild, fallback_node),
        ],
    );
    b.lam(
        &["opts", "item", "name", "p", "pref"],
        b.block(
            [
                b.let_("sep", opt_or(b, b.field(b.loc("opts"), "separator"), ",")),
                b.let_("path", push_seg(b, b.loc("pref"), b.loc("name"))),
                b.let_("nd", load_node(b, b.loc("p"), b.loc("path"))),
            ],
            body,
        ),
    )
}

fn cfg_record_from_str(b: &ExprBuilder) -> Expr {
    let k_slice = b.app3(
        b.global("String", "slice"),
        b.int(0),
        b.loc("pos"),
        b.loc("pair"),
    );
    let rest_start = b.add_i(
        b.loc("pos"),
        b.app(b.global("String", "length"), b.loc("kv")),
    );
    let rest_slice = b.app3(
        b.global("String", "slice"),
        rest_start,
        b.app(b.global("String", "length"), b.loc("pair")),
        b.loc("pair"),
    );
    let dec_err = fail_cfg(
        b,
        schema_err_expr(b, push_seg(b, b.loc("path"), b.loc("k")), b.loc("iss")),
    );
    let dec_val = b.match_(
        b.app2(rt(b, "Helper.cfg_schema_dec"), b.loc("item"), b.loc("rest")),
        [
            arm(b.ok_pat("v"), b.yield_(b.tuple([b.loc("k"), b.loc("v")]))),
            arm(b.err_pat("iss"), dec_err),
        ],
    );
    let on_kv = b.block([b.let_("k", k_slice), b.let_("rest", rest_slice)], dec_val);
    let bad_pair = fail_cfg(
        b,
        schema_err_expr(b, b.loc("path"), str_lit(b, "Expected a key-value pair")),
    );
    let step = b.match_(
        b.app2(b.global("String", "index_of"), b.loc("kv"), b.loc("pair")),
        [arm(b.some_pat("pos"), on_kv), arm(Pat::Wild, bad_pair)],
    );
    let pairs = b.app2(b.global("String", "split"), b.loc("sep"), b.loc("txt"));
    b.block(
        [b.let_("pairs", pairs)],
        b.ok(b.gen_strict(b.each("pair", b.loc("pairs"), step))),
    )
}

pub(super) fn cfg_record(b: &ExprBuilder) -> Expr {
    let nc = |c: &str, n: usize| ExprBuilder::con("ConfigProvider", "Node", c, n);
    let dec_key = b.match_(
        b.app3(
            rt(b, "Helper.cfg_decode_at"),
            b.loc("p"),
            b.loc("item"),
            push_seg(b, b.loc("path"), b.loc("k")),
        ),
        [
            arm(b.ok_pat("v"), b.yield_(b.tuple([b.loc("k"), b.loc("v")]))),
            arm(b.err_pat("e"), fail_cfg(b, b.loc("e"))),
        ],
    );
    let from_keys = b.ok(b.gen_strict(b.each("k", b.loc("ks"), dec_key)));
    let miss_err = b.err(schema_err_expr(
        b,
        b.loc("path"),
        str_lit(b, "Expected object | string"),
    ));
    let try_str_or_miss = b.match_(
        node_value_opt(b, b.loc("nd")),
        [
            arm(b.some_pat("txt"), cfg_record_from_str(b)),
            arm(Pat::Wild, miss_err),
        ],
    );
    let check_node = b.match_(
        b.loc("n"),
        [
            arm(
                Pat::Con(
                    nc("RecordNode", 2),
                    vec![Pat::Bind(b.name("ks")), Pat::Wild],
                ),
                from_keys,
            ),
            arm(Pat::Wild, try_str_or_miss.clone()),
        ],
    );
    let body = b.match_(
        b.loc("nd"),
        [
            arm(b.some_pat("n"), check_node),
            arm(Pat::Wild, try_str_or_miss),
        ],
    );
    b.lam(
        &["opts", "item", "name", "p", "pref"],
        b.block(
            [
                b.let_("sep", opt_or(b, b.field(b.loc("opts"), "separator"), ",")),
                b.let_(
                    "kv",
                    opt_or(b, b.field(b.loc("opts"), "keyValueSeparator"), "="),
                ),
                b.let_("path", push_seg(b, b.loc("pref"), b.loc("name"))),
                b.let_("nd", load_node(b, b.loc("p"), b.loc("path"))),
            ],
            body,
        ),
    )
}
