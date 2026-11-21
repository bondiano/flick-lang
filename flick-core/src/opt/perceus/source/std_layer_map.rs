//! Audited `LayerMap` and `LayerRef` pure IR adapters (Stage 4B).
//!
//! Dynamic keyed/singleton layers backed by captured maker environment,
//! `Layer.from_build`, `Layer.make_memo_map`, `Layer.build_with_memo_map`,
//! and manual-scope lifecycle management.

use flick_ir::{BinOp, Foreign, ForeignMode};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::{Code, Expr, ExprKind, Local, Lower, LowerError, Pat, Span, Stmt};

impl Lower<'_> {
    pub(super) fn std_layer_map_foreign(
        &mut self,
        value: &Foreign,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.dicts != 0 || !has_standard_shape(value) {
            return None;
        }
        let from = value.binding.from.as_deref()?;
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$lm");
        let expr = match from {
            "flick:LayerMap" => layer_map_template(&b, bind, value.mode, stages)?,
            "flick:LayerRef" => layer_ref_template(&b, bind, value.mode, stages)?,
            _ => return None,
        };
        Some(self.lower_with_manual_scope(&expr, "$lm", block, span))
    }
}

// LayerMap: `(lookup, env, memo, entries : Ref (List (k, Layer)), hold_owners : Ref (List (k, Owner)))`
// Wait: each key caches `(k, layer_obj)` where `layer_obj` is a memoizable `Layer` built with
// `Layer.from_build_memo` so `memo` shares the built services across every `provide (LayerMap.get k m)`
// and `preload` holds an observer on `m`'s owning scope!
fn lm_pat(b: &ExprBuilder) -> Pat {
    b.tup_pat(&["lkp", "env", "memo", "ents", "idle_opt"])
}

fn lm_find_layer(b: &ExprBuilder, ents: Expr, key: Expr) -> Expr {
    b.app2(
        b.global("List.Core", "find"),
        b.lam(
            &["p"],
            b.match_(
                b.loc("p"),
                vec![arm(
                    b.tup_pat(&["ek", "_el"]),
                    b.bin(BinOp::Eq, b.loc("ek"), key),
                )],
            ),
        ),
        ents,
    )
}

fn lm_get_or_create_layer(b: &ExprBuilder) -> Expr {
    let make_fresh_layer = b.block(
        vec![
            b.let_("inner", b.app(b.loc("lkp"), b.loc("k"))),
            b.let_("imemo", b.app(b.global("Layer", "make_memo_map"), b.unit())),
            b.let_(
                "lay",
                b.app(
                    b.global("Layer", "from_build_memo"),
                    b.lam(
                        &["_m"],
                        b.app2(
                            b.global("Layer", "build_with_memo_map"),
                            b.loc("imemo"),
                            b.loc("inner"),
                        ),
                    ),
                ),
            ),
            b.rset(
                b.loc("ents"),
                b.app2(
                    b.global("List.Core", "append"),
                    b.rget(b.loc("ents")),
                    b.list(vec![b.tuple(vec![b.loc("k"), b.loc("lay")])]),
                ),
            ),
        ],
        b.loc("lay"),
    );
    b.match_(
        lm_find_layer(b, b.rget(b.loc("ents")), b.loc("k")),
        vec![
            arm(
                b.some_pat("p"),
                b.match_(
                    b.loc("p"),
                    vec![arm(b.tup_pat(&["_ek", "el"]), b.loc("el"))],
                ),
            ),
            arm(Pat::Wild, make_fresh_layer),
        ],
    )
}

fn lm_build_key_expr(b: &ExprBuilder) -> Expr {
    b.block(
        vec![b.let_("lay", lm_get_or_create_layer(b))],
        b.app2(
            b.loc("env_run"),
            b.loc("env"),
            b.task(b.app2(
                b.global("Layer", "build_with_memo_map"),
                b.loc("memo"),
                b.loc("lay"),
            )),
        ),
    )
}

fn lm_get_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["k", "m"],
        b.match_(
            b.loc("m"),
            vec![arm(
                lm_pat(b),
                b.app(
                    b.global("Layer", "from_build"),
                    b.lam(&["_cm"], lm_build_key_expr(b)),
                ),
            )],
        ),
    )
}

fn lm_make_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["lkp", "opts"],
        b.block(
            vec![
                b.let_("env", b.app(b.loc("env_capture"), b.unit())),
                b.let_("memo", b.app(b.global("Layer", "make_memo_map"), b.unit())),
                b.let_("ents", b.rmake(b.list(vec![]))),
                b.let_("idle_opt", b.field(b.loc("opts"), "idleTimeToLive")),
                Stmt::Expr(b.match_(
                    b.field(b.loc("opts"), "preloadKeys"),
                    vec![
                        arm(
                            b.some_pat("ks"),
                            b.for_ctrl(
                                "lmp",
                                "k",
                                b.loc("ks"),
                                b.block(vec![b.let_("_ctx", lm_build_key_expr(b))], b.unit()),
                            ),
                        ),
                        arm(Pat::Wild, b.unit()),
                    ],
                )),
            ],
            b.tuple(vec![
                b.loc("lkp"),
                b.loc("env"),
                b.loc("memo"),
                b.loc("ents"),
                b.loc("idle_opt"),
            ]),
        ),
    )
}

fn lm_record_lookup(b: &ExprBuilder) -> Expr {
    b.lam(
        &["k"],
        b.match_(
            b.app2(
                b.global("List.Core", "find"),
                b.lam(
                    &["p"],
                    b.match_(
                        b.loc("p"),
                        vec![arm(
                            b.tup_pat(&["pk", "_pl"]),
                            b.bin(BinOp::Eq, b.loc("pk"), b.loc("k")),
                        )],
                    ),
                ),
                b.loc("pairs"),
            ),
            vec![
                arm(
                    b.some_pat("p"),
                    b.match_(
                        b.loc("p"),
                        vec![arm(b.tup_pat(&["_pk", "pl"]), b.loc("pl"))],
                    ),
                ),
                arm(
                    Pat::Wild,
                    b.app(
                        b.global("Effect.Core", "die"),
                        b.str_("LayerMap has no key"),
                    ),
                ),
            ],
        ),
    )
}

fn lm_from_record_template(b: &ExprBuilder) -> Expr {
    let lookup_rec = lm_record_lookup(b);
    let pre_keys = b.if_(
        b.field(b.loc("opts"), "preload"),
        b.some(b.app2(
            b.global("List", "map"),
            b.lam(
                &["p"],
                b.match_(
                    b.loc("p"),
                    vec![arm(b.tup_pat(&["pk", "_pl"]), b.loc("pk"))],
                ),
            ),
            b.loc("pairs"),
        )),
        b.none(),
    );
    b.lam(
        &["pairs", "opts"],
        b.app2(
            b.global("LayerMap", "make"),
            lookup_rec,
            b.at(ExprKind::Record(vec![
                (
                    "idleTimeToLive".into(),
                    b.field(b.loc("opts"), "idleTimeToLive"),
                ),
                ("preloadKeys".into(), pre_keys),
            ])),
        ),
    )
}

fn lm_invalidate_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["k", "m"],
        b.match_(
            b.loc("m"),
            vec![arm(
                lm_pat(b),
                b.block(
                    vec![b.rset(
                        b.loc("ents"),
                        b.app2(
                            b.global("List.Core", "filter"),
                            b.lam(
                                &["p"],
                                b.match_(
                                    b.loc("p"),
                                    vec![arm(
                                        b.tup_pat(&["ek", "_el"]),
                                        b.not(b.bin(BinOp::Eq, b.loc("ek"), b.loc("k"))),
                                    )],
                                ),
                            ),
                            b.rget(b.loc("ents")),
                        ),
                    )],
                    b.unit(),
                ),
            )],
        ),
    )
}

fn layer_map_template(
    b: &ExprBuilder,
    bind: &str,
    mode: ForeignMode,
    stages: usize,
) -> Option<Expr> {
    if mode == ForeignMode::Pure && bind == "flickGet" && stages == 2 {
        return Some(lm_get_template(b));
    }
    if mode != ForeignMode::Effect {
        return None;
    }
    match (bind, stages) {
        ("flickMake", 2) => Some(lm_make_template(b)),
        ("flickFromRecord", 2) => Some(lm_from_record_template(b)),
        ("flickInvalidate", 2) => Some(lm_invalidate_template(b)),
        _ => None,
    }
}

// LayerRef: `(map : LayerMap, hold : Ref (Option Owner))`
fn lr_release_hold(b: &ExprBuilder, next_opt: Expr) -> Expr {
    b.block(
        vec![
            b.let_("old", b.rget(b.loc("hold"))),
            b.rset(b.loc("hold"), next_opt),
        ],
        b.match_(
            b.loc("old"),
            vec![
                arm(
                    b.some_pat("o"),
                    b.app2(b.loc("owner_close"), b.loc("o"), b.none()),
                ),
                arm(Pat::Wild, b.unit()),
            ],
        ),
    )
}

fn lr_get_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["ref"],
        b.match_(
            b.loc("ref"),
            vec![arm(
                b.tup_pat(&["m", "_h"]),
                b.app2(b.global("LayerMap", "get"), b.unit(), b.loc("m")),
            )],
        ),
    )
}

fn lr_make_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["lay", "opts"],
        b.block(
            vec![
                b.let_(
                    "m",
                    b.app2(
                        b.global("LayerMap", "make"),
                        b.lam(&["_u"], b.loc("lay")),
                        b.at(ExprKind::Record(vec![
                            (
                                "idleTimeToLive".into(),
                                b.field(b.loc("opts"), "idleTimeToLive"),
                            ),
                            ("preloadKeys".into(), b.none()),
                        ])),
                    ),
                ),
                b.let_("hold", b.rmake(b.none())),
                Stmt::Expr(b.app(
                    b.global("Scope", "add_finalizer"),
                    b.task(lr_release_hold(b, b.none())),
                )),
                b.let_("ref", b.tuple(vec![b.loc("m"), b.loc("hold")])),
                Stmt::Expr(b.if_(
                    b.field(b.loc("opts"), "preload"),
                    b.app(b.global("LayerRef", "refresh"), b.loc("ref")),
                    b.unit(),
                )),
            ],
            b.loc("ref"),
        ),
    )
}

fn lr_invalidate_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["ref"],
        b.match_(
            b.loc("ref"),
            vec![arm(
                b.tup_pat(&["m", "hold"]),
                b.block(
                    vec![Stmt::Expr(b.app2(
                        b.global("LayerMap", "invalidate"),
                        b.unit(),
                        b.loc("m"),
                    ))],
                    lr_release_hold(b, b.none()),
                ),
            )],
        ),
    )
}

fn lr_refresh_template(b: &ExprBuilder) -> Expr {
    b.lam(
        &["ref"],
        b.match_(
            b.loc("ref"),
            vec![arm(
                b.tup_pat(&["m", "hold"]),
                b.block(
                    vec![
                        Stmt::Expr(b.app2(
                            b.global("LayerMap", "invalidate"),
                            b.unit(),
                            b.loc("m"),
                        )),
                        b.let_("ho", b.app(b.loc("owner_new"), b.unit())),
                        b.let_(
                            "_ctx",
                            b.app2(
                                b.loc("owner_run"),
                                b.loc("ho"),
                                b.task(b.app(
                                    b.global("Layer", "build"),
                                    b.app2(b.global("LayerMap", "get"), b.unit(), b.loc("m")),
                                )),
                            ),
                        ),
                    ],
                    lr_release_hold(b, b.some(b.loc("ho"))),
                ),
            )],
        ),
    )
}

fn layer_ref_template(
    b: &ExprBuilder,
    bind: &str,
    mode: ForeignMode,
    stages: usize,
) -> Option<Expr> {
    if mode == ForeignMode::Pure && bind == "flickGet" && stages == 1 {
        return Some(lr_get_template(b));
    }
    if mode != ForeignMode::Effect {
        return None;
    }
    match (bind, stages) {
        ("flickMake", 2) => Some(lr_make_template(b)),
        ("flickInvalidate", 1) => Some(lr_invalidate_template(b)),
        ("flickRefresh", 1) => Some(lr_refresh_template(b)),
        _ => None,
    }
}
