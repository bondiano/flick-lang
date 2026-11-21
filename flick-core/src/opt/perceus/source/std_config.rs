//! Audited `Config` pure IR adapters (`packages/rpc/src/Config.fl`, Stage 4D).

mod combinators;
mod decode;
mod helpers;

use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::std_observe_refs::obs_read;
use super::std_schema_rt::{cat, con_app, rt, str_lit};
use super::{Code, Expr, Local, Lower, LowerError, Pat, Span, Stmt};

pub(super) use helpers::config_helper_expr;

pub(super) const KEY_CONFIG_PROVIDER: &str = "effect/ConfigProvider";

impl Lower<'_> {
    pub(super) fn std_config_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        if value.binding.from.as_deref()? != "flick:Config" {
            return None;
        }
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$cfg");
        let expr = config_template(&b, bind, value.mode, stages)?;
        Some(self.lower_with_manual_scope(&expr, "$cfg", block, span))
    }
}

// Config a: `(provider, prefix : List Segment) ->{} Result ConfigError a`
// where `Ok v` = value, `Err err` = recoverable missing ConfigError,
// and hard failures (malformed value / SourceError) are raised directly as `Config.ConfigError`.
// Wait: raising hard failures via `Fail.fail` while returning `Err missing` for recoverable
// missing values matches `config.go` 100% (`with_default`/`option` only catch `Err missing`,
// while `or_else` catches BOTH `Err missing` AND raised `ConfigError`!).

fn cfg_err(b: &ExprBuilder, msg: Expr) -> Expr {
    con_app(b, "Config", "ConfigError", "ConfigError", vec![msg])
}

fn fail_cfg(b: &ExprBuilder, err: Expr) -> Expr {
    b.app(b.global("Fail", "fail"), err)
}

fn schema_err_expr(b: &ExprBuilder, path: Expr, msg: Expr) -> Expr {
    let idx_seg = cat(
        b,
        str_lit(b, "["),
        cat(
            b,
            b.app(b.global("Int", "to_string"), b.loc("i")),
            str_lit(b, "]"),
        ),
    );
    let key_seg = cat(
        b,
        str_lit(b, "["),
        cat(
            b,
            b.app(rt(b, "Arbitrary.QuoteJson"), b.loc("s")),
            str_lit(b, "]"),
        ),
    );
    let seg_txt = b.match_(
        b.app(rt(b, "Config.SegmentInfo"), b.loc("seg")),
        [arm(b.ok_pat("i"), idx_seg), arm(b.err_pat("s"), key_seg)],
    );
    let with_path = b.block(
        [
            b.var_decl("at", str_lit(b, "")),
            Stmt::Expr(b.each(
                "seg",
                b.loc("p"),
                b.assign_e("at", cat(b, b.var("at"), seg_txt)),
            )),
        ],
        cat(b, b.loc("m"), cat(b, str_lit(b, "\n  at "), b.var("at"))),
    );
    let has_path = b.bin(
        BinOp::Gt,
        b.app(b.global("List.Core", "length"), b.loc("p")),
        b.int(0),
    );
    let full = b.if_(has_path, with_path, b.loc("m"));
    let wrapped = cat(
        b,
        str_lit(b, "SchemaError("),
        cat(b, b.loc("full"), str_lit(b, ")")),
    );
    b.block(
        [b.let_("p", path), b.let_("m", msg), b.let_("full", full)],
        cfg_err(b, wrapped),
    )
}

fn load_node(b: &ExprBuilder, prov: Expr, path: Expr) -> Expr {
    let src_err = cfg_err(b, cat(b, str_lit(b, "SourceError: "), b.loc("msg")));
    b.match_(
        b.app2(rt(b, "Helper.cfg_prov_load"), prov, path),
        [
            arm(b.ok_pat("opt"), b.loc("opt")),
            arm(b.err_pat("msg"), fail_cfg(b, src_err)),
        ],
    )
}

fn node_value_opt(b: &ExprBuilder, node_opt: Expr) -> Expr {
    let nc = |c: &str, n: usize| ExprBuilder::con("ConfigProvider", "Node", c, n);
    let on_node = b.match_(
        b.loc("nd"),
        [
            arm(
                Pat::Con(nc("ValueNode", 1), vec![Pat::Bind(b.name("v"))]),
                b.some(b.loc("v")),
            ),
            arm(
                Pat::Con(
                    nc("RecordNode", 2),
                    vec![Pat::Wild, Pat::Bind(b.name("ov"))],
                ),
                b.loc("ov"),
            ),
            arm(
                Pat::Con(nc("ArrayNode", 2), vec![Pat::Wild, Pat::Bind(b.name("ov"))]),
                b.loc("ov"),
            ),
        ],
    );
    b.match_(
        node_opt,
        [arm(b.some_pat("nd"), on_node), arm(Pat::Wild, b.none())],
    )
}

fn push_seg(b: &ExprBuilder, path: Expr, seg: Expr) -> Expr {
    b.app2(b.global("List.Core", "append"), path, b.list([seg]))
}

fn parse_with_prov(b: &ExprBuilder, prov: Expr, cfg: Expr) -> Expr {
    b.match_(
        b.app2(cfg, prov, b.list(vec![])),
        [
            arm(b.ok_pat("v"), b.loc("v")),
            arm(b.err_pat("e"), fail_cfg(b, b.loc("e"))),
        ],
    )
}

fn scalar_cfg(b: &ExprBuilder, decode_fn: Expr) -> Expr {
    // decode_fn : String -> Result String val
    let decode_res = b.match_(
        b.app(decode_fn, b.loc("txt")),
        [
            arm(b.ok_pat("v"), b.ok(b.loc("v"))),
            arm(
                b.err_pat("iss"),
                fail_cfg(b, schema_err_expr(b, b.loc("path"), b.loc("iss"))),
            ),
        ],
    );
    let missing = b.err(schema_err_expr(
        b,
        b.loc("path"),
        str_lit(b, "Expected string"),
    ));
    let body = b.match_(
        node_value_opt(b, b.loc("nd")),
        [arm(b.some_pat("txt"), decode_res), arm(Pat::Wild, missing)],
    );
    b.lam(
        &["name", "p", "pref"],
        b.block(
            [
                b.let_("path", push_seg(b, b.loc("pref"), b.loc("name"))),
                b.let_("nd", load_node(b, b.loc("p"), b.loc("path"))),
            ],
            body,
        ),
    )
}

fn effect_config_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("flickGetWith", 2) => {
            let prov = b.match_(
                obs_read(b, KEY_CONFIG_PROVIDER, b.none()),
                [
                    arm(b.some_pat("p"), b.loc("p")),
                    arm(Pat::Wild, b.app(b.loc("fallback"), b.unit())),
                ],
            );
            Some(b.lam(
                &["fallback", "c"],
                b.block(
                    [b.let_("prov", prov)],
                    parse_with_prov(b, b.loc("prov"), b.loc("c")),
                ),
            ))
        }
        ("flickParse", 2) => Some(b.lam(&["p", "c"], parse_with_prov(b, b.loc("p"), b.loc("c")))),
        ("flickRedacted", 1) => Some(b.lam(
            &["name"],
            b.app2(
                b.global("Config", "map"),
                b.lam(
                    &["v"],
                    con_app(b, "Redacted", "Redacted", "$Redacted", vec![b.loc("v")]),
                ),
                b.app(b.global("Config", "string"), b.loc("name")),
            ),
        )),
        _ => None,
    }
}

fn scalar_prim_config_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    match (bind, stages) {
        ("String", 1) => Some(scalar_cfg(b, b.lam(&["t"], b.ok(b.loc("t"))))),
        ("NonEmptyString", 1) => Some(scalar_cfg(
            b,
            b.lam(
                &["t"],
                b.if_(
                    b.bin(BinOp::Eq, b.loc("t"), str_lit(b, "")),
                    b.err(str_lit(b, "Expected a value with a length of at least 1")),
                    b.ok(b.loc("t")),
                ),
            ),
        )),
        ("Number", 1) => Some(scalar_cfg(b, rt(b, "Helper.cfg_dec_number"))),
        ("Finite", 1) => Some(scalar_cfg(b, rt(b, "Helper.cfg_dec_finite"))),
        ("flickInt", 1) => Some(scalar_cfg(b, rt(b, "Helper.cfg_dec_int"))),
        ("Port", 1) => Some(scalar_cfg(b, rt(b, "Helper.cfg_dec_port"))),
        ("Boolean", 1) => Some(scalar_cfg(b, rt(b, "Helper.cfg_dec_bool"))),
        ("Duration", 1) => Some(scalar_cfg(
            b,
            b.lam(
                &["t"],
                b.match_(
                    b.app(b.global("Duration", "from_input"), b.loc("t")),
                    [
                        arm(b.ok_pat("d"), b.ok(b.loc("d"))),
                        arm(
                            Pat::Wild,
                            b.err(str_lit(b, "Expected a valid Duration string")),
                        ),
                    ],
                ),
            ),
        )),
        ("ByteSize", 1) => Some(scalar_cfg(
            b,
            b.lam(
                &["t"],
                b.match_(
                    b.app(rt(b, "Config.DecodeByteSize"), b.loc("t")),
                    [
                        arm(b.some_pat("n"), b.ok(b.loc("n"))),
                        arm(
                            Pat::Wild,
                            b.err(str_lit(b, "Expected a valid ByteSize string")),
                        ),
                    ],
                ),
            ),
        )),
        _ => None,
    }
}

fn scalar_config_template(b: &ExprBuilder, bind: &str, stages: usize) -> Option<Expr> {
    if let Some(expr) = scalar_prim_config_template(b, bind, stages) {
        return Some(expr);
    }
    match (bind, stages) {
        ("LogLevel", 1) => {
            let levels = b.list([
                str_lit(b, "All"),
                str_lit(b, "Trace"),
                str_lit(b, "Debug"),
                str_lit(b, "Info"),
                str_lit(b, "Warn"),
                str_lit(b, "Error"),
                str_lit(b, "Fatal"),
                str_lit(b, "None"),
            ]);
            Some(scalar_cfg(
                b,
                b.app(rt(b, "Helper.cfg_dec_literals"), levels),
            ))
        }
        ("flickUrl", 1) => Some(scalar_cfg(
            b,
            b.lam(
                &["t"],
                b.match_(
                    b.app(rt(b, "Config.ParseUrl"), b.loc("t")),
                    [
                        arm(b.some_pat("u"), b.ok(b.loc("u"))),
                        arm(
                            Pat::Wild,
                            b.err(str_lit(
                                b,
                                "Expected a string that will be decoded as a URL",
                            )),
                        ),
                    ],
                ),
            ),
        )),
        ("flickDate", 1) => Some(scalar_cfg(
            b,
            b.lam(
                &["t"],
                b.match_(
                    b.app(rt(b, "Config.ParseDate"), b.loc("t")),
                    [
                        arm(b.some_pat("ms"), b.ok(b.loc("ms"))),
                        arm(Pat::Wild, b.err(str_lit(b, "Expected a valid Date"))),
                    ],
                ),
            ),
        )),
        ("Literal", 2) => {
            let dec = b.app(rt(b, "Helper.cfg_dec_literals"), b.list([b.loc("lit")]));
            Some(b.lam(&["lit", "name"], b.app(scalar_cfg(b, dec), b.loc("name"))))
        }
        ("Literals", 2) => {
            let dec = b.app(rt(b, "Helper.cfg_dec_literals"), b.loc("opts"));
            Some(b.lam(&["opts", "name"], b.app(scalar_cfg(b, dec), b.loc("name"))))
        }
        _ => None,
    }
}

fn config_template(b: &ExprBuilder, bind: &str, mode: ForeignMode, stages: usize) -> Option<Expr> {
    if mode == ForeignMode::Effect {
        return effect_config_template(b, bind, stages);
    }
    if mode != ForeignMode::Pure {
        return None;
    }
    if let Some(expr) = scalar_config_template(b, bind, stages) {
        return Some(expr);
    }
    match (bind, stages) {
        ("map" | "mapEffect", 2) => Some(b.lam(
            &["f", "self", "p", "pref"],
            b.match_(
                b.app2(b.loc("self"), b.loc("p"), b.loc("pref")),
                [
                    arm(b.ok_pat("v"), b.ok(b.app(b.loc("f"), b.loc("v")))),
                    arm(b.err_pat("e"), b.err(b.loc("e"))),
                ],
            ),
        )),
        ("flatMap", 2) => Some(b.lam(
            &["f", "self", "p", "pref"],
            b.match_(
                b.app2(b.loc("self"), b.loc("p"), b.loc("pref")),
                [
                    arm(
                        b.ok_pat("v"),
                        b.app2(b.app(b.loc("f"), b.loc("v")), b.loc("p"), b.loc("pref")),
                    ),
                    arm(b.err_pat("e"), b.err(b.loc("e"))),
                ],
            ),
        )),
        ("flickOrElse", 2) => Some(combinators::cfg_or_else(b)),
        ("all", 1) => Some(combinators::cfg_all(b)),
        ("withDefault", 2) => Some(b.lam(
            &["def", "self", "p", "pref"],
            b.match_(
                b.app2(b.loc("self"), b.loc("p"), b.loc("pref")),
                [
                    arm(b.ok_pat("v"), b.ok(b.loc("v"))),
                    arm(b.err_pat("_"), b.ok(b.loc("def"))),
                ],
            ),
        )),
        ("option", 1) => Some(b.lam(
            &["self", "p", "pref"],
            b.match_(
                b.app2(b.loc("self"), b.loc("p"), b.loc("pref")),
                [
                    arm(b.ok_pat("v"), b.ok(b.some(b.loc("v")))),
                    arm(b.err_pat("_"), b.ok(b.none())),
                ],
            ),
        )),
        ("flickFail", 1) => Some(b.lam(
            &["msg", "_p", "_pref"],
            fail_cfg(
                b,
                cfg_err(b, cat(b, str_lit(b, "SourceError: "), b.loc("msg"))),
            ),
        )),
        ("succeed", 1) => Some(b.lam(&["v", "_p", "_pref"], b.ok(b.loc("v")))),
        ("nested", 2) => Some(b.lam(
            &["nm", "self", "p", "pref"],
            b.app2(
                b.loc("self"),
                b.loc("p"),
                push_seg(b, b.loc("pref"), b.loc("nm")),
            ),
        )),
        ("schema", 2) => Some(b.lam(
            &["sch", "name", "p", "pref"],
            b.app3(
                rt(b, "Helper.cfg_decode_at"),
                b.loc("p"),
                b.loc("sch"),
                push_seg(b, b.loc("pref"), b.loc("name")),
            ),
        )),
        ("flickArray", 3) => Some(combinators::cfg_array(b)),
        ("flickRecord", 3) => Some(combinators::cfg_record(b)),
        _ => None,
    }
}
