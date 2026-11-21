//! `ConfigProvider` builders (`make`, `layerAdd`, `fromDir`).

use flick_ir::BinOp;

use super::super::expr_builder::{ExprBuilder, arm};
use super::super::std_config::KEY_CONFIG_PROVIDER;
use super::super::std_observe_refs::{obs_read, obs_ref};
use super::super::std_schema_rt::{cat, con_app, rt, str_lit};
use super::super::{Expr, ExprKind, Pat};
use super::{id_path, prov_or_else, prov_source};

pub(super) fn make_template(b: &ExprBuilder) -> Expr {
    let se_con = ExprBuilder::con("ConfigProvider", "SourceError", "SourceError", 1);
    let default_err = b.err(str_lit(b, "the provider failed"));
    let unpack_err = b.match_(
        b.loc("err"),
        [
            arm(
                Pat::Con(se_con, vec![Pat::Bind(b.name("m"))]),
                b.err(b.loc("m")),
            ),
            arm(Pat::Wild, default_err.clone()),
        ],
    );
    let from_cause = b.match_(
        b.app(b.global("Cause", "find_error_option"), b.loc("c")),
        [
            arm(b.some_pat("err"), unpack_err),
            arm(Pat::Wild, default_err),
        ],
    );
    let is_pure_fail = b.bin(
        BinOp::And,
        b.app(b.global("Cause", "has_fails"), b.loc("c")),
        b.bin(
            BinOp::And,
            b.not(b.app(b.global("Cause", "has_dies"), b.loc("c"))),
            b.not(b.app(b.global("Cause", "has_interrupts"), b.loc("c"))),
        ),
    );
    let on_fail = b.match_(
        b.app(b.global("Exit", "failure"), b.loc("ex")),
        [arm(
            b.some_pat("c"),
            b.if_(
                is_pure_fail,
                from_cause,
                b.app(b.global("Failure", "rethrow"), b.loc("c")),
            ),
        )],
    );
    let run_load = b.app(
        b.global("Effect.Core", "exit"),
        b.task(b.app(b.loc("load"), b.loc("path"))),
    );
    let get = b.lam(
        &["path"],
        b.block(
            [b.let_("ex", run_load)],
            b.match_(
                b.app(b.global("Exit", "value"), b.loc("ex")),
                [
                    arm(b.some_pat("opt"), b.ok(b.loc("opt"))),
                    arm(Pat::Wild, on_fail),
                ],
            ),
        ),
    );
    b.lam(&["load"], prov_source(b, get, id_path(b)))
}

pub(super) fn layer_add_template(b: &ExprBuilder) -> Expr {
    let default_env = b.app(
        b.global("ConfigProvider", "from_env"),
        b.at(ExprKind::Record(vec![(
            "preserveEmptyStrings".into(),
            b.bool_(false),
        )])),
    );
    let cur = b.match_(
        obs_read(b, KEY_CONFIG_PROVIDER, b.none()),
        [
            arm(b.some_pat("existing"), b.loc("existing")),
            arm(Pat::Wild, default_env),
        ],
    );
    let comb = b.if_(
        b.field(b.loc("opts"), "asPrimary"),
        prov_or_else(b, b.loc("p"), b.loc("cur")),
        prov_or_else(b, b.loc("cur"), b.loc("p")),
    );
    let build = b.lam(
        &["_m"],
        b.block(
            [b.let_("cur", cur), b.let_("comb", comb)],
            b.app2(
                b.global("Context", "make"),
                obs_ref(b, KEY_CONFIG_PROVIDER, b.none()),
                b.some(b.loc("comb")),
            ),
        ),
    );
    b.lam(
        &["opts", "p"],
        b.app(b.global("Layer", "from_build"), build),
    )
}

fn dir_read_fallback(b: &ExprBuilder) -> Expr {
    let read_dir = b.app(
        b.global("Effect.Core", "exit"),
        b.task(b.app2(
            b.global("FileSystem", "read_directory"),
            b.bool_(false),
            b.loc("full"),
        )),
    );
    let names = b.gen_strict(b.each(
        "e",
        b.loc("ents"),
        b.yield_(b.app(rt(b, "Path.Basename"), b.loc("e"))),
    ));
    let rec_node = con_app(
        b,
        "ConfigProvider",
        "Node",
        "RecordNode",
        vec![names, b.none()],
    );
    let err_msg = cat(b, str_lit(b, "Failed to read file at "), b.loc("full"));
    let on_dir = b.match_(
        b.app(b.global("Exit", "value"), b.loc("dex")),
        [
            arm(b.some_pat("ents"), b.ok(b.some(rec_node))),
            arm(Pat::Wild, b.err(err_msg)),
        ],
    );
    b.block([b.let_("dex", read_dir)], on_dir)
}

pub(super) fn from_dir_template(b: &ExprBuilder) -> Expr {
    // Runs through the calling fiber's `FileSystem.FileSystem` service (works with mock layers too!).
    let seg_str = b.match_(
        b.app(rt(b, "Config.SegmentInfo"), b.loc("seg")),
        [
            arm(
                b.ok_pat("i"),
                b.app(b.global("Int", "to_string"), b.loc("i")),
            ),
            arm(b.err_pat("s"), b.loc("s")),
        ],
    );
    let parts = b.app2(
        b.global("List.Core", "append"),
        b.list([b.loc("root")]),
        b.gen_strict(b.each("seg", b.loc("path"), b.yield_(seg_str))),
    );
    let read_file = b.app(
        b.global("Effect.Core", "exit"),
        b.task(b.app(b.global("FileSystem", "read_file_string"), b.loc("full"))),
    );
    let is_empty = b.bin(
        BinOp::And,
        b.bin(BinOp::Eq, b.loc("tr"), str_lit(b, "")),
        b.not(b.loc("pres")),
    );
    let val_node = con_app(b, "ConfigProvider", "Node", "ValueNode", vec![b.loc("tr")]);
    let on_txt = b.block(
        [b.let_("tr", b.app(b.global("String", "trim"), b.loc("txt")))],
        b.if_(is_empty, b.ok(b.none()), b.ok(b.some(val_node))),
    );
    let get = b.lam(
        &["path"],
        b.block(
            [
                b.let_("parts", parts),
                b.let_("full", b.app(rt(b, "Path.Join"), b.loc("parts"))),
                b.let_("fex", read_file),
            ],
            b.match_(
                b.app(b.global("Exit", "value"), b.loc("fex")),
                [
                    arm(b.some_pat("txt"), on_txt),
                    arm(Pat::Wild, dir_read_fallback(b)),
                ],
            ),
        ),
    );
    let root = b.match_(
        b.field(b.loc("opts"), "rootPath"),
        [
            arm(b.some_pat("r"), b.loc("r")),
            arm(Pat::Wild, str_lit(b, "/")),
        ],
    );
    b.lam(
        &["opts"],
        b.block(
            [
                b.let_("root", root),
                b.let_("pres", b.field(b.loc("opts"), "preserveEmptyStrings")),
            ],
            prov_source(b, get, id_path(b)),
        ),
    )
}
