//! Audited `ConfigProvider` pure IR adapters (`packages/std/src/ConfigProvider.fl`, Stage 4D).

mod builders;

use flick_ir::{Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::std_config::KEY_CONFIG_PROVIDER;
use super::std_observe_refs::obs_ref;
use super::std_schema_rt::{con_app, rt};
use super::{Code, Env, Expr, Local, Lower, LowerError, Pat, Span};

impl Lower<'_> {
    pub(super) fn std_config_prov_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        if value.binding.from.as_deref()? != "flick:ConfigProvider" {
            return None;
        }
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$cp");
        let expr = prov_template(&b, bind, value.mode, stages)?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

// ConfigProvider ADT:
// - $Source(get_fn, transform_fn)
// - $OrElse(first, second)
// where `get_fn : Path -> Result String (Option Node)` and `transform_fn : Path -> Path`.

fn prov_source(b: &ExprBuilder, get_fn: Expr, tr_fn: Expr) -> Expr {
    con_app(b, "ConfigProvider", "$Prov", "$Source", vec![get_fn, tr_fn])
}

fn prov_or_else(b: &ExprBuilder, first: Expr, second: Expr) -> Expr {
    con_app(b, "ConfigProvider", "$Prov", "$OrElse", vec![first, second])
}

fn id_path(b: &ExprBuilder) -> Expr {
    b.lam(&["p"], b.loc("p"))
}

fn constant_case_template(b: &ExprBuilder) -> Expr {
    let map_seg = b.match_(
        b.app(rt(b, "Config.SegmentInfo"), b.loc("seg")),
        [
            arm(b.ok_pat("_"), b.yield_(b.loc("seg"))),
            arm(
                b.err_pat("s"),
                b.yield_(b.app(rt(b, "Config.ConfigCase"), b.loc("s"))),
            ),
        ],
    );
    let tr = b.lam(
        &["path"],
        b.gen_strict(b.each("seg", b.loc("path"), map_seg)),
    );
    b.lam(
        &["self"],
        b.app2(rt(b, "Helper.cfg_prov_map"), tr, b.loc("self")),
    )
}

fn prov_source_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickFromUnknown", 2) => {
            let get = b.lam(
                &["path"],
                b.ok(b.app3(
                    rt(b, "Config.UnknownNode"),
                    b.loc("pres"),
                    b.loc("root"),
                    b.loc("path"),
                )),
            );
            Some(b.lam(
                &["opts", "root"],
                b.block(
                    [b.let_("pres", b.field(b.loc("opts"), "preserveEmptyStrings"))],
                    prov_source(b, get, id_path(b)),
                ),
            ))
        }
        ("flickFromEnvRecord", 2) => {
            let get = b.lam(
                &["path"],
                b.ok(b.app3(
                    rt(b, "Config.EnvNode"),
                    b.loc("pres"),
                    b.loc("pairs"),
                    b.loc("path"),
                )),
            );
            Some(b.lam(
                &["opts", "pairs"],
                b.block(
                    [b.let_("pres", b.field(b.loc("opts"), "preserveEmptyStrings"))],
                    prov_source(b, get, id_path(b)),
                ),
            ))
        }
        ("flickFromDotEnvContents", 2) => {
            let pairs = b.app2(
                rt(b, "Config.ParseDotEnv"),
                b.field(b.loc("opts"), "expandVariables"),
                b.loc("text"),
            );
            let get = b.lam(
                &["path"],
                b.ok(b.app3(
                    rt(b, "Config.EnvNode"),
                    b.loc("pres"),
                    b.loc("pairs"),
                    b.loc("path"),
                )),
            );
            Some(b.lam(
                &["opts", "text"],
                b.block(
                    [
                        b.let_("pres", b.field(b.loc("opts"), "preserveEmptyStrings")),
                        b.let_("pairs", pairs),
                    ],
                    prov_source(b, get, id_path(b)),
                ),
            ))
        }
        _ => None,
    }
}

fn prov_template(b: &ExprBuilder, bind: &str, mode: ForeignMode, stages: usize) -> Option<Expr> {
    if mode == ForeignMode::Effect && bind == "flickFromDir" && stages == 1 {
        return Some(builders::from_dir_template(b));
    }
    if mode != ForeignMode::Pure {
        return None;
    }
    if let Some(expr) = prov_source_template(b, bind, stages) {
        return Some(expr);
    }
    match (bind, stages) {
        ("flickMake", 1) => Some(builders::make_template(b)),
        ("orElse", 2) => Some(b.lam(
            &["that", "self"],
            prov_or_else(b, b.loc("self"), b.loc("that")),
        )),
        ("mapInput", 2) => Some(b.lam(
            &["f", "self"],
            b.app2(rt(b, "Helper.cfg_prov_map"), b.loc("f"), b.loc("self")),
        )),
        ("constantCase", 1) => Some(constant_case_template(b)),
        ("nested", 2) => {
            let prepend = b.lam(
                &["path"],
                b.app2(
                    b.global("List.Core", "append"),
                    b.list([b.loc("pref")]),
                    b.loc("path"),
                ),
            );
            Some(b.lam(
                &["pref", "self"],
                b.app2(rt(b, "Helper.cfg_prov_map"), prepend, b.loc("self")),
            ))
        }
        ("layer", 1) => Some(b.lam(
            &["p"],
            b.app(
                b.global("Layer", "from_build"),
                b.lam(
                    &["_m"],
                    b.app2(
                        b.global("Context", "make"),
                        obs_ref(b, KEY_CONFIG_PROVIDER, b.none()),
                        b.some(b.loc("p")),
                    ),
                ),
            ),
        )),
        ("flickLayerAdd", 2) => Some(builders::layer_add_template(b)),
        _ => None,
    }
}

pub(super) fn prov_helper_expr(name: &str, span: Span) -> Option<Expr> {
    let b = ExprBuilder::new(span, "$cph");
    let src_pat = Pat::Con(
        ExprBuilder::con("ConfigProvider", "$Prov", "$Source", 2),
        vec![Pat::Bind(b.name("get")), Pat::Bind(b.name("tr"))],
    );
    let oe_pat = Pat::Con(
        ExprBuilder::con("ConfigProvider", "$Prov", "$OrElse", 2),
        vec![Pat::Bind(b.name("a")), Pat::Bind(b.name("c"))],
    );
    match name {
        "cfg_prov_load" => {
            let fallback = b.app2(rt(&b, "Helper.cfg_prov_load"), b.loc("c"), b.loc("path"));
            let check_opt = b.match_(
                b.loc("opt"),
                [
                    arm(b.some_pat("nd"), b.ok(b.some(b.loc("nd")))),
                    arm(Pat::Wild, fallback),
                ],
            );
            let oe_body = b.match_(
                b.app2(rt(&b, "Helper.cfg_prov_load"), b.loc("a"), b.loc("path")),
                [
                    arm(b.ok_pat("opt"), check_opt),
                    arm(b.err_pat("e"), b.err(b.loc("e"))),
                ],
            );
            let src_body = b.app(b.loc("get"), b.app(b.loc("tr"), b.loc("path")));
            Some(b.lam(
                &["prov", "path"],
                b.match_(
                    b.loc("prov"),
                    [arm(src_pat, src_body), arm(oe_pat, oe_body)],
                ),
            ))
        }
        "cfg_prov_map" => {
            let mapped_tr = b.lam(&["p"], b.app(b.loc("f"), b.app(b.loc("tr"), b.loc("p"))));
            let oe_body = prov_or_else(
                &b,
                b.app2(rt(&b, "Helper.cfg_prov_map"), b.loc("f"), b.loc("a")),
                b.app2(rt(&b, "Helper.cfg_prov_map"), b.loc("f"), b.loc("c")),
            );
            Some(b.lam(
                &["f", "prov"],
                b.match_(
                    b.loc("prov"),
                    [
                        arm(src_pat, prov_source(&b, b.loc("get"), mapped_tr)),
                        arm(oe_pat, oe_body),
                    ],
                ),
            ))
        }
        _ => None,
    }
}
