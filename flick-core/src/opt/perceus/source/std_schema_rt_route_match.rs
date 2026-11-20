//! Synthetic `$rt.Helper.route_match` helper for `Route.parse`.

use flick_ir::BinOp;

use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::{pair_pat, rt, str_lit, triple_pat};
use super::std_schema_rt_route_try::try_route_expr;
use super::{Expr, Pat, Stmt};

pub(super) fn with_query_expr(b: &ExprBuilder) -> Expr {
    let miss_q = || {
        b.if_(
            b.loc("qopt"),
            b.yield_(b.tuple(vec![b.loc("qlbl"), b.none()])),
            b.block(
                vec![b.assign("qok", b.bool_(false))],
                b.break_("$rmq", b.unit()),
            ),
        )
    };
    let coerce_q = b.match_(
        b.app2(rt(b, "Helper.route_coerce"), b.loc("qsch"), b.loc("qtxt")),
        vec![
            arm(
                b.some_pat("qval"),
                b.yield_(b.tuple(vec![
                    b.loc("qlbl"),
                    b.if_(b.loc("qopt"), b.some(b.loc("qval")), b.loc("qval")),
                ])),
            ),
            arm(Pat::Wild, miss_q()),
        ],
    );
    let on_field = b.match_(
        b.app(b.loc("lparam"), b.loc("qlbl")),
        vec![arm(b.some_pat("qtxt"), coerce_q), arm(Pat::Wild, miss_q())],
    );
    let qents = b.gen_strict(b.for_ctrl(
        "$rmq",
        "qf",
        b.loc("qfields"),
        b.match_(
            b.loc("qf"),
            vec![arm(triple_pat(b, "qlbl", "qsch", "qopt"), on_field)],
        ),
    ));
    let built_with_q = b.some(b.app2(
        rt(b, "Schema.ConstructAdt"),
        b.loc("rtag"),
        b.app2(
            b.global("List.Core", "append"),
            b.loc("pfields"),
            b.list(vec![
                b.app(rt(b, "Schema.RecordFromSortedEntries"), b.loc("qents")),
            ]),
        ),
    ));
    b.match_(
        b.loc("rq"),
        vec![
            arm(
                b.some_pat("qfields"),
                b.block(
                    vec![b.var_decl("qok", b.bool_(true)), b.let_("qents", qents)],
                    b.if_(b.var("qok"), built_with_q, b.none()),
                ),
            ),
            arm(
                Pat::Wild,
                b.some(b.app2(
                    rt(b, "Schema.ConstructAdt"),
                    b.loc("rtag"),
                    b.loc("pfields"),
                )),
            ),
        ],
    )
}

#[expect(clippy::too_many_lines, reason = "closed route matching AST template")]
pub(super) fn route_match_helper(b: &ExprBuilder) -> Expr {
    let url_pat = Pat::Con(
        ExprBuilder::con("Route", "Url", "$Url", 2),
        vec![Pat::Bind(b.name("usch")), Pat::Bind(b.name("routes"))],
    );
    let path_pat = Pat::Con(
        ExprBuilder::con("Route", "Path", "$Path", 5),
        vec![
            Pat::Bind(b.name("rtag")),
            Pat::Bind(b.name("rpath")),
            Pat::Bind(b.name("rsegs")),
            Pat::Bind(b.name("rq")),
            Pat::Bind(b.name("ropt")),
        ],
    );
    let words_loop = b.for_ctrl(
        "$rmw",
        "wp",
        b.loc("wps"),
        b.match_(
            b.loc("wp"),
            vec![arm(
                pair_pat(b, "ww", "wt"),
                b.if_(
                    b.bin(BinOp::Eq, b.loc("ww"), b.loc("txt")),
                    b.block(
                        vec![b.assign(
                            "wf",
                            b.some(b.app2(
                                rt(b, "Schema.ConstructAdt"),
                                b.loc("wt"),
                                b.list(vec![]),
                            )),
                        )],
                        b.break_("$rmw", b.unit()),
                    ),
                    b.unit(),
                ),
            )],
        ),
    );
    let parse_seg_fn = b.lam(
        &["seg", "txt"],
        b.match_(
            b.loc("seg"),
            vec![
                arm(
                    Pat::Con(
                        ExprBuilder::con("Route", "Segment", "$Words", 1),
                        vec![Pat::Bind(b.name("wps"))],
                    ),
                    b.block(
                        vec![b.var_decl("wf", b.none()), Stmt::Expr(words_loop)],
                        b.var("wf"),
                    ),
                ),
                arm(
                    Pat::Con(
                        ExprBuilder::con("Route", "Segment", "$Schema", 1),
                        vec![Pat::Bind(b.name("ss"))],
                    ),
                    b.app2(rt(b, "Helper.route_coerce"), b.loc("ss"), b.loc("txt")),
                ),
                arm(Pat::Wild, b.none()),
            ],
        ),
    );
    let lookup_loop = b.for_ctrl(
        "$rmp",
        "qp",
        b.loc("qparams"),
        b.match_(
            b.loc("qp"),
            vec![arm(
                pair_pat(b, "qk", "qv"),
                b.if_(
                    b.bin(BinOp::Eq, b.loc("qk"), b.loc("lbl")),
                    b.block(
                        vec![b.assign("pf", b.some(b.loc("qv")))],
                        b.break_("$rmp", b.unit()),
                    ),
                    b.unit(),
                ),
            )],
        ),
    );
    let lookup_param = b.lam(
        &["lbl"],
        b.block(
            vec![b.var_decl("pf", b.none()), Stmt::Expr(lookup_loop)],
            b.var("pf"),
        ),
    );
    let validate_cand = b.match_(
        b.app(b.loc("vf"), b.loc("cand")),
        vec![
            arm(
                b.ok_pat("checked"),
                b.block(
                    vec![b.assign("matched", b.some(b.loc("checked")))],
                    b.break_("$rm", b.unit()),
                ),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let on_rest = b.match_(
        try_route_expr(b),
        vec![
            arm(b.some_pat("cand"), validate_cand),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let has_prefix = b.bin(
        BinOp::And,
        b.bin(BinOp::Gt, b.loc("plen"), b.int(0)),
        b.bin(
            BinOp::Eq,
            b.app2(rt(b, "Helper.list_at"), b.int(0), b.loc("parts")),
            b.loc("rpath"),
        ),
    );
    let rest_opt = b.if_(
        b.bin(BinOp::Eq, b.loc("rpath"), str_lit(b, "")),
        b.some(b.loc("parts")),
        b.if_(
            has_prefix,
            b.some(b.app2(b.global("List.Core", "drop"), b.int(1), b.loc("parts"))),
            b.none(),
        ),
    );
    let route_step = b.block(
        vec![b.let_("rest_opt", rest_opt)],
        b.match_(
            b.loc("rest_opt"),
            vec![arm(b.some_pat("rest"), on_rest), arm(Pat::Wild, b.unit())],
        ),
    );
    let route_loop = b.for_ctrl(
        "$rm",
        "r",
        b.loc("routes"),
        b.match_(b.loc("r"), vec![arm(path_pat, route_step)]),
    );
    let on_url = b.block(
        vec![
            b.let_("pseg", parse_seg_fn),
            b.let_("lparam", lookup_param),
            b.let_(
                "plen",
                b.app(b.global("List.Core", "length"), b.loc("parts")),
            ),
            b.let_("vf", b.app(rt(b, "Helper.schema_val"), b.loc("usch"))),
            b.var_decl("matched", b.none()),
            Stmt::Expr(route_loop),
        ],
        b.var("matched"),
    );
    b.lam(
        &["url", "parts", "qparams"],
        b.match_(b.loc("url"), vec![arm(url_pat, on_url)]),
    )
}
