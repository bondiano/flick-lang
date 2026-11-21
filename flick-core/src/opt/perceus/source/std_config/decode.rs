//! Recursive schema-driven config tree decoding (`cfg_decode_at`, `cfg_decode_array`).

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{pair_pat, rt, str_lit};
use super::super::{Expr, Pat};
use super::{fail_cfg, load_node, node_value_opt, push_seg, schema_err_expr};

fn decode_record_arm(b: &ExprBuilder) -> Expr {
    let nc = |c: &str, n: usize| ExprBuilder::con("ConfigProvider", "Node", c, n);
    let dec_field = b.match_(
        b.app3(
            rt(b, "Helper.cfg_decode_at"),
            b.loc("p"),
            b.loc("fs"),
            push_seg(b, b.loc("path"), b.loc("k")),
        ),
        [
            arm(b.ok_pat("v"), b.yield_(b.tuple([b.loc("k"), b.loc("v")]))),
            arm(b.err_pat("e"), fail_cfg(b, b.loc("e"))),
        ],
    );
    let step = b.match_(b.loc("fld"), [arm(pair_pat(b, "k", "fs"), dec_field)]);
    let build_rec = b.block(
        [b.let_("ents", b.gen_strict(b.each("fld", b.loc("flds"), step)))],
        b.ok(b.app(rt(b, "Schema.RecordFromSortedEntries"), b.loc("ents"))),
    );
    let expected_obj = b.err(schema_err_expr(
        b,
        b.loc("path"),
        str_lit(b, "Expected object"),
    ));
    let check_node = b.match_(
        b.loc("n"),
        [
            arm(
                Pat::Con(nc("RecordNode", 2), vec![Pat::Wild, Pat::Wild]),
                build_rec,
            ),
            arm(Pat::Wild, expected_obj.clone()),
        ],
    );
    b.match_(
        b.loc("nd"),
        [
            arm(b.some_pat("n"), check_node),
            arm(Pat::Wild, expected_obj),
        ],
    )
}

fn decode_list_arm(b: &ExprBuilder) -> Expr {
    let nc = |c: &str, n: usize| ExprBuilder::con("ConfigProvider", "Node", c, n);
    let expected_arr = b.err(schema_err_expr(
        b,
        b.loc("path"),
        str_lit(b, "Expected array"),
    ));
    let dec_arr = b.app4(
        rt(b, "Helper.cfg_decode_array"),
        b.loc("p"),
        b.loc("it"),
        b.loc("path"),
        b.loc("len"),
    );
    let check_node = b.match_(
        b.loc("n"),
        [
            arm(
                Pat::Con(
                    nc("ArrayNode", 2),
                    vec![Pat::Bind(b.name("len")), Pat::Wild],
                ),
                dec_arr,
            ),
            arm(Pat::Wild, expected_arr.clone()),
        ],
    );
    b.match_(
        b.loc("nd"),
        [
            arm(b.some_pat("n"), check_node),
            arm(Pat::Wild, expected_arr),
        ],
    )
}

fn decode_scalar_arm(b: &ExprBuilder) -> Expr {
    let dec_txt = b.match_(
        b.app2(rt(b, "Helper.cfg_schema_dec"), b.loc("sch"), b.loc("txt")),
        [
            arm(b.ok_pat("v"), b.ok(b.loc("v"))),
            arm(
                b.err_pat("iss"),
                fail_cfg(b, schema_err_expr(b, b.loc("path"), b.loc("iss"))),
            ),
        ],
    );
    let expected_str = b.err(schema_err_expr(
        b,
        b.loc("path"),
        str_lit(b, "Expected string"),
    ));
    b.match_(
        node_value_opt(b, b.loc("nd")),
        [
            arm(b.some_pat("txt"), dec_txt),
            arm(Pat::Wild, expected_str),
        ],
    )
}

pub(super) fn cfg_decode_at_helper(b: &ExprBuilder) -> Expr {
    let kc = |c: &str, n: usize| ExprBuilder::con("Schema", "$Kind", c, n);
    let dispatch_kind = b.match_(
        b.app(rt(b, "Helper.schema_kind"), b.loc("sch")),
        [
            arm(
                Pat::Con(kc("$Record", 1), vec![Pat::Bind(b.name("flds"))]),
                decode_record_arm(b),
            ),
            arm(
                Pat::Con(kc("$List", 1), vec![Pat::Bind(b.name("it"))]),
                decode_list_arm(b),
            ),
            arm(Pat::Wild, decode_scalar_arm(b)),
        ],
    );
    let eager_arm = b.block(
        [b.let_("nd", load_node(b, b.loc("p"), b.loc("path")))],
        dispatch_kind,
    );
    let lazy_arm = b.app3(
        rt(b, "Helper.cfg_decode_at"),
        b.loc("p"),
        b.app(b.loc("th"), b.unit()),
        b.loc("path"),
    );
    b.lam(
        &["p", "sch", "path"],
        b.match_(
            b.app(rt(b, "Helper.schema_kind"), b.loc("sch")),
            [
                arm(
                    Pat::Con(kc("$Lazy", 2), vec![Pat::Wild, Pat::Bind(b.name("th"))]),
                    lazy_arm,
                ),
                arm(Pat::Wild, eager_arm),
            ],
        ),
    )
}

pub(super) fn cfg_decode_array_helper(b: &ExprBuilder) -> Expr {
    let ok_step = b.block(
        [b.assign("i", b.add_i(b.var("i"), b.int(1)))],
        b.yield_(b.loc("v")),
    );
    let dec_item = b.match_(
        b.app3(
            rt(b, "Helper.cfg_decode_at"),
            b.loc("p"),
            b.loc("item"),
            push_seg(b, b.loc("path"), b.var("i")),
        ),
        [
            arm(b.ok_pat("v"), ok_step),
            arm(b.err_pat("e"), fail_cfg(b, b.loc("e"))),
        ],
    );
    let loop_expr = b.while_(b.bin(BinOp::Lt, b.var("i"), b.loc("len")), dec_item);
    b.lam(
        &["p", "item", "path", "len"],
        b.block(
            [
                b.var_decl("i", b.int(0)),
                b.let_("out", b.gen_strict(loop_expr)),
            ],
            b.ok(b.loc("out")),
        ),
    )
}
