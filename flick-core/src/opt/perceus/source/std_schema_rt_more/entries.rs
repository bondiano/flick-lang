//! Check execution and `Codec.entries` runtime helpers.

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_schema_rt::{
    expected_err, extra_con, make_codec, map_result_list, meta_con, pair_pat, rt, schema_err,
    str_lit,
};
use super::super::{Expr, Pat, Stmt};

pub(super) fn run_checks_helper(b: &ExprBuilder) -> Expr {
    let atom_pat = Pat::Con(
        ExprBuilder::con("Check", "$Atom", "$Atom", 3),
        vec![Pat::Bind(b.name("rf")), Pat::Wild, Pat::Wild],
    );
    let on_msg = b.block(
        [b.assign("fail", b.some(schema_err(b, b.loc("msg"))))],
        b.break_("$rchk", b.unit()),
    );
    let check_res = b.match_(
        b.app(b.loc("rf"), b.loc("val")),
        [arm(b.some_pat("msg"), on_msg), arm(Pat::Wild, b.unit())],
    );
    let step = b.match_(b.loc("atom"), [arm(atom_pat, check_res)]);
    let finish = b.match_(
        b.var("fail"),
        [
            arm(b.some_pat("e"), b.err(b.loc("e"))),
            arm(Pat::Wild, b.ok(b.loc("val"))),
        ],
    );
    b.lam(
        &["checks", "val"],
        b.block(
            [
                b.var_decl("fail", b.none()),
                Stmt::Expr(b.for_ctrl("$rchk", "atom", b.loc("checks"), step)),
            ],
            finish,
        ),
    )
}

fn map_entry_pair(b: &ExprBuilder, fn_name: &str, out_name: &str) -> Expr {
    let on_res = b.match_(
        b.app(b.loc(fn_name), b.loc("v")),
        [
            arm(
                b.ok_pat(out_name),
                b.ok(b.tuple([b.loc("k"), b.loc(out_name)])),
            ),
            arm(
                b.err_pat("e"),
                b.err(b.app2(rt(b, "Schema.PrefixIssues"), b.loc("k"), b.loc("e"))),
            ),
        ],
    );
    b.match_(b.loc("ent"), [arm(pair_pat(b, "k", "v"), on_res)])
}

fn entries_decode_fn(b: &ExprBuilder) -> Expr {
    let mapped = map_result_list(
        b,
        "$ed",
        b.loc("ents"),
        "ent",
        "i",
        map_entry_pair(b, "vd", "dv"),
        |_idx, err| err,
    );
    let run_chks = b.match_(
        b.app2(rt(b, "Helper.run_checks"), b.loc("chks"), b.loc("input")),
        [
            arm(b.ok_pat("_"), b.ok(b.loc("pairs"))),
            arm(b.err_pat("e"), b.err(b.loc("e"))),
        ],
    );
    let on_ents = b.match_(
        mapped,
        [
            arm(b.err_pat("e"), b.err(b.loc("e"))),
            arm(b.ok_pat("pairs"), run_chks),
        ],
    );
    b.lam(
        &["input"],
        b.match_(
            b.app(rt(b, "Schema.RecordEntries"), b.loc("input")),
            [
                arm(b.some_pat("ents"), on_ents),
                arm(
                    Pat::Wild,
                    expected_err(b, str_lit(b, "an object"), b.loc("input")),
                ),
            ],
        ),
    )
}

fn entries_encode_fn(b: &ExprBuilder) -> Expr {
    let mapped = map_result_list(
        b,
        "$ee",
        b.loc("pairs"),
        "ent",
        "i",
        map_entry_pair(b, "ve", "wv"),
        |_idx, err| err,
    );
    let on_checked = b.match_(
        mapped,
        [
            arm(
                b.ok_pat("out"),
                b.ok(b.app(rt(b, "Schema.RecordFromEntries"), b.loc("out"))),
            ),
            arm(b.err_pat("e"), b.err(b.loc("e"))),
        ],
    );
    let check_call = b.app2(
        rt(b, "Helper.run_checks"),
        b.loc("chks"),
        b.app(rt(b, "Schema.RecordFromEntries"), b.loc("pairs")),
    );
    b.lam(
        &["pairs"],
        b.match_(
            check_call,
            [
                arm(b.err_pat("e"), b.err(b.loc("e"))),
                arm(b.ok_pat("_"), on_checked),
            ],
        ),
    )
}

pub(super) fn make_entries_codec_helper(b: &ExprBuilder) -> Expr {
    let atom_pat = Pat::Con(
        ExprBuilder::con("Check", "$Atom", "$Atom", 3),
        vec![Pat::Wild, Pat::Bind(b.name("m")), Pat::Wild],
    );
    let atom_step = b.match_(b.loc("a"), [arm(atom_pat, b.yield_(b.loc("m")))]);
    let atom_meta = b.gen_strict(b.each("a", b.loc("chks"), atom_step));
    let accepts = b.lam(
        &["in"],
        b.match_(
            b.app(rt(b, "Schema.RecordEntries"), b.loc("in")),
            [
                arm(b.some_pat("_"), b.bool_(true)),
                arm(Pat::Wild, b.bool_(false)),
            ],
        ),
    );
    let meta = meta_con(
        b,
        "$Entries",
        vec![b.app(rt(b, "Helper.codec_meta"), b.loc("val_c")), atom_meta],
    );
    let extra = extra_con(b, "$Entries", vec![b.loc("val_c"), b.loc("chks")]);
    b.lam(
        &["val_c", "chks"],
        b.block(
            [
                b.let_("vd", b.app(rt(b, "Helper.codec_dec"), b.loc("val_c"))),
                b.let_("ve", b.app(rt(b, "Helper.codec_enc"), b.loc("val_c"))),
            ],
            make_codec(
                b,
                entries_decode_fn(b),
                entries_encode_fn(b),
                accepts,
                meta,
                extra,
            ),
        ),
    )
}

pub(super) fn apply_prop_check_helper(b: &ExprBuilder) -> Expr {
    let entries_pat = Pat::Con(
        ExprBuilder::con("Codec", "$Extra", "$Entries", 2),
        vec![Pat::Bind(b.name("vc")), Pat::Bind(b.name("chks"))],
    );
    let next_chks = b.app2(
        b.global("List.Core", "append"),
        b.loc("chks"),
        b.list([b.loc("atom")]),
    );
    let defect_msg = "TypeError: a property check applies to a codec made by `Codec.entries`";
    b.lam(
        &["atom", "codec"],
        b.match_(
            b.app(rt(b, "Helper.codec_extra"), b.loc("codec")),
            [
                arm(
                    entries_pat,
                    b.app2(rt(b, "Helper.make_entries_codec"), b.loc("vc"), next_chks),
                ),
                arm(
                    Pat::Wild,
                    b.app(rt(b, "Schema.Defect"), str_lit(b, defect_msg)),
                ),
            ],
        ),
    )
}
