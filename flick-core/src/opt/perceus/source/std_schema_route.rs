//! Audited `Route` and `Differ.json_patch` pure IR adapters.

use flick_ir::BinOp;

use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::{con_app, pair_pat, rt, triple_pat};
use super::{Expr, Pat};

pub(super) fn route_rt_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("routeSegment", 1) => Some(b.lam(
            &["s"],
            con_app(b, "Route", "Segment", "$Schema", vec![b.loc("s")]),
        )),
        ("routeWordsOf", 1) => Some(b.lam(
            &["pairs"],
            con_app(
                b,
                "Route",
                "Segment",
                "$Words",
                vec![b.gen_strict(b.each(
                    "p",
                    b.loc("pairs"),
                    b.match_(
                        b.loc("p"),
                        vec![arm(
                            pair_pat(b, "w", "c"),
                            b.match_(
                                b.app(rt(b, "Schema.TagOf"), b.loc("c")),
                                vec![arm(
                                    pair_pat(b, "ft", "_sn"),
                                    b.yield_(b.tuple(vec![b.loc("w"), b.loc("ft")])),
                                )],
                            ),
                        )],
                    ),
                ))],
            ),
        )),
        ("routeNestedOf", 1) => Some(b.lam(
            &["get"],
            con_app(b, "Route", "Segment", "$Nested", vec![b.loc("get")]),
        )),
        ("routeQuery", 3) => Some(b.lam(
            &["lbl", "s", "opt"],
            b.tuple(vec![b.loc("lbl"), b.loc("s"), b.loc("opt")]),
        )),
        ("routePath", 5) => Some(b.lam(
            &["ctor", "path", "segs", "query", "opt"],
            b.block(
                vec![
                    b.let_(
                        "arity",
                        b.add_i(
                            b.app(b.global("List.Core", "length"), b.loc("segs")),
                            b.match_(
                                b.loc("query"),
                                vec![arm(b.some_pat("_"), b.int(1)), arm(Pat::Wild, b.int(0))],
                            ),
                        ),
                    ),
                    b.let_(
                        "tag",
                        b.match_(
                            b.app(
                                rt(b, "Schema.TagOf"),
                                b.app2(rt(b, "Helper.apply_units"), b.loc("ctor"), b.loc("arity")),
                            ),
                            vec![arm(pair_pat(b, "ft", "_s"), b.loc("ft"))],
                        ),
                    ),
                ],
                con_app(
                    b,
                    "Route",
                    "Path",
                    "$Path",
                    vec![
                        b.loc("tag"),
                        b.loc("path"),
                        b.loc("segs"),
                        b.loc("query"),
                        b.loc("opt"),
                    ],
                ),
            ),
        )),
        ("routeCodec", 2) => Some(b.lam(
            &["s", "paths"],
            con_app(b, "Route", "Url", "$Url", vec![b.loc("s"), b.loc("paths")]),
        )),
        ("routePrint", 2) => Some(route_print_template(b)),
        _ => None,
    }
}

fn route_print_template(b: &ExprBuilder) -> Expr {
    let path_pat = Pat::Con(
        ExprBuilder::con("Route", "Path", "$Path", 5),
        vec![
            Pat::Wild,
            Pat::Wild,
            Pat::Bind(b.name("rsegs")),
            Pat::Bind(b.name("rq")),
            Pat::Wild,
        ],
    );
    let yield_opt_fv = b.match_(
        b.loc("fv"),
        vec![
            arm(
                b.some_pat("iv"),
                b.yield_(b.tuple(vec![
                    b.loc("qlbl"),
                    b.app(rt(b, "Schema.RouteJsString"), b.loc("iv")),
                ])),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let yield_req_fv = b.yield_(b.tuple(vec![
        b.loc("qlbl"),
        b.app(rt(b, "Schema.RouteJsString"), b.loc("fv")),
    ]));
    let lookup_qfield = b.match_(
        b.app2(rt(b, "Schema.RecordLookup"), b.loc("qrec"), b.loc("qlbl")),
        vec![
            arm(
                b.some_pat("fv"),
                b.if_(b.loc("qopt"), yield_opt_fv, yield_req_fv),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let qpairs_expr = b.gen_strict(b.each(
        "qf",
        b.loc("qfields"),
        b.match_(
            b.loc("qf"),
            vec![arm(triple_pat(b, "qlbl", "_qs", "qopt"), lookup_qfield)],
        ),
    ));
    let fmt_no_q = b.app2(
        rt(b, "Schema.RouteFormatUrl"),
        b.loc("parts"),
        b.list(vec![]),
    );
    let with_fvals = b.block(
        vec![
            b.let_(
                "qrec",
                b.app2(
                    rt(b, "Helper.list_at"),
                    b.app(b.global("List.Core", "length"), b.loc("rsegs")),
                    b.loc("fvals"),
                ),
            ),
            b.let_("qpairs", qpairs_expr),
        ],
        b.app2(
            rt(b, "Schema.RouteFormatUrl"),
            b.loc("parts"),
            b.loc("qpairs"),
        ),
    );
    let deconstruct = b.match_(
        b.app(rt(b, "Schema.DeconstructAdt"), b.loc("val")),
        vec![
            arm(
                b.some_pat("df"),
                b.match_(
                    b.loc("df"),
                    vec![arm(pair_pat(b, "_t", "fvals"), with_fvals)],
                ),
            ),
            arm(Pat::Wild, fmt_no_q.clone()),
        ],
    );
    let on_rq = b.match_(
        b.loc("rq"),
        vec![
            arm(b.some_pat("qfields"), deconstruct),
            arm(Pat::Wild, fmt_no_q),
        ],
    );
    let on_route = b.match_(b.loc("route"), vec![arm(path_pat, on_rq)]);
    b.lam(
        &["url", "val"],
        b.match_(
            b.app2(rt(b, "Helper.route_parts"), b.loc("url"), b.loc("val")),
            vec![arm(pair_pat(b, "parts", "route"), on_route)],
        ),
    )
}

pub(super) fn route_parse_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["url", "text"],
        b.match_(
            b.app(rt(b, "Schema.RouteParseUrl"), b.loc("text")),
            vec![
                arm(
                    b.some_pat("pq"),
                    b.match_(
                        b.loc("pq"),
                        vec![arm(
                            pair_pat(b, "parts", "qparams"),
                            b.app3(
                                rt(b, "Helper.route_match"),
                                b.loc("url"),
                                b.loc("parts"),
                                b.loc("qparams"),
                            ),
                        )],
                    ),
                ),
                arm(Pat::Wild, b.none()),
            ],
        ),
    )
}

pub(super) fn differ_json_patch(b: &ExprBuilder) -> Expr {
    let unwrap_or_die = |res: Expr| {
        b.match_(
            res,
            vec![
                arm(b.ok_pat("v"), b.loc("v")),
                arm(
                    b.err_pat("e"),
                    b.app(
                        rt(b, "Schema.Defect"),
                        b.app(rt(b, "Schema.FormatValue"), b.loc("e")),
                    ),
                ),
            ],
        )
    };
    let apply_patch = b.match_(
        b.app2(
            rt(b, "Schema.DifferApplyJsonPatch"),
            unwrap_or_die(b.app(b.loc("enc"), b.loc("old"))),
            b.loc("patch"),
        ),
        vec![
            arm(
                b.ok_pat("patched"),
                unwrap_or_die(b.app(b.loc("dec"), b.loc("patched"))),
            ),
            arm(
                b.err_pat("msg"),
                b.app(rt(b, "Schema.Defect"), b.loc("msg")),
            ),
        ],
    );
    let patch_fn = b.lam(
        &["old", "patch"],
        b.if_(
            b.bin(
                BinOp::Eq,
                b.app(b.global("List.Core", "length"), b.loc("patch")),
                b.int(0),
            ),
            b.loc("old"),
            apply_patch,
        ),
    );
    b.lam(
        &["codec"],
        b.block(
            vec![
                b.let_("dec", b.app(rt(b, "Helper.codec_dec"), b.loc("codec"))),
                b.let_("enc", b.app(rt(b, "Helper.codec_enc"), b.loc("codec"))),
            ],
            con_app(
                b,
                "Differ",
                "Differ",
                "$Differ",
                vec![
                    b.list(vec![]),
                    b.lam(
                        &["old", "new"],
                        b.app2(
                            rt(b, "Schema.DifferJsonPatchOps"),
                            unwrap_or_die(b.app(b.loc("enc"), b.loc("old"))),
                            unwrap_or_die(b.app(b.loc("enc"), b.loc("new"))),
                        ),
                    ),
                    b.lam(
                        &["a", "c"],
                        b.app2(b.global("List.Core", "append"), b.loc("a"), b.loc("c")),
                    ),
                    patch_fn,
                ],
            ),
        ),
    )
}
