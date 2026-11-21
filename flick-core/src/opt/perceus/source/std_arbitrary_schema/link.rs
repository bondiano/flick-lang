//! Linked type node builders (`Duration`, `DateTime`, `Zone`, `Json`) for `Arbitrary`.

use flick_ir::BinOp;

use super::super::Expr;
use super::super::expr_builder::ExprBuilder;
use super::super::std_schema_rt::{rt, str_lit};
use super::{float_inf, node_con, push_node, push_simple, raw_entry, set_node};

fn int_between(b: &ExprBuilder, lo: f64, hi: f64) -> Expr {
    push_simple(
        b,
        b.list(vec![]),
        node_con(
            b,
            "$Number",
            vec![
                b.some(b.float(lo)),
                b.some(b.float(hi)),
                str_lit(b, "integer"),
            ],
        ),
    )
}

fn named_zones(b: &ExprBuilder) -> Expr {
    let names = [
        "UTC",
        "Europe/London",
        "America/New_York",
        "Asia/Tokyo",
        "Australia/Sydney",
    ];
    let mut stmts = Vec::new();
    let mut ids = Vec::new();
    for (i, nm) in names.iter().enumerate() {
        let vn = format!("zn{i}");
        stmts.push(b.let_(
            &vn,
            push_simple(
                b,
                b.list(vec![]),
                node_con(b, "$Literal", vec![str_lit(b, nm)]),
            ),
        ));
        ids.push(b.loc(&vn));
    }
    b.block(
        stmts,
        push_simple(
            b,
            b.list(ids.clone()),
            node_con(b, "$Union", vec![b.list(ids)]),
        ),
    )
}

fn zones_union(b: &ExprBuilder) -> Expr {
    let ids = b.list([b.loc("off_id"), b.loc("nm_id")]);
    b.block(
        [
            b.let_("off_id", int_between(b, -43_200_000.0, 50_400_000.0)),
            b.let_("nm_id", named_zones(b)),
        ],
        push_simple(b, ids.clone(), node_con(b, "$Union", vec![ids])),
    )
}

fn dur_obj_node(b: &ExprBuilder, child: &str, kind: &str) -> Expr {
    let mk = b.lam(
        &["vs"],
        b.app2(
            rt(b, "Arbitrary.MakeDuration"),
            str_lit(b, kind),
            b.app2(rt(b, "Helper.list_at"), b.int(0), b.loc("vs")),
        ),
    );
    push_simple(
        b,
        b.list([b.loc(child)]),
        node_con(b, "$Objects", vec![b.list([b.loc(child)]), mk]),
    )
}

fn dur_base(b: &ExprBuilder) -> Expr {
    let d0_spec = node_con(
        b,
        "$Literal",
        vec![b.app2(
            rt(b, "Arbitrary.MakeDuration"),
            str_lit(b, "Infinity"),
            b.unit(),
        )],
    );
    let d1_spec = node_con(
        b,
        "$Literal",
        vec![b.app2(
            rt(b, "Arbitrary.MakeDuration"),
            str_lit(b, "NegativeInfinity"),
            b.unit(),
        )],
    );
    let bi_spec = node_con(b, "$BigInt", vec![b.none(), b.none(), b.bigint("0")]);
    let in_spec = node_con(
        b,
        "$Number",
        vec![b.none(), b.none(), str_lit(b, "integer")],
    );
    let chks = b.if_(
        b.loc("non_neg"),
        b.list([rt(b, "Arbitrary.IsNonNegativeDuration")]),
        b.list(vec![]),
    );
    let stmts = [
        b.let_("d0", push_simple(b, b.list(vec![]), d0_spec)),
        b.let_("d1", push_simple(b, b.list(vec![]), d1_spec)),
        b.let_("bi_id", push_simple(b, b.list(vec![]), bi_spec)),
        b.let_("d2", dur_obj_node(b, "bi_id", "Nanos")),
        b.let_("in_id", push_simple(b, b.list(vec![]), in_spec)),
        b.let_("d3", dur_obj_node(b, "in_id", "Millis")),
        b.let_(
            "mids",
            b.list([b.loc("d0"), b.loc("d1"), b.loc("d2"), b.loc("d3")]),
        ),
    ];
    let tail = push_node(
        b,
        b.loc("ctx"),
        raw_entry(
            b,
            b.loc("mids"),
            node_con(b, "$Union", vec![b.loc("mids")]),
            chks,
            b.none(),
            false,
        ),
    );
    b.block(stmts, tail)
}

fn json_node(b: &ExprBuilder) -> Expr {
    let sus = || {
        push_node(
            b,
            b.loc("ctx"),
            raw_entry(
                b,
                b.list([b.loc("uid")]),
                node_con(b, "$Suspend", vec![b.loc("uid")]),
                b.list(vec![]),
                b.none(),
                true,
            ),
        )
    };
    let arr_spec = node_con(
        b,
        "$Arrays",
        vec![
            b.list(vec![]),
            b.some(b.loc("s1")),
            b.bool_(false),
            b.float(0.0),
            float_inf(b),
        ],
    );
    let rec_spec = node_con(b, "$Record", vec![b.loc("j3"), b.loc("s2")]);
    let stmts = [
        b.let_(
            "uid",
            push_simple(b, b.list(vec![]), node_con(b, "$Never", vec![])),
        ),
        b.let_("s1", sus()),
        b.let_("s2", sus()),
        b.let_(
            "j0",
            push_simple(
                b,
                b.list(vec![]),
                node_con(b, "$Literal", vec![rt(b, "Schema.JsonNull")]),
            ),
        ),
        b.let_(
            "j1",
            push_simple(
                b,
                b.list(vec![]),
                node_con(b, "$Number", vec![b.none(), b.none(), str_lit(b, "finite")]),
            ),
        ),
        b.let_(
            "j2",
            push_simple(b, b.list(vec![]), node_con(b, "$Bool", vec![])),
        ),
        b.let_(
            "j3",
            push_simple(
                b,
                b.list(vec![]),
                b.app(rt(b, "Helper.arb_build_string"), b.none()),
            ),
        ),
        b.let_("j4", push_simple(b, b.list([b.loc("s1")]), arr_spec)),
        b.let_(
            "j5",
            push_simple(b, b.list([b.loc("j3"), b.loc("s2")]), rec_spec),
        ),
        b.let_(
            "jm",
            b.list([
                b.loc("j0"),
                b.loc("j1"),
                b.loc("j2"),
                b.loc("j3"),
                b.loc("j4"),
                b.loc("j5"),
            ]),
        ),
        set_node(
            b,
            b.loc("ctx"),
            b.loc("uid"),
            raw_entry(
                b,
                b.loc("jm"),
                node_con(b, "$Union", vec![b.loc("jm")]),
                b.list(vec![]),
                b.none(),
                false,
            ),
        ),
    ];
    b.block(stmts, b.loc("uid"))
}

fn datetime_zoned_branch(b: &ExprBuilder) -> Expr {
    let mk_zoned = b.lam(
        &["vs"],
        b.app2(
            rt(b, "Arbitrary.MakeZonedDateTime"),
            b.app2(rt(b, "Helper.list_at"), b.int(0), b.loc("vs")),
            b.app2(rt(b, "Helper.list_at"), b.int(1), b.loc("vs")),
        ),
    );
    let deps = b.list([b.loc("ep"), b.loc("zn")]);
    b.block(
        [
            b.let_(
                "ep",
                int_between(b, -8_639_999_949_600_000.0, 8_639_999_949_600_000.0),
            ),
            b.let_("zn", zones_union(b)),
        ],
        push_simple(
            b,
            deps.clone(),
            node_con(b, "$Objects", vec![deps, mk_zoned]),
        ),
    )
}

fn zone_map_branch(b: &ExprBuilder, var: &str, init: Expr) -> Expr {
    b.block(
        [b.let_(var, init)],
        push_simple(
            b,
            b.list([b.loc(var)]),
            node_con(b, "$Map", vec![b.loc(var), rt(b, "Arbitrary.MakeZone")]),
        ),
    )
}

pub(super) fn build_link_helper(b: &ExprBuilder) -> Expr {
    let tag_eq = |s: &str| b.bin(BinOp::Eq, b.loc("tag"), str_lit(b, s));
    let int_node = push_simple(
        b,
        b.list(vec![]),
        node_con(
            b,
            "$Number",
            vec![b.none(), b.none(), str_lit(b, "integer")],
        ),
    );
    let branch_offset = b.if_(
        tag_eq("datetime_zone_offset"),
        zone_map_branch(b, "in_id", int_node),
        json_node(b),
    );
    let branch_named = b.if_(
        tag_eq("datetime_zone_named"),
        zone_map_branch(b, "zn", named_zones(b)),
        branch_offset,
    );
    let branch_zone = b.if_(
        tag_eq("datetime_zone"),
        zone_map_branch(b, "zn", zones_union(b)),
        branch_named,
    );
    let branch_zoned = b.if_(
        tag_eq("datetime_zoned"),
        datetime_zoned_branch(b),
        branch_zone,
    );
    let utc_body = b.block(
        [b.let_(
            "ep",
            int_between(b, -8_640_000_000_000_000.0, 8_640_000_000_000_000.0),
        )],
        push_simple(
            b,
            b.list([b.loc("ep")]),
            node_con(
                b,
                "$Map",
                vec![b.loc("ep"), rt(b, "DateTime.UnsafeFromEpochMillis")],
            ),
        ),
    );
    let branch_utc = b.if_(
        b.bin(
            BinOp::Or,
            tag_eq("datetime_utc"),
            tag_eq("datetime_utc_millis"),
        ),
        utc_body,
        branch_zoned,
    );
    let branch_dur_any = b.if_(
        b.bin(
            BinOp::Or,
            tag_eq("duration_millis"),
            tag_eq("duration_nanos"),
        ),
        b.block([b.let_("non_neg", b.bool_(false))], dur_base(b)),
        branch_utc,
    );
    let body = b.if_(
        b.bin(
            BinOp::Or,
            tag_eq("duration_nonneg"),
            tag_eq("duration_string"),
        ),
        b.block([b.let_("non_neg", b.bool_(true))], dur_base(b)),
        branch_dur_any,
    );
    b.lam(&["ctx", "tag"], body)
}
