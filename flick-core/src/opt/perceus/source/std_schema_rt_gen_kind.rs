//! Per-kind `$rt.Helper.draw_schema` generators for `Schema.arbitrary`.

use flick_ir::{Arith, BinOp, Num};

use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::{cat, con_app, pair_pat, rt, str_lit, triple_pat};
use super::std_schema_rt_gen::needs_gen;
use super::{Expr, Pat, Stmt};

#[expect(clippy::too_many_lines, reason = "closed ADT generator AST template")]
fn draw_adt_expr(b: &ExprBuilder) -> Expr {
    let step = |seed: Expr| b.app(rt(b, "Gen.Step"), seed);
    let fl_int = |x: Expr| {
        b.app(
            b.global("Int", "truncate"),
            b.app(b.global("Float", "floor"), x),
        )
    };
    let mul_f = |a: Expr, c: Expr| b.bin(BinOp::Arith(Arith::Mul, Num::Float), a, c);
    let leaves = b.gen_strict(b.each(
        "c",
        b.loc("cases"),
        b.match_(
            b.loc("c"),
            vec![arm(
                triple_pat(b, "_t", "_n", "its"),
                b.if_(
                    b.bin(
                        BinOp::Eq,
                        b.app(b.global("List.Core", "length"), b.loc("its")),
                        b.int(0),
                    ),
                    b.yield_(b.loc("c")),
                    b.unit(),
                ),
            )],
        ),
    ));
    let choices = b.if_(
        b.bin(BinOp::Gt, b.loc("depth"), b.int(3)),
        b.block(
            vec![b.let_("leaves", leaves)],
            b.if_(
                b.bin(
                    BinOp::Gt,
                    b.app(b.global("List.Core", "length"), b.loc("leaves")),
                    b.int(0),
                ),
                b.loc("leaves"),
                b.loc("cases"),
            ),
        ),
        b.loc("cases"),
    );
    let bump = |off: i32, bind: &str| {
        Stmt::Expr(b.if_(
            b.bin(BinOp::Le, b.add_i(b.loc("depth"), b.int(off)), b.int(3)),
            b.match_(
                step(b.var("st")),
                vec![arm(
                    pair_pat(b, bind, "_"),
                    b.block(vec![b.assign("st", b.loc(bind))], b.unit()),
                )],
            ),
            b.unit(),
        ))
    };
    let draw_fields = b.gen_strict(b.each(
        "it",
        b.loc("its"),
        b.match_(
            b.app3(
                rt(b, "Helper.draw_schema"),
                b.loc("it"),
                b.add_i(b.loc("depth"), b.int(2)),
                b.var("st"),
            ),
            vec![arm(
                pair_pat(b, "ns", "v"),
                b.block(vec![b.assign("st", b.loc("ns"))], b.yield_(b.loc("v"))),
            )],
        ),
    ));
    let on_pick = b.block(
        vec![b.let_("fvals", draw_fields)],
        b.tuple(vec![
            b.var("st"),
            b.app2(rt(b, "Schema.ConstructAdt"), b.loc("ftag"), b.loc("fvals")),
        ]),
    );
    let on_step = b.block(
        vec![
            b.let_(
                "pick",
                b.app2(
                    rt(b, "Helper.list_at"),
                    fl_int(mul_f(
                        b.loc("r"),
                        b.app(b.global("Int", "to_float"), b.loc("clen")),
                    )),
                    b.loc("choices"),
                ),
            ),
            b.var_decl("st", b.loc("s2")),
            bump(1, "sa"),
            bump(2, "sb"),
        ],
        b.match_(
            b.loc("pick"),
            vec![arm(triple_pat(b, "ftag", "_sn", "its"), on_pick)],
        ),
    );
    b.block(
        vec![
            b.let_("choices", choices),
            b.let_(
                "clen",
                b.app(b.global("List.Core", "length"), b.loc("choices")),
            ),
        ],
        b.match_(
            step(b.loc("s1")),
            vec![arm(pair_pat(b, "s2", "r"), on_step)],
        ),
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "closed Schema kind generator dispatch"
)]
pub(super) fn draw_kind_expr(b: &ExprBuilder) -> Expr {
    let step = |seed: Expr| b.app(rt(b, "Gen.Step"), seed);
    let fl_int = |x: Expr| {
        b.app(
            b.global("Int", "truncate"),
            b.app(b.global("Float", "floor"), x),
        )
    };
    let mul_f = |a: Expr, c: Expr| b.bin(BinOp::Arith(Arith::Mul, Num::Float), a, c);
    let sub_f = |a: Expr, c: Expr| b.bin(BinOp::Arith(Arith::Sub, Num::Float), a, c);
    let kc = |c: &str, n: usize| ExprBuilder::con("Schema", "$Kind", c, n);

    let alpha = b.list(
        (b'a'..=b'z')
            .map(|c| str_lit(b, &String::from(char::from(c))))
            .collect::<Vec<_>>(),
    );
    let draw_ch = b.block(
        vec![
            b.assign("st", b.loc("ns")),
            b.assign("i", b.add_i(b.var("i"), b.int(1))),
            b.let_(
                "ch",
                b.app2(
                    rt(b, "Helper.list_at"),
                    fl_int(mul_f(b.loc("r"), b.float(26.0))),
                    alpha,
                ),
            ),
            b.assign("out", cat(b, b.var("out"), b.loc("ch"))),
        ],
        b.unit(),
    );
    let str_arm = b.block(
        vec![
            b.var_decl("st", b.loc("s1")),
            b.var_decl("i", b.int(0)),
            b.var_decl("out", str_lit(b, "")),
        ],
        b.block(
            vec![Stmt::Expr(b.while_(
                b.bin(BinOp::Lt, b.var("i"), b.loc("sz")),
                b.match_(
                    step(b.var("st")),
                    vec![arm(pair_pat(b, "ns", "r"), draw_ch)],
                ),
            ))],
            b.tuple(vec![b.var("st"), b.var("out")]),
        ),
    );
    let list_loop = b.while_(
        b.bin(BinOp::Lt, b.var("i"), b.loc("sz")),
        b.match_(
            b.app3(
                rt(b, "Helper.draw_schema"),
                b.loc("it"),
                b.add_i(b.loc("depth"), b.int(1)),
                b.var("st"),
            ),
            vec![arm(
                pair_pat(b, "ns", "v"),
                b.block(
                    vec![
                        b.assign("st", b.loc("ns")),
                        b.assign("i", b.add_i(b.var("i"), b.int(1))),
                    ],
                    b.yield_(b.loc("v")),
                ),
            )],
        ),
    );
    let list_arm = b.block(
        vec![
            b.var_decl("st", b.loc("s1")),
            b.var_decl("i", b.int(0)),
            b.let_("xs", b.gen_strict(list_loop)),
        ],
        b.tuple(vec![b.var("st"), b.loc("xs")]),
    );
    let tuple_loop = b.each(
        "it",
        b.loc("its"),
        b.match_(
            b.app3(
                rt(b, "Helper.draw_schema"),
                b.loc("it"),
                b.add_i(b.loc("depth"), b.int(1)),
                b.var("st"),
            ),
            vec![arm(
                pair_pat(b, "ns", "v"),
                b.block(vec![b.assign("st", b.loc("ns"))], b.yield_(b.loc("v"))),
            )],
        ),
    );
    let tuple_arm = b.block(
        vec![
            b.var_decl("st", b.loc("s1")),
            b.let_("xs", b.gen_strict(tuple_loop)),
        ],
        b.tuple(vec![
            b.var("st"),
            b.app(rt(b, "Schema.TupleFromList"), b.loc("xs")),
        ]),
    );
    let rec_each = b.each(
        "fld",
        b.loc("flds"),
        b.match_(
            b.loc("fld"),
            vec![arm(
                pair_pat(b, "k", "fs"),
                b.match_(
                    b.app3(
                        rt(b, "Helper.draw_schema"),
                        b.loc("fs"),
                        b.add_i(b.loc("depth"), b.int(1)),
                        b.var("st"),
                    ),
                    vec![arm(
                        pair_pat(b, "ns", "v"),
                        b.block(
                            vec![b.assign("st", b.loc("ns"))],
                            b.yield_(b.tuple(vec![b.loc("k"), b.loc("v")])),
                        ),
                    )],
                ),
            )],
        ),
    );
    let rec_arm = b.block(
        vec![
            b.var_decl("st", b.loc("s1")),
            b.let_("ents", b.gen_strict(rec_each)),
        ],
        b.tuple(vec![
            b.var("st"),
            b.app(rt(b, "Schema.RecordFromSortedEntries"), b.loc("ents")),
        ]),
    );
    let opt_some = b.match_(
        b.app3(
            rt(b, "Helper.draw_schema"),
            b.loc("it"),
            b.add_i(b.loc("depth"), b.int(1)),
            b.loc("s2"),
        ),
        vec![arm(
            pair_pat(b, "s3", "v"),
            b.tuple(vec![b.loc("s3"), b.some(b.loc("v"))]),
        )],
    );
    let res_err = b.match_(
        b.app3(
            rt(b, "Helper.draw_schema"),
            b.loc("es"),
            b.add_i(b.loc("depth"), b.int(1)),
            b.loc("s2"),
        ),
        vec![arm(
            pair_pat(b, "s3", "v"),
            b.tuple(vec![b.loc("s3"), b.err(b.loc("v"))]),
        )],
    );
    let res_ok = b.match_(
        b.app3(
            rt(b, "Helper.draw_schema"),
            b.loc("os"),
            b.add_i(b.loc("depth"), b.int(1)),
            b.loc("s2"),
        ),
        vec![arm(
            pair_pat(b, "s3", "v"),
            b.tuple(vec![b.loc("s3"), b.ok(b.loc("v"))]),
        )],
    );
    let num_val = sub_f(
        b.app(
            b.global("Float", "floor"),
            mul_f(b.loc("r"), b.float(2000.0)),
        ),
        b.float(1000.0),
    );

    b.match_(
        b.loc("kind"),
        vec![
            arm(
                Pat::Con(kc("$Number", 0), vec![]),
                b.match_(
                    step(b.loc("s1")),
                    vec![arm(
                        pair_pat(b, "s2", "r"),
                        b.tuple(vec![b.loc("s2"), num_val.clone()]),
                    )],
                ),
            ),
            arm(Pat::Con(kc("$String", 0), vec![]), str_arm),
            arm(
                Pat::Con(kc("$Bool", 0), vec![]),
                b.match_(
                    step(b.loc("s1")),
                    vec![arm(
                        pair_pat(b, "s2", "r"),
                        b.tuple(vec![
                            b.loc("s2"),
                            b.bin(BinOp::Lt, b.loc("r"), b.float(0.5)),
                        ]),
                    )],
                ),
            ),
            arm(
                Pat::Con(kc("$BigInt", 0), vec![]),
                b.match_(
                    step(b.loc("s1")),
                    vec![arm(
                        pair_pat(b, "s2", "r"),
                        b.tuple(vec![b.loc("s2"), b.app(rt(b, "Int.ToBigInt"), num_val)]),
                    )],
                ),
            ),
            arm(
                Pat::Con(kc("$Unit", 0), vec![]),
                b.tuple(vec![b.loc("s1"), b.unit()]),
            ),
            arm(
                Pat::Con(kc("$List", 1), vec![Pat::Bind(b.name("it"))]),
                list_arm,
            ),
            arm(
                Pat::Con(kc("$Tuple", 1), vec![Pat::Bind(b.name("its"))]),
                tuple_arm,
            ),
            arm(
                Pat::Con(kc("$Record", 1), vec![Pat::Bind(b.name("flds"))]),
                rec_arm,
            ),
            arm(
                Pat::Con(kc("$Option", 1), vec![Pat::Bind(b.name("it"))]),
                b.match_(
                    step(b.loc("s1")),
                    vec![arm(
                        pair_pat(b, "s2", "r"),
                        b.if_(
                            b.bin(BinOp::Lt, b.loc("r"), b.float(0.3)),
                            b.tuple(vec![b.loc("s2"), b.none()]),
                            opt_some,
                        ),
                    )],
                ),
            ),
            arm(
                Pat::Con(
                    kc("$Result", 2),
                    vec![Pat::Bind(b.name("es")), Pat::Bind(b.name("os"))],
                ),
                b.match_(
                    step(b.loc("s1")),
                    vec![arm(
                        pair_pat(b, "s2", "r"),
                        b.if_(b.bin(BinOp::Lt, b.loc("r"), b.float(0.5)), res_err, res_ok),
                    )],
                ),
            ),
            arm(
                Pat::Con(kc("$Redacted", 1), vec![Pat::Bind(b.name("it"))]),
                b.match_(
                    b.app3(
                        rt(b, "Helper.draw_schema"),
                        b.loc("it"),
                        b.add_i(b.loc("depth"), b.int(1)),
                        b.loc("s1"),
                    ),
                    vec![arm(
                        pair_pat(b, "s2", "v"),
                        b.tuple(vec![
                            b.loc("s2"),
                            con_app(b, "Redacted", "Redacted", "$Redacted", vec![b.loc("v")]),
                        ]),
                    )],
                ),
            ),
            arm(
                Pat::Con(kc("$Lazy", 2), vec![Pat::Wild, Pat::Bind(b.name("th"))]),
                b.app3(
                    rt(b, "Helper.draw_schema"),
                    b.app(b.loc("th"), b.unit()),
                    b.loc("depth"),
                    b.loc("s1"),
                ),
            ),
            arm(
                Pat::Con(kc("$Adt", 2), vec![Pat::Wild, Pat::Bind(b.name("cases"))]),
                draw_adt_expr(b),
            ),
            arm(
                Pat::Con(kc("$Never", 0), vec![]),
                needs_gen(b, str_lit(b, "Never")),
            ),
            arm(Pat::Wild, needs_gen(b, str_lit(b, "Unknown"))),
        ],
    )
}
