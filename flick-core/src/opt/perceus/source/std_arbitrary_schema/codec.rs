//! Codec metadata and extra-tag compiler (`arb_add_codec`) for `Arbitrary`.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{con_app, rt, str_lit};
use super::super::{Expr, Pat, Stmt};
use super::{
    arb_err, float_inf, js_int_fn, node_con, push_node, push_simple, raw_entry, wire_finite_fn,
};

fn by_meta_expr(b: &ExprBuilder) -> Expr {
    let mc = |c: &str, n: usize| ExprBuilder::con("Codec", "$Meta", c, n);
    let num_entry = raw_entry(
        b,
        b.list(vec![]),
        b.app(rt(b, "Helper.arb_build_number"), b.loc("c_opt")),
        b.list([wire_finite_fn(b)]),
        b.none(),
        false,
    );
    let int_entry = raw_entry(
        b,
        b.list(vec![]),
        b.app(rt(b, "Helper.arb_build_number"), b.loc("c_opt")),
        b.list([js_int_fn(b)]),
        b.some(b.global("Int", "truncate")),
        false,
    );
    let enum_mids = b.gen_strict(b.each(
        "v",
        b.loc("vs"),
        b.yield_(push_simple(
            b,
            b.list(vec![]),
            node_con(b, "$Literal", vec![b.loc("v")]),
        )),
    ));
    let enum_body = b.block(
        [b.let_("mids", enum_mids)],
        push_simple(b, b.loc("mids"), node_con(b, "$Union", vec![b.loc("mids")])),
    );
    let arms = [
        arm(
            Pat::Con(mc("$Number", 0), vec![]),
            push_node(b, b.loc("ctx"), num_entry),
        ),
        arm(
            Pat::Con(mc("$Int", 0), vec![]),
            push_node(b, b.loc("ctx"), int_entry),
        ),
        arm(
            Pat::Con(mc("$String", 0), vec![]),
            push_simple(
                b,
                b.list(vec![]),
                b.app(rt(b, "Helper.arb_build_string"), b.loc("c_opt")),
            ),
        ),
        arm(
            Pat::Con(mc("$Bool", 0), vec![]),
            push_simple(b, b.list(vec![]), node_con(b, "$Bool", vec![])),
        ),
        arm(
            Pat::Con(mc("$BigInt", 0), vec![]),
            push_simple(
                b,
                b.list(vec![]),
                b.app(rt(b, "Helper.arb_build_bigint"), b.loc("c_opt")),
            ),
        ),
        arm(
            Pat::Con(mc("$Json", 0), vec![]),
            b.app2(
                rt(b, "Helper.arb_build_link"),
                b.loc("ctx"),
                str_lit(b, "json"),
            ),
        ),
        arm(
            Pat::Con(mc("$Literal", 1), vec![Pat::Bind(b.name("lv"))]),
            push_simple(
                b,
                b.list(vec![]),
                node_con(b, "$Literal", vec![b.loc("lv")]),
            ),
        ),
        arm(
            Pat::Con(mc("$Enum", 1), vec![Pat::Bind(b.name("vs"))]),
            enum_body,
        ),
        arm(
            Pat::Wild,
            arb_err(b, str_lit(b, "an unsupported Declaration")),
        ),
    ];
    b.match_(b.app(rt(b, "Helper.codec_meta"), b.loc("codec")), arms)
}

fn checked_arm(b: &ExprBuilder) -> Expr {
    let merge_mc = b.assign_e(
        "mc",
        b.some(b.app2(rt(b, "Arbitrary.MergeConstraint"), b.var("mc"), b.loc("nc"))),
    );
    let update_mc = b.match_(
        b.app(rt(b, "Arbitrary.ExtractConstraint"), b.loc("sp_o")),
        [arm(b.some_pat("nc"), merge_mc), arm(Pat::Wild, b.unit())],
    );
    let pred_fn = b.lam(
        &["v"],
        b.match_(
            b.app(b.loc("rf"), b.loc("v")),
            [
                arm(b.some_pat("_"), b.bool_(false)),
                arm(Pat::Wild, b.bool_(true)),
            ],
        ),
    );
    let atom_pat = Pat::Con(
        ExprBuilder::con("Check", "$Atom", "$Atom", 3),
        vec![
            Pat::Bind(b.name("rf")),
            Pat::Wild,
            Pat::Bind(b.name("sp_o")),
        ],
    );
    let preds = b.gen_strict(b.each(
        "at",
        b.loc("atoms"),
        b.match_(
            b.loc("at"),
            [arm(
                atom_pat,
                b.block([Stmt::Expr(update_mc)], b.yield_(pred_fn)),
            )],
        ),
    ));
    let bid = b.app3(
        rt(b, "Helper.arb_add_codec"),
        b.loc("ctx"),
        b.loc("base_c"),
        b.var("mc"),
    );
    let entry = raw_entry(
        b,
        b.list([b.loc("bid")]),
        node_con(b, "$Map", vec![b.loc("bid"), b.lam(&["v"], b.loc("v"))]),
        b.loc("preds"),
        b.none(),
        false,
    );
    b.block(
        [
            b.var_decl("mc", b.loc("c_opt")),
            b.let_("preds", preds),
            b.let_("bid", bid),
        ],
        push_node(b, b.loc("ctx"), entry),
    )
}

fn chunk_codec_arm(b: &ExprBuilder) -> Expr {
    let add_ic = b.app3(
        rt(b, "Helper.arb_add_codec"),
        b.loc("ctx"),
        b.loc("ic"),
        b.none(),
    );
    let arr_spec = node_con(
        b,
        "$Arrays",
        vec![
            b.list(vec![]),
            b.some(b.loc("iid")),
            b.bool_(false),
            b.float(0.0),
            float_inf(b),
        ],
    );
    let wrap_chunk = b.lam(
        &["xs"],
        con_app(b, "Chunk.Core", "Chunk", "$Chunk", vec![b.loc("xs")]),
    );
    let map_spec = node_con(b, "$Map", vec![b.loc("lid"), wrap_chunk]);
    b.block(
        [
            b.let_("iid", add_ic),
            b.let_("lid", push_simple(b, b.list([b.loc("iid")]), arr_spec)),
        ],
        push_simple(b, b.list([b.loc("lid")]), map_spec),
    )
}

pub(super) fn add_codec_helper(b: &ExprBuilder) -> Expr {
    let ec = |c: &str, n: usize| ExprBuilder::con("Codec", "$Extra", c, n);
    let arms = [
        arm(
            Pat::Con(
                ec("$Checked", 2),
                vec![Pat::Bind(b.name("base_c")), Pat::Bind(b.name("atoms"))],
            ),
            checked_arm(b),
        ),
        arm(
            Pat::Con(ec("$Link", 1), vec![Pat::Bind(b.name("tag"))]),
            b.app2(rt(b, "Helper.arb_build_link"), b.loc("ctx"), b.loc("tag")),
        ),
        arm(
            Pat::Con(ec("$ChunkCodec", 1), vec![Pat::Bind(b.name("ic"))]),
            chunk_codec_arm(b),
        ),
        arm(Pat::Wild, by_meta_expr(b)),
    ];
    b.lam(
        &["ctx", "codec", "c_opt"],
        b.match_(b.app(rt(b, "Helper.codec_extra"), b.loc("codec")), arms),
    )
}
