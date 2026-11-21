//! Helper AST templates for `Graph` combine, rendering, flow, and JSON codec adapters.

use super::expr_builder::{ExprBuilder, arm};
use super::std_codec::{REFUSE_FALLIBLE, leaf_codec};
use super::std_graph::{map_strict, snap_edges, snap_nodes};
use super::std_schema_rt::{con_app, extra_con, make_codec, meta_con, rt, str_lit};
use super::{Expr, Pat};

pub(super) fn flow_template(b: &ExprBuilder, op: &str) -> Expr {
    b.lam(
        &["cfg", "g"],
        b.block(
            vec![b.let_(
                "caps",
                map_strict(
                    b,
                    "e",
                    snap_edges(b, b.loc("g")),
                    b.app(
                        b.field(b.loc("cfg"), "capacity"),
                        b.field(b.loc("e"), "data"),
                    ),
                ),
            )],
            b.app(
                b.app3(
                    rt(b, op),
                    b.field(b.loc("cfg"), "source"),
                    b.field(b.loc("cfg"), "target"),
                    b.loc("caps"),
                ),
                b.loc("g"),
            ),
        ),
    )
}

pub(super) fn combine_template(b: &ExprBuilder, op: &str) -> Expr {
    let opt_fn = |field: &str| {
        b.match_(
            b.field(b.loc("opts"), field),
            vec![
                arm(b.some_pat("f"), b.loc("f")),
                arm(Pat::Wild, b.lam(&["x"], b.loc("x"))),
            ],
        )
    };
    let ids_of = |g_var: &str| {
        b.tuple(vec![
            b.loc(g_var),
            map_strict(
                b,
                "n",
                snap_nodes(b, b.loc(g_var)),
                b.app(b.loc("nf"), b.field(b.loc("n"), "data")),
            ),
            map_strict(
                b,
                "e",
                snap_edges(b, b.loc(g_var)),
                b.app(b.loc("ef"), b.field(b.loc("e"), "data")),
            ),
        ])
    };
    b.lam(
        &["opts", "that", "self"],
        b.block(
            vec![
                b.let_("nf", opt_fn("nodeIdentity")),
                b.let_("ef", opt_fn("edgeIdentity")),
            ],
            b.app3(
                rt(b, "Graph.Combine"),
                str_lit(b, op),
                ids_of("self"),
                ids_of("that"),
            ),
        ),
    )
}

pub(super) fn graphviz_template(b: &ExprBuilder) -> Expr {
    let label_fn = |field: &str| {
        b.match_(
            b.field(b.loc("opts"), field),
            vec![
                arm(b.some_pat("f"), b.loc("f")),
                arm(Pat::Wild, rt(b, "Graph.LabelText")),
            ],
        )
    };
    b.lam(
        &["opts", "g"],
        b.block(
            vec![
                b.let_("snap", b.app(rt(b, "Graph.ToSnapshot"), b.loc("g"))),
                b.let_("nf", label_fn("nodeLabel")),
                b.let_("ef", label_fn("edgeLabel")),
                b.let_(
                    "nl",
                    map_strict(
                        b,
                        "n",
                        b.field(b.loc("snap"), "nodes"),
                        b.app(b.loc("nf"), b.field(b.loc("n"), "data")),
                    ),
                ),
                b.let_(
                    "el",
                    map_strict(
                        b,
                        "e",
                        b.field(b.loc("snap"), "edges"),
                        b.app(b.loc("ef"), b.field(b.loc("e"), "data")),
                    ),
                ),
            ],
            b.app(
                b.app3(
                    rt(b, "Graph.ToGraphVizRender"),
                    b.field(b.loc("opts"), "graphName"),
                    b.loc("nl"),
                    b.loc("el"),
                ),
                b.loc("g"),
            ),
        ),
    )
}

pub(super) fn mermaid_template(b: &ExprBuilder) -> Expr {
    let label_fn = |field: &str| {
        b.match_(
            b.field(b.loc("opts"), field),
            vec![
                arm(b.some_pat("f"), b.loc("f")),
                arm(Pat::Wild, rt(b, "Graph.LabelText")),
            ],
        )
    };
    let shape_fn = b.match_(
        b.field(b.loc("opts"), "nodeShape"),
        vec![
            arm(b.some_pat("f"), b.loc("f")),
            arm(Pat::Wild, b.lam(&["_"], str_lit(b, "rectangle"))),
        ],
    );
    b.lam(
        &["opts", "g"],
        b.block(
            vec![
                b.let_("snap", b.app(rt(b, "Graph.ToSnapshot"), b.loc("g"))),
                b.let_("nf", label_fn("nodeLabel")),
                b.let_("ef", label_fn("edgeLabel")),
                b.let_("sf", shape_fn),
                b.let_(
                    "nl",
                    map_strict(
                        b,
                        "n",
                        b.field(b.loc("snap"), "nodes"),
                        b.app(b.loc("nf"), b.field(b.loc("n"), "data")),
                    ),
                ),
                b.let_(
                    "el",
                    map_strict(
                        b,
                        "e",
                        b.field(b.loc("snap"), "edges"),
                        b.app(b.loc("ef"), b.field(b.loc("e"), "data")),
                    ),
                ),
                b.let_(
                    "sh",
                    map_strict(
                        b,
                        "n",
                        b.field(b.loc("snap"), "nodes"),
                        b.app(b.loc("sf"), b.field(b.loc("n"), "data")),
                    ),
                ),
            ],
            b.app3(
                rt(b, "Graph.ToMermaidRender"),
                b.tuple(vec![
                    b.field(b.loc("opts"), "diagramType"),
                    b.field(b.loc("opts"), "direction"),
                ]),
                b.tuple(vec![b.loc("nl"), b.loc("el"), b.loc("sh")]),
                b.loc("g"),
            ),
        ),
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "closed Graph.json wire codec AST template"
)]
pub(super) fn graph_json_template(b: &ExprBuilder) -> Expr {
    let field_spec = |wire: &str, codec: Expr| {
        b.tuple(vec![
            str_lit(b, wire),
            con_app(
                b,
                "Codec",
                "$Field",
                "$Field",
                vec![str_lit(b, wire), codec, b.bool_(false), b.bool_(false)],
            ),
        ])
    };
    let list_codec = |item: Expr| {
        make_codec(
            b,
            b.app(
                rt(b, "Helper.decode_list"),
                b.app(rt(b, "Helper.codec_dec"), item.clone()),
            ),
            b.app(
                rt(b, "Helper.encode_list"),
                b.app(rt(b, "Helper.codec_enc"), item.clone()),
            ),
            b.lam(
                &["in"],
                b.match_(
                    b.app(rt(b, "Schema.ListFromWire"), b.loc("in")),
                    vec![
                        arm(b.some_pat("_"), b.bool_(true)),
                        arm(Pat::Wild, b.bool_(false)),
                    ],
                ),
            ),
            meta_con(b, "$List", vec![b.app(rt(b, "Helper.codec_meta"), item)]),
            extra_con(b, "$None", vec![]),
        )
    };
    let nat = || leaf_codec(b, "natural", meta_con(b, "$Number", vec![]));
    let block = |name: &str, fields: Vec<Expr>| {
        b.app3(
            rt(b, "Helper.make_block_codec"),
            str_lit(b, name),
            b.list(fields),
            b.bool_(false),
        )
    };
    b.lam(
        &["kind", "nc", "ec"],
        b.block(
            vec![
                b.let_("ks", b.list(vec![b.loc("kind")])),
                b.let_(
                    "type_c",
                    make_codec(
                        b,
                        b.app2(rt(b, "Schema.LiteralDecode"), b.loc("ks"), b.bool_(false)),
                        b.app2(rt(b, "Schema.LiteralDecode"), b.loc("ks"), b.bool_(false)),
                        b.app(rt(b, "Schema.LiteralAccepts"), b.loc("ks")),
                        meta_con(b, "$Literal", vec![b.loc("kind")]),
                        extra_con(b, "$None", vec![]),
                    ),
                ),
                b.let_(
                    "node_c",
                    block(
                        "IndexedNode",
                        vec![field_spec("index", nat()), field_spec("data", b.loc("nc"))],
                    ),
                ),
                b.let_(
                    "edge_c",
                    block(
                        "IndexedEdge",
                        vec![
                            field_spec("index", nat()),
                            field_spec("source", nat()),
                            field_spec("target", nat()),
                            field_spec("data", b.loc("ec")),
                        ],
                    ),
                ),
                b.let_(
                    "wire_c",
                    block(
                        "Graph",
                        vec![
                            field_spec("type", b.loc("type_c")),
                            field_spec("nodes", list_codec(b.loc("node_c"))),
                            field_spec("edges", list_codec(b.loc("edge_c"))),
                        ],
                    ),
                ),
            ],
            make_codec(
                b,
                b.lam(
                    &["in"],
                    b.match_(
                        b.app(
                            b.app(rt(b, "Helper.codec_dec"), b.loc("wire_c")),
                            b.loc("in"),
                        ),
                        vec![
                            arm(
                                b.ok_pat("rec"),
                                b.app2(rt(b, "Graph.JsonDecode"), b.loc("kind"), b.loc("rec")),
                            ),
                            arm(b.err_pat("e"), b.err(b.loc("e"))),
                        ],
                    ),
                ),
                b.lam(
                    &["v"],
                    b.match_(
                        b.app2(rt(b, "Graph.JsonEncode"), b.loc("kind"), b.loc("v")),
                        vec![
                            arm(
                                b.ok_pat("rec"),
                                b.app(
                                    b.app(rt(b, "Helper.codec_enc"), b.loc("wire_c")),
                                    b.loc("rec"),
                                ),
                            ),
                            arm(b.err_pat("e"), b.err(b.loc("e"))),
                        ],
                    ),
                ),
                b.app(rt(b, "Helper.codec_acc"), b.loc("wire_c")),
                meta_con(b, "$Refused", vec![str_lit(b, REFUSE_FALLIBLE)]),
                extra_con(b, "$None", vec![]),
            ),
        ),
    )
}
