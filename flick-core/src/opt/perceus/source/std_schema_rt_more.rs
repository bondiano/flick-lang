//! Synthetic `$rt.Helper.*` recursive helpers for JSON Schema meta, blocks, checks, entries, and string trees.

mod entries;

use flick_ir::BinOp;

use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::{meta_con, pair_pat, rt};
use super::std_schema_rt_block::make_block_codec_helper;
use super::std_schema_rt_tree::{check_tree_helper, decode_tree_helper, encode_tree_helper};
use super::{Expr, Pat, Span, Stmt};

pub(super) fn helper_expr(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$srm");
    match name {
        "resolve_meta" => Some(resolve_meta_helper(&b)),
        "run_checks" => Some(entries::run_checks_helper(&b)),
        "make_block_codec" => Some(make_block_codec_helper(&b)),
        "make_entries_codec" => Some(entries::make_entries_codec_helper(&b)),
        "apply_prop_check" => Some(entries::apply_prop_check_helper(&b)),
        "check_tree" => Some(check_tree_helper(&b)),
        "decode_tree" => Some(decode_tree_helper(&b)),
        "encode_tree" => Some(encode_tree_helper(&b)),
        _ => None,
    }
}

pub(super) fn contains_str(b: &ExprBuilder, loop_id: &str, list: Expr, needle: Expr) -> Expr {
    let step = b.if_(
        b.bin(BinOp::Eq, b.loc("el"), needle),
        b.block(
            [b.assign("has", b.bool_(true))],
            b.break_(loop_id, b.unit()),
        ),
        b.unit(),
    );
    b.block(
        [
            b.var_decl("has", b.bool_(false)),
            Stmt::Expr(b.for_ctrl(loop_id, "el", list, step)),
        ],
        b.var("has"),
    )
}

fn resolve_lazy_meta(b: &ExprBuilder) -> Expr {
    let inner_meta = b.app(
        rt(b, "Helper.codec_meta"),
        b.app(rt(b, "Helper.schema_codec"), b.app(b.loc("th"), b.unit())),
    );
    let next_seen = b.app2(rt(b, "Helper.list_cons"), b.loc("nm"), b.loc("seen"));
    let resolved = b.app2(rt(b, "Helper.resolve_meta"), next_seen, inner_meta);
    b.if_(
        contains_str(b, "$rml", b.loc("seen"), b.loc("nm")),
        meta_con(b, "$ResolvedLazy", vec![b.loc("nm"), b.none()]),
        meta_con(b, "$ResolvedLazy", vec![b.loc("nm"), b.some(resolved)]),
    )
}

fn resolve_meta_helper(b: &ExprBuilder) -> Expr {
    let mc = |c: &str, n: usize| ExprBuilder::con("Codec", "$Meta", c, n);
    let rec = |e: Expr| b.app2(rt(b, "Helper.resolve_meta"), b.loc("seen"), e);
    let bind = |n: &str| Pat::Bind(b.name(n));
    let con_pat = |tag: &str, names: &[&str]| {
        Pat::Con(
            mc(tag, names.len()),
            names.iter().map(|n| bind(n)).collect(),
        )
    };
    let unary = |tag: &str| {
        arm(
            con_pat(tag, &["x"]),
            meta_con(b, tag, vec![rec(b.loc("x"))]),
        )
    };
    let map_items = |xs: Expr| b.gen_strict(b.each("m", xs, b.yield_(rec(b.loc("m")))));
    let map_pairs = |xs: Expr| {
        let step = b.match_(
            b.loc("p"),
            [arm(
                pair_pat(b, "k", "m"),
                b.yield_(b.tuple([b.loc("k"), rec(b.loc("m"))])),
            )],
        );
        b.gen_strict(b.each("p", xs, step))
    };
    let adt_step = b.match_(
        b.loc("c"),
        [arm(
            pair_pat(b, "sn", "fms"),
            b.yield_(b.tuple([b.loc("sn"), map_items(b.loc("fms"))])),
        )],
    );
    let adt_body = meta_con(
        b,
        "$Adt",
        vec![b.gen_strict(b.each("c", b.loc("cs"), adt_step))],
    );
    let arms = [
        arm(con_pat("$Lazy", &["nm", "th"]), resolve_lazy_meta(b)),
        unary("$Stage"),
        unary("$List"),
        unary("$NonEmptyList"),
        unary("$ListEnsure"),
        arm(
            con_pat("$Tuple", &["xs"]),
            meta_con(b, "$Tuple", vec![map_items(b.loc("xs"))]),
        ),
        unary("$Option"),
        unary("$TaggedOption"),
        arm(
            con_pat("$Result", &["em", "om"]),
            meta_con(b, "$Result", vec![rec(b.loc("em")), rec(b.loc("om"))]),
        ),
        arm(
            con_pat("$Record", &["ps"]),
            meta_con(b, "$Record", vec![map_pairs(b.loc("ps"))]),
        ),
        arm(
            con_pat("$Block", &["ps", "ig"]),
            meta_con(b, "$Block", vec![map_pairs(b.loc("ps")), b.loc("ig")]),
        ),
        arm(
            con_pat("$Entries", &["vm", "cs"]),
            meta_con(b, "$Entries", vec![rec(b.loc("vm")), b.loc("cs")]),
        ),
        arm(con_pat("$Adt", &["cs"]), adt_body),
        arm(
            con_pat("$Refined", &["n", "k", "bm"]),
            meta_con(
                b,
                "$Refined",
                vec![b.loc("n"), b.loc("k"), rec(b.loc("bm"))],
            ),
        ),
        arm(
            con_pat("$Checked", &["bm", "cs", "t", "d"]),
            meta_con(
                b,
                "$Checked",
                vec![rec(b.loc("bm")), b.loc("cs"), b.loc("t"), b.loc("d")],
            ),
        ),
        arm(Pat::Wild, b.loc("meta")),
    ];
    b.lam(&["seen", "meta"], b.match_(b.loc("meta"), arms))
}
