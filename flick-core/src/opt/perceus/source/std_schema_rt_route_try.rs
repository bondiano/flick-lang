//! Per-route segment matching AST builder for `std_schema_rt_route_match`.

use flick_ir::BinOp;

use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::rt;
use super::std_schema_rt_route_match::with_query_expr;
use super::{Expr, Pat};

#[expect(
    clippy::too_many_lines,
    reason = "closed per-route segment matching AST template"
)]
pub(super) fn try_route_expr(b: &ExprBuilder) -> Expr {
    let with_query = with_query_expr(b);
    let parse_fixed_step = b.match_(
        b.app2(
            b.loc("pseg"),
            b.app2(rt(b, "Helper.list_at"), b.loc("cur"), b.loc("rsegs")),
            b.app2(rt(b, "Helper.list_at"), b.loc("cur"), b.loc("rest")),
        ),
        vec![
            arm(b.some_pat("sv"), b.yield_(b.loc("sv"))),
            arm(
                Pat::Wild,
                b.block(vec![b.assign("ok", b.bool_(false))], b.unit()),
            ),
        ],
    );
    let fixed_gen = b.gen_strict(b.while_(
        b.bin(
            BinOp::And,
            b.var("ok"),
            b.bin(BinOp::Lt, b.var("i"), b.sub_i(b.loc("n"), b.int(1))),
        ),
        b.block(
            vec![
                b.let_("cur", b.var("i")),
                b.assign("i", b.add_i(b.loc("cur"), b.int(1))),
            ],
            parse_fixed_step,
        ),
    ));
    let nested_present = b.match_(
        b.app3(
            rt(b, "Helper.route_match"),
            b.app(b.loc("nget"), b.unit()),
            b.loc("tail"),
            b.list(vec![]),
        ),
        vec![
            arm(
                b.some_pat("nv"),
                b.block(
                    vec![b.let_(
                        "pfields",
                        b.app2(
                            b.global("List.Core", "append"),
                            b.loc("fixed"),
                            b.list(vec![b.if_(b.loc("ropt"), b.some(b.loc("nv")), b.loc("nv"))]),
                        ),
                    )],
                    with_query.clone(),
                ),
            ),
            arm(Pat::Wild, b.none()),
        ],
    );
    let nested_absent = b.block(
        vec![b.let_(
            "pfields",
            b.app2(
                b.global("List.Core", "append"),
                b.loc("fixed"),
                b.list(vec![b.none()]),
            ),
        )],
        with_query.clone(),
    );
    let nested_tail = b.block(
        vec![
            b.let_(
                "tail",
                b.app2(
                    b.global("List.Core", "drop"),
                    b.sub_i(b.loc("n"), b.int(1)),
                    b.loc("rest"),
                ),
            ),
            b.let_(
                "tlen",
                b.app(b.global("List.Core", "length"), b.loc("tail")),
            ),
        ],
        b.if_(
            b.bin(
                BinOp::And,
                b.loc("ropt"),
                b.bin(BinOp::Eq, b.loc("tlen"), b.int(0)),
            ),
            nested_absent,
            nested_present,
        ),
    );
    let nested_arm = b.if_(
        b.bin(BinOp::Lt, b.loc("rlen"), b.sub_i(b.loc("n"), b.int(1))),
        b.none(),
        b.block(
            vec![
                b.var_decl("ok", b.bool_(true)),
                b.var_decl("i", b.int(0)),
                b.let_("fixed", fixed_gen),
            ],
            b.if_(b.not(b.var("ok")), b.none(), nested_tail),
        ),
    );

    let flat_parse = b.match_(
        b.app2(
            b.loc("pseg"),
            b.app2(rt(b, "Helper.list_at"), b.loc("cur"), b.loc("rsegs")),
            b.app2(rt(b, "Helper.list_at"), b.loc("cur"), b.loc("rest")),
        ),
        vec![
            arm(
                b.some_pat("sv"),
                b.yield_(b.if_(b.loc("is_last"), b.some(b.loc("sv")), b.loc("sv"))),
            ),
            arm(
                Pat::Wild,
                b.block(vec![b.assign("ok", b.bool_(false))], b.unit()),
            ),
        ],
    );
    let flat_gen = b.gen_strict(b.while_(
        b.bin(
            BinOp::And,
            b.var("ok"),
            b.bin(BinOp::Lt, b.var("i"), b.loc("n")),
        ),
        b.block(
            vec![
                b.let_("cur", b.var("i")),
                b.assign("i", b.add_i(b.loc("cur"), b.int(1))),
                b.let_(
                    "is_last",
                    b.bin(
                        BinOp::And,
                        b.loc("ropt"),
                        b.bin(BinOp::Eq, b.loc("cur"), b.sub_i(b.loc("n"), b.int(1))),
                    ),
                ),
            ],
            b.if_(
                b.bin(BinOp::And, b.loc("is_last"), b.loc("missing")),
                b.yield_(b.none()),
                flat_parse,
            ),
        ),
    ));
    let flat_arm = b.block(
        vec![b.let_(
            "missing",
            b.bin(
                BinOp::And,
                b.loc("ropt"),
                b.bin(BinOp::Eq, b.loc("rlen"), b.sub_i(b.loc("n"), b.int(1))),
            ),
        )],
        b.if_(
            b.bin(
                BinOp::And,
                b.not(b.bin(BinOp::Eq, b.loc("rlen"), b.loc("n"))),
                b.not(b.loc("missing")),
            ),
            b.none(),
            b.block(
                vec![
                    b.var_decl("ok", b.bool_(true)),
                    b.var_decl("i", b.int(0)),
                    b.let_("pfields", flat_gen),
                ],
                b.if_(b.var("ok"), with_query, b.none()),
            ),
        ),
    );
    let last_nested = b.if_(
        b.bin(BinOp::Eq, b.loc("n"), b.int(0)),
        b.none(),
        b.match_(
            b.app2(
                rt(b, "Helper.list_at"),
                b.sub_i(b.loc("n"), b.int(1)),
                b.loc("rsegs"),
            ),
            vec![
                arm(
                    Pat::Con(
                        ExprBuilder::con("Route", "Segment", "$Nested", 1),
                        vec![Pat::Bind(b.name("ng"))],
                    ),
                    b.some(b.loc("ng")),
                ),
                arm(Pat::Wild, b.none()),
            ],
        ),
    );
    b.block(
        vec![
            b.let_("n", b.app(b.global("List.Core", "length"), b.loc("rsegs"))),
            b.let_(
                "rlen",
                b.app(b.global("List.Core", "length"), b.loc("rest")),
            ),
            b.let_("nested_opt", last_nested),
        ],
        b.match_(
            b.loc("nested_opt"),
            vec![
                arm(b.some_pat("nget"), nested_arm),
                arm(Pat::Wild, flat_arm),
            ],
        ),
    )
}
