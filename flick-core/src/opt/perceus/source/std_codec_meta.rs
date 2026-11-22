//! Audited `Codec` transform, option, annotation, flip, catch, and effectful check adapters.

use flick_ir::BinOp;

use super::expr_builder::{ExprBuilder, arm};
use super::std_codec::{REFUSE_EFFECT, REFUSE_FALLBACK, REFUSE_FALLIBLE};
use super::std_schema_rt::{
    cat, con_app, expected_err, extra_con, make_codec, meta_con, rt, schema_err, str_lit,
};
use super::{Expr, ExprKind, Pat};

pub(super) fn codec_option_null(b: &ExprBuilder, nullish: bool) -> Expr {
    let extra = if nullish {
        extra_con(b, "$Nullish", vec![])
    } else {
        extra_con(b, "$None", vec![])
    };
    let dec_inner = b.match_(
        b.app(b.loc("id"), b.loc("in")),
        vec![
            arm(b.ok_pat("v"), b.ok(b.some(b.loc("v")))),
            arm(b.err_pat("e"), b.err(b.loc("e"))),
        ],
    );
    let dec_fn = b.lam(
        &["in"],
        b.if_(
            b.app(rt(b, "Schema.IsJsonNull"), b.loc("in")),
            b.ok(b.none()),
            b.if_(
                b.not(b.app(b.loc("ia"), b.loc("in"))),
                expected_err(b, str_lit(b, "Union"), b.loc("in")),
                dec_inner,
            ),
        ),
    );
    let enc_fn = b.lam(
        &["v"],
        b.match_(
            b.loc("v"),
            vec![
                arm(b.some_pat("x"), b.app(b.loc("ie"), b.loc("x"))),
                arm(Pat::Wild, b.ok(rt(b, "Schema.JsonNull"))),
            ],
        ),
    );
    let acc_fn = b.lam(
        &["in"],
        b.bin(
            BinOp::Or,
            b.app(rt(b, "Schema.IsJsonNull"), b.loc("in")),
            b.app(b.loc("ia"), b.loc("in")),
        ),
    );
    b.lam(
        &["item"],
        b.block(
            vec![
                b.let_("id", b.app(rt(b, "Helper.codec_dec"), b.loc("item"))),
                b.let_("ie", b.app(rt(b, "Helper.codec_enc"), b.loc("item"))),
                b.let_("ia", b.app(rt(b, "Helper.codec_acc"), b.loc("item"))),
            ],
            make_codec(
                b,
                dec_fn,
                enc_fn,
                acc_fn,
                meta_con(
                    b,
                    "$Option",
                    vec![b.app(rt(b, "Helper.codec_meta"), b.loc("item"))],
                ),
                extra,
            ),
        ),
    )
}

pub(super) fn codec_transform_pure(b: &ExprBuilder) -> Expr {
    codec_transform_pure_with_extra(b, extra_con(b, "$None", vec![]))
}

pub(super) fn codec_transform_as(b: &ExprBuilder) -> Expr {
    b.lam(
        &["s"],
        codec_transform_pure_with_extra(b, extra_con(b, "$TransformAs", vec![b.loc("s")])),
    )
}

fn codec_transform_pure_with_extra(b: &ExprBuilder, extra: Expr) -> Expr {
    b.lam(
        &["dec", "enc", "from"],
        make_codec(
            b,
            b.lam(
                &["in"],
                b.match_(
                    b.app(b.app(rt(b, "Helper.codec_dec"), b.loc("from")), b.loc("in")),
                    vec![
                        arm(b.ok_pat("v"), b.ok(b.app(b.loc("dec"), b.loc("v")))),
                        arm(b.err_pat("e"), b.err(b.loc("e"))),
                    ],
                ),
            ),
            b.lam(
                &["v"],
                b.app(
                    b.app(rt(b, "Helper.codec_enc"), b.loc("from")),
                    b.app(b.loc("enc"), b.loc("v")),
                ),
            ),
            b.app(rt(b, "Helper.codec_acc"), b.loc("from")),
            meta_con(
                b,
                "$Stage",
                vec![b.app(rt(b, "Helper.codec_meta"), b.loc("from"))],
            ),
            extra,
        ),
    )
}

pub(super) fn codec_transform_result(b: &ExprBuilder) -> Expr {
    let on_ok = b.match_(
        b.app(b.loc("dec"), b.loc("v")),
        vec![
            arm(b.ok_pat("w"), b.ok(b.loc("w"))),
            arm(
                b.err_pat("e"),
                b.err(b.app(rt(b, "Schema.JoinErrorMessages"), b.loc("e"))),
            ),
        ],
    );
    b.lam(
        &["dec", "enc", "from"],
        make_codec(
            b,
            b.lam(
                &["in"],
                b.match_(
                    b.app(b.app(rt(b, "Helper.codec_dec"), b.loc("from")), b.loc("in")),
                    vec![
                        arm(b.ok_pat("v"), on_ok),
                        arm(b.err_pat("e"), b.err(b.loc("e"))),
                    ],
                ),
            ),
            b.lam(
                &["v"],
                b.app(
                    b.app(rt(b, "Helper.codec_enc"), b.loc("from")),
                    b.app(b.loc("enc"), b.loc("v")),
                ),
            ),
            b.app(rt(b, "Helper.codec_acc"), b.loc("from")),
            meta_con(b, "$Refused", vec![str_lit(b, REFUSE_FALLIBLE)]),
            extra_con(b, "$None", vec![]),
        ),
    )
}

pub(super) fn codec_transform_effect(b: &ExprBuilder) -> Expr {
    b.lam(
        &["dec", "enc", "from"],
        make_codec(
            b,
            b.lam(
                &["in"],
                b.match_(
                    b.app(b.app(rt(b, "Helper.codec_dec"), b.loc("from")), b.loc("in")),
                    vec![
                        arm(
                            b.ok_pat("v"),
                            b.app(
                                rt(b, "Helper.schema_task"),
                                b.lam(&["_"], b.app(b.loc("dec"), b.loc("v"))),
                            ),
                        ),
                        arm(b.err_pat("e"), b.err(b.loc("e"))),
                    ],
                ),
            ),
            b.lam(
                &["v"],
                b.app(
                    b.app(rt(b, "Helper.codec_enc"), b.loc("from")),
                    b.app(b.loc("enc"), b.loc("v")),
                ),
            ),
            b.app(rt(b, "Helper.codec_acc"), b.loc("from")),
            meta_con(b, "$Refused", vec![str_lit(b, REFUSE_EFFECT)]),
            extra_con(b, "$None", vec![]),
        ),
    )
}

pub(super) fn codec_check_whole(b: &ExprBuilder) -> Expr {
    b.lam(
        &["chk", "msg", "from"],
        make_codec(
            b,
            b.lam(
                &["in"],
                b.match_(
                    b.app(b.app(rt(b, "Helper.codec_dec"), b.loc("from")), b.loc("in")),
                    vec![
                        arm(
                            b.ok_pat("v"),
                            b.if_(
                                b.app(b.loc("chk"), b.loc("v")),
                                b.ok(b.loc("v")),
                                b.err(schema_err(b, b.loc("msg"))),
                            ),
                        ),
                        arm(b.err_pat("e"), b.err(b.loc("e"))),
                    ],
                ),
            ),
            b.app(rt(b, "Helper.codec_enc"), b.loc("from")),
            b.app(rt(b, "Helper.codec_acc"), b.loc("from")),
            meta_con(
                b,
                "$Refused",
                vec![cat(
                    b,
                    str_lit(b, "the check \""),
                    cat(b, b.loc("msg"), str_lit(b, "\" has no JSON Schema form")),
                )],
            ),
            extra_con(b, "$None", vec![]),
        ),
    )
}

pub(super) fn dsl_on_missing(b: &ExprBuilder, effectful: bool) -> Expr {
    let om_fn = if effectful {
        b.lam(&["_"], b.app(rt(b, "Helper.schema_task"), b.loc("fac")))
    } else {
        b.lam(&["_"], b.ok(b.app(b.loc("fac"), b.unit())))
    };
    b.lam(
        &["fac", "fld"],
        b.match_(
            b.loc("fld"),
            vec![arm(
                Pat::Con(
                    ExprBuilder::con("Codec", "$Field", "$Field", 4),
                    vec![
                        Pat::Bind(b.name("w")),
                        Pat::Bind(b.name("c")),
                        Pat::Bind(b.name("o")),
                        Pat::Wild,
                    ],
                ),
                con_app(
                    b,
                    "Codec",
                    "$Field",
                    "$Field",
                    vec![b.loc("w"), b.loc("c"), b.loc("o"), b.some(om_fn)],
                ),
            )],
        ),
    )
}

pub(super) fn codec_annotate(b: &ExprBuilder) -> Expr {
    let pick = |new_o: Expr, old_o: Expr| {
        b.match_(
            new_o,
            vec![
                arm(b.some_pat("x"), b.some(b.loc("x"))),
                arm(Pat::Wild, old_o),
            ],
        )
    };
    let checked_pat = Pat::Con(
        ExprBuilder::con("Codec", "$Meta", "$Checked", 4),
        vec![
            Pat::Bind(b.name("bm")),
            Pat::Bind(b.name("cs")),
            Pat::Bind(b.name("ot")),
            Pat::Bind(b.name("od")),
        ],
    );
    let new_meta = b.match_(
        b.loc("om"),
        vec![
            arm(
                checked_pat,
                meta_con(
                    b,
                    "$Checked",
                    vec![
                        b.loc("bm"),
                        b.loc("cs"),
                        pick(b.loc("nt"), b.loc("ot")),
                        pick(b.loc("nd"), b.loc("od")),
                    ],
                ),
            ),
            arm(
                Pat::Wild,
                meta_con(
                    b,
                    "$Checked",
                    vec![b.loc("om"), b.list(vec![]), b.loc("nt"), b.loc("nd")],
                ),
            ),
        ],
    );
    b.lam(
        &["ann", "c"],
        b.block(
            vec![
                b.let_("nt", b.field(b.loc("ann"), "title")),
                b.let_("nd", b.field(b.loc("ann"), "description")),
                b.let_("om", b.app(rt(b, "Helper.codec_meta"), b.loc("c"))),
                b.let_("nm", new_meta),
            ],
            make_codec(
                b,
                b.app(rt(b, "Helper.codec_dec"), b.loc("c")),
                b.app(rt(b, "Helper.codec_enc"), b.loc("c")),
                b.app(rt(b, "Helper.codec_acc"), b.loc("c")),
                b.loc("nm"),
                b.app(rt(b, "Helper.codec_extra"), b.loc("c")),
            ),
        ),
    )
}

pub(super) fn codec_annotations(b: &ExprBuilder) -> Expr {
    b.lam(
        &["c"],
        b.match_(
            b.app(rt(b, "Helper.codec_meta"), b.loc("c")),
            vec![
                arm(
                    Pat::Con(
                        ExprBuilder::con("Codec", "$Meta", "$Checked", 4),
                        vec![
                            Pat::Wild,
                            Pat::Wild,
                            Pat::Bind(b.name("t")),
                            Pat::Bind(b.name("d")),
                        ],
                    ),
                    b.at(ExprKind::Record(vec![
                        ("description".into(), b.loc("d")),
                        ("title".into(), b.loc("t")),
                    ])),
                ),
                arm(
                    Pat::Wild,
                    b.at(ExprKind::Record(vec![
                        ("description".into(), b.none()),
                        ("title".into(), b.none()),
                    ])),
                ),
            ],
        ),
    )
}

pub(super) fn codec_flip(b: &ExprBuilder) -> Expr {
    let flipped = make_codec(
        b,
        b.app(rt(b, "Helper.codec_enc"), b.loc("c")),
        b.app(rt(b, "Helper.codec_dec"), b.loc("c")),
        b.lam(&["_"], b.bool_(true)),
        meta_con(
            b,
            "$Refused",
            vec![str_lit(b, "this codec has no JSON Schema form")],
        ),
        extra_con(b, "$Flipped", vec![b.loc("c")]),
    );
    b.lam(
        &["c"],
        b.match_(
            b.app(rt(b, "Helper.codec_extra"), b.loc("c")),
            vec![
                arm(
                    Pat::Con(
                        ExprBuilder::con("Codec", "$Extra", "$Flipped", 1),
                        vec![Pat::Bind(b.name("orig"))],
                    ),
                    b.loc("orig"),
                ),
                arm(Pat::Wild, flipped),
            ],
        ),
    )
}

pub(super) fn codec_catch(b: &ExprBuilder, on_decode: bool, effectful: bool) -> Expr {
    let run_fb = if effectful {
        b.app(
            rt(b, "Helper.schema_task"),
            b.lam(&["_"], b.app(b.loc("f"), b.loc("e"))),
        )
    } else {
        b.match_(
            b.app(b.loc("f"), b.loc("e")),
            vec![
                arm(b.ok_pat("v"), b.ok(b.loc("v"))),
                arm(
                    b.err_pat("e2"),
                    b.err(b.app(rt(b, "Schema.JoinErrorMessages"), b.loc("e2"))),
                ),
            ],
        )
    };
    let dec = if on_decode {
        b.lam(
            &["in"],
            b.match_(
                b.app(b.app(rt(b, "Helper.codec_dec"), b.loc("c")), b.loc("in")),
                vec![
                    arm(b.ok_pat("v"), b.ok(b.loc("v"))),
                    arm(b.err_pat("e"), run_fb.clone()),
                ],
            ),
        )
    } else {
        b.app(rt(b, "Helper.codec_dec"), b.loc("c"))
    };
    let enc = if on_decode {
        b.app(rt(b, "Helper.codec_enc"), b.loc("c"))
    } else {
        b.lam(
            &["v"],
            b.match_(
                b.app(b.app(rt(b, "Helper.codec_enc"), b.loc("c")), b.loc("v")),
                vec![
                    arm(b.ok_pat("w"), b.ok(b.loc("w"))),
                    arm(b.err_pat("e"), run_fb),
                ],
            ),
        )
    };
    let meta = if on_decode {
        meta_con(b, "$Refused", vec![str_lit(b, REFUSE_FALLBACK)])
    } else {
        b.app(rt(b, "Helper.codec_meta"), b.loc("c"))
    };
    b.lam(
        &["f", "c"],
        make_codec(
            b,
            dec,
            enc,
            b.app(rt(b, "Helper.codec_acc"), b.loc("c")),
            meta,
            extra_con(b, "$None", vec![]),
        ),
    )
}

pub(super) fn codec_check_effect(b: &ExprBuilder) -> Expr {
    let run_check = b.match_(
        b.app(
            rt(b, "Helper.schema_task"),
            b.lam(&["_"], b.app(b.loc("f"), b.loc("v"))),
        ),
        vec![
            arm(
                b.ok_pat("ok"),
                b.if_(
                    b.loc("ok"),
                    b.ok(b.loc("v")),
                    b.err(schema_err(b, str_lit(b, "an invalid value"))),
                ),
            ),
            arm(b.err_pat("e"), b.err(b.loc("e"))),
        ],
    );
    b.lam(
        &["f", "c"],
        make_codec(
            b,
            b.lam(
                &["in"],
                b.match_(
                    b.app(b.app(rt(b, "Helper.codec_dec"), b.loc("c")), b.loc("in")),
                    vec![
                        arm(b.ok_pat("v"), run_check),
                        arm(b.err_pat("e"), b.err(b.loc("e"))),
                    ],
                ),
            ),
            b.app(rt(b, "Helper.codec_enc"), b.loc("c")),
            b.app(rt(b, "Helper.codec_acc"), b.loc("c")),
            meta_con(b, "$Refused", vec![str_lit(b, REFUSE_EFFECT)]),
            extra_con(b, "$None", vec![]),
        ),
    )
}
