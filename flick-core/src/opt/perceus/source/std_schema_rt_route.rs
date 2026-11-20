//! Synthetic `$rt.Helper.*` recursive helpers for `Route` and `Schema.arbitrary`.

use flick_ir::BinOp;

use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::{pair_pat, rt, str_lit};
use super::{Expr, Pat, Span, Stmt};

pub(super) fn helper_expr(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$srr");
    match name {
        "route_parts" => Some(route_parts_helper(&b)),
        "route_match" => Some(super::std_schema_rt_route_match::route_match_helper(&b)),
        "route_coerce" => Some(route_coerce_helper(&b)),
        "draw_schema" => Some(super::std_schema_rt_gen::draw_schema_helper(&b)),
        _ => None,
    }
}

fn route_coerce_helper(b: &ExprBuilder) -> Expr {
    b.lam(
        &["sch", "text"],
        b.block(
            vec![
                b.let_(
                    "cands",
                    b.app(rt(b, "Schema.RouteCoerceCandidates"), b.loc("text")),
                ),
                b.let_("vf", b.app(rt(b, "Helper.schema_val"), b.loc("sch"))),
                b.var_decl("res", b.none()),
                Stmt::Expr(b.for_ctrl(
                    "$rc",
                    "cand",
                    b.loc("cands"),
                    b.match_(
                        b.app(b.loc("vf"), b.loc("cand")),
                        vec![
                            arm(
                                b.ok_pat("v"),
                                b.block(
                                    vec![b.assign("res", b.some(b.loc("v")))],
                                    b.break_("$rc", b.unit()),
                                ),
                            ),
                            arm(Pat::Wild, b.unit()),
                        ],
                    ),
                )),
            ],
            b.var("res"),
        ),
    )
}

#[expect(clippy::too_many_lines, reason = "closed route printing AST template")]
fn route_parts_helper(b: &ExprBuilder) -> Expr {
    let url_pat = Pat::Con(
        ExprBuilder::con("Route", "Url", "$Url", 2),
        vec![Pat::Wild, Pat::Bind(b.name("routes"))],
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
    let seg_word_pat = Pat::Con(
        ExprBuilder::con("Route", "Segment", "$Words", 1),
        vec![Pat::Bind(b.name("wpairs"))],
    );
    let seg_nest_pat = Pat::Con(
        ExprBuilder::con("Route", "Segment", "$Nested", 1),
        vec![Pat::Bind(b.name("nget"))],
    );
    let bad_ctor = b.app(
        rt(b, "Schema.Defect"),
        str_lit(b, "TypeError: a constructor of another type"),
    );
    let bad_val = b.app(
        rt(b, "Schema.Defect"),
        str_lit(b, "TypeError: a value of another type"),
    );
    let find_word = b.for_ctrl(
        "$rw",
        "wp",
        b.loc("wpairs"),
        b.match_(
            b.loc("wp"),
            vec![arm(
                pair_pat(b, "ww", "wt"),
                b.if_(
                    b.bin(BinOp::Eq, b.loc("wt"), b.loc("wtag")),
                    b.block(
                        vec![b.assign("wfound", b.some(b.loc("ww")))],
                        b.break_("$rw", b.unit()),
                    ),
                    b.unit(),
                ),
            )],
        ),
    );
    let on_word_tag = b.block(
        vec![b.var_decl("wfound", b.none()), Stmt::Expr(find_word)],
        b.match_(
            b.var("wfound"),
            vec![
                arm(b.some_pat("word"), b.list(vec![b.loc("word")])),
                arm(Pat::Wild, bad_ctor.clone()),
            ],
        ),
    );
    let seg_words = b.match_(
        b.app(rt(b, "Schema.DeconstructAdt"), b.loc("fval")),
        vec![
            arm(
                b.some_pat("wd"),
                b.match_(
                    b.loc("wd"),
                    vec![arm(pair_pat(b, "wtag", "_wf"), on_word_tag)],
                ),
            ),
            arm(Pat::Wild, bad_ctor),
        ],
    );
    let seg_nested = b.match_(
        b.app2(
            rt(b, "Helper.route_parts"),
            b.app(b.loc("nget"), b.unit()),
            b.loc("fval"),
        ),
        vec![arm(pair_pat(b, "nparts", "_nr"), b.loc("nparts"))],
    );
    let seg_texts = b.match_(
        b.loc("seg"),
        vec![
            arm(seg_nest_pat, seg_nested),
            arm(seg_word_pat, seg_words),
            arm(
                Pat::Wild,
                b.list(vec![b.app(rt(b, "Schema.RouteJsString"), b.loc("fval"))]),
            ),
        ],
    );
    let emit_opt = b.match_(
        b.loc("raw"),
        vec![
            arm(
                b.some_pat("fval"),
                b.each("p", seg_texts.clone(), b.yield_(b.loc("p"))),
            ),
            arm(Pat::Wild, b.unit()),
        ],
    );
    let emit_req = b.block(
        vec![b.let_("fval", b.loc("raw"))],
        b.each("p", seg_texts, b.yield_(b.loc("p"))),
    );
    let seg_loop = b.each(
        "seg",
        b.loc("rsegs"),
        b.block(
            vec![
                b.let_("i", b.var("si")),
                b.assign("si", b.add_i(b.loc("i"), b.int(1))),
                b.let_(
                    "raw",
                    b.app2(rt(b, "Helper.list_at"), b.loc("i"), b.loc("fvals")),
                ),
                b.let_(
                    "use_opt",
                    b.bin(
                        BinOp::And,
                        b.loc("ropt"),
                        b.bin(BinOp::Eq, b.loc("i"), b.sub_i(b.loc("slen"), b.int(1))),
                    ),
                ),
            ],
            b.if_(b.loc("use_opt"), emit_opt, emit_req),
        ),
    );
    let parts_gen = b.gen_strict(b.block(
        vec![Stmt::Expr(b.if_(
            b.not(b.bin(BinOp::Eq, b.loc("rpath"), str_lit(b, ""))),
            b.yield_(b.loc("rpath")),
            b.unit(),
        ))],
        seg_loop,
    ));
    let on_path = b.block(
        vec![
            b.let_(
                "slen",
                b.app(b.global("List.Core", "length"), b.loc("rsegs")),
            ),
            b.var_decl("si", b.int(0)),
            b.let_("parts", parts_gen),
        ],
        b.tuple(vec![b.loc("parts"), b.loc("route")]),
    );
    let find_route = b.each(
        "r",
        b.loc("routes"),
        b.match_(
            b.loc("r"),
            vec![arm(
                Pat::Con(
                    ExprBuilder::con("Route", "Path", "$Path", 5),
                    vec![
                        Pat::Bind(b.name("ct")),
                        Pat::Wild,
                        Pat::Wild,
                        Pat::Wild,
                        Pat::Wild,
                    ],
                ),
                b.if_(
                    b.bin(BinOp::Eq, b.loc("ct"), b.loc("tag")),
                    b.block(vec![b.assign("found", b.some(b.loc("r")))], b.unit()),
                    b.unit(),
                ),
            )],
        ),
    );
    let on_tag_fvals = b.block(
        vec![b.var_decl("found", b.none()), Stmt::Expr(find_route)],
        b.match_(
            b.var("found"),
            vec![
                arm(
                    b.some_pat("route"),
                    b.match_(b.loc("route"), vec![arm(path_pat, on_path)]),
                ),
                arm(Pat::Wild, bad_val.clone()),
            ],
        ),
    );
    let on_url = b.match_(
        b.app(rt(b, "Schema.DeconstructAdt"), b.loc("val")),
        vec![
            arm(
                b.some_pat("df"),
                b.match_(
                    b.loc("df"),
                    vec![arm(pair_pat(b, "tag", "fvals"), on_tag_fvals)],
                ),
            ),
            arm(Pat::Wild, bad_val),
        ],
    );
    b.lam(
        &["url", "val"],
        b.match_(b.loc("url"), vec![arm(url_pat, on_url)]),
    )
}
