//! Central `Schema.$Kind`-to-graph dispatcher (`arb_add_schema`).

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{con_app, rt, str_lit};
use super::super::{Arm, Expr, Pat};
use super::schema_arms::{
    adt_arm, chunk_arm, lazy_arm, list_arm, opt_arm, rec_arm, res_arm, tuple_arm,
};
use super::{arb_err, js_int_fn, node_con, push_node, push_simple, raw_entry, wire_finite_fn};

fn redacted_arm(b: &ExprBuilder) -> Expr {
    let add_it = b.app4(
        rt(b, "Helper.arb_add_schema"),
        b.loc("ctx"),
        b.loc("it"),
        b.bool_(false),
        b.none(),
    );
    let wrap = b.lam(
        &["v"],
        con_app(b, "Redacted", "Redacted", "$Redacted", vec![b.loc("v")]),
    );
    let map_node = push_simple(
        b,
        b.list([b.loc("iid")]),
        node_con(b, "$Map", vec![b.loc("iid"), wrap]),
    );
    b.if_(
        b.loc("wire"),
        b.app2(
            rt(b, "Helper.arb_build_link"),
            b.loc("ctx"),
            str_lit(b, "json"),
        ),
        b.block([b.let_("iid", add_it)], map_node),
    )
}

fn refined_arm(b: &ExprBuilder) -> Expr {
    let add_base = b.app4(
        rt(b, "Helper.arb_add_schema"),
        b.loc("ctx"),
        b.loc("bs"),
        b.loc("wire"),
        b.loc("c_opt"),
    );
    let entry = raw_entry(
        b,
        b.list([b.loc("bid")]),
        node_con(b, "$Map", vec![b.loc("bid"), b.lam(&["v"], b.loc("v"))]),
        b.list([b.loc("rchk")]),
        b.none(),
        false,
    );
    b.block([b.let_("bid", add_base)], push_node(b, b.loc("ctx"), entry))
}

fn enum_arm(b: &ExprBuilder) -> Expr {
    let mids = b.gen_strict(b.each(
        "v",
        b.loc("vals"),
        b.yield_(push_simple(
            b,
            b.list(vec![]),
            node_con(b, "$Literal", vec![b.loc("v")]),
        )),
    ));
    b.block(
        [b.let_("mids", mids)],
        push_simple(b, b.loc("mids"), node_con(b, "$Union", vec![b.loc("mids")])),
    )
}

fn kind_pat(b: &ExprBuilder, c: &str, names: &[&str]) -> Pat {
    let args = names
        .iter()
        .map(|n| {
            if *n == "_" {
                Pat::Wild
            } else {
                Pat::Bind(b.name(n))
            }
        })
        .collect();
    Pat::Con(ExprBuilder::con("Schema", "$Kind", c, names.len()), args)
}

fn scalar_kind_arms(b: &ExprBuilder) -> [Arm; 8] {
    let num_entry = raw_entry(
        b,
        b.list(vec![]),
        b.app(rt(b, "Helper.arb_build_number"), b.loc("c_opt")),
        b.if_(b.loc("wire"), b.list([wire_finite_fn(b)]), b.list(vec![])),
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
    [
        arm(
            kind_pat(b, "$Number", &[]),
            push_node(b, b.loc("ctx"), num_entry),
        ),
        arm(
            kind_pat(b, "$Int", &[]),
            push_node(b, b.loc("ctx"), int_entry),
        ),
        arm(
            kind_pat(b, "$String", &[]),
            push_simple(
                b,
                b.list(vec![]),
                b.app(rt(b, "Helper.arb_build_string"), b.loc("c_opt")),
            ),
        ),
        arm(
            kind_pat(b, "$Bool", &[]),
            push_simple(b, b.list(vec![]), node_con(b, "$Bool", vec![])),
        ),
        arm(
            kind_pat(b, "$BigInt", &[]),
            push_simple(
                b,
                b.list(vec![]),
                b.app(rt(b, "Helper.arb_build_bigint"), b.loc("c_opt")),
            ),
        ),
        arm(
            kind_pat(b, "$Unit", &[]),
            push_simple(b, b.list(vec![]), node_con(b, "$Literal", vec![b.unit()])),
        ),
        arm(
            kind_pat(b, "$Never", &[]),
            push_simple(b, b.list(vec![]), node_con(b, "$Never", vec![])),
        ),
        arm(
            kind_pat(b, "$Unknown", &[]),
            b.app2(
                rt(b, "Helper.arb_build_link"),
                b.loc("ctx"),
                str_lit(b, "json"),
            ),
        ),
    ]
}

pub(super) fn add_schema_helper(b: &ExprBuilder) -> Expr {
    let mut arms = Vec::from(scalar_kind_arms(b));
    arms.extend([
        arm(kind_pat(b, "$List", &["it"]), list_arm(b)),
        arm(kind_pat(b, "$Tuple", &["its"]), tuple_arm(b)),
        arm(kind_pat(b, "$Record", &["flds"]), rec_arm(b)),
        arm(kind_pat(b, "$Adt", &["_", "cases"]), adt_arm(b)),
        arm(kind_pat(b, "$Option", &["it"]), opt_arm(b)),
        arm(kind_pat(b, "$Result", &["es", "os"]), res_arm(b)),
        arm(kind_pat(b, "$Redacted", &["it"]), redacted_arm(b)),
        arm(kind_pat(b, "$Lazy", &["lname", "lth"]), lazy_arm(b)),
        arm(
            kind_pat(b, "$Refined", &["_", "bs", "rchk"]),
            refined_arm(b),
        ),
        arm(kind_pat(b, "$Enum", &["vals"]), enum_arm(b)),
        arm(
            kind_pat(b, "$Duration", &[]),
            b.app2(
                rt(b, "Helper.arb_build_link"),
                b.loc("ctx"),
                str_lit(b, "duration_nonneg"),
            ),
        ),
        arm(
            kind_pat(b, "$BigDecimal", &[]),
            push_simple(b, b.list(vec![]), node_con(b, "$Decimal", vec![])),
        ),
        arm(kind_pat(b, "$Chunk", &["it"]), chunk_arm(b)),
        arm(
            kind_pat(b, "$CodecType", &["c"]),
            b.app3(
                rt(b, "Helper.arb_add_codec"),
                b.loc("ctx"),
                b.loc("c"),
                b.loc("c_opt"),
            ),
        ),
        arm(
            Pat::Wild,
            arb_err(b, str_lit(b, "an unsupported Declaration")),
        ),
    ]);
    b.lam(
        &["ctx", "sch", "wire", "c_opt"],
        b.match_(b.app(rt(b, "Helper.schema_kind"), b.loc("sch")), arms),
    )
}
