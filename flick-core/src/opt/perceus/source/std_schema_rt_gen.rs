//! Synthetic `$rt.Helper.draw_schema` outer dispatch for `Schema.arbitrary`.

use flick_ir::{Arith, BinOp, Num};

use super::expr_builder::{ExprBuilder, arm};
use super::std_schema_rt::{cat, pair_pat, rt, str_lit};
use super::std_schema_rt_gen_kind::draw_kind_expr;
use super::{Expr, Pat, Stmt};

pub(super) fn needs_gen(b: &ExprBuilder, what: Expr) -> Expr {
    b.app(
        rt(b, "Schema.Defect"),
        cat(
            b,
            str_lit(b, "RangeError: Schema.arbitrary: "),
            cat(
                b,
                what,
                str_lit(b, " needs an explicit generator (Schema.with_generator)"),
            ),
        ),
    )
}

#[expect(
    clippy::too_many_lines,
    reason = "closed Schema.arbitrary outer dispatch template"
)]
pub(super) fn draw_schema_helper(b: &ExprBuilder) -> Expr {
    let step = |seed: Expr| b.app(rt(b, "Gen.Step"), seed);
    let fl_int = |x: Expr| {
        b.app(
            b.global("Int", "truncate"),
            b.app(b.global("Float", "floor"), x),
        )
    };
    let mul_f = |a: Expr, c: Expr| b.bin(BinOp::Arith(Arith::Mul, Num::Float), a, c);
    let kc = |c: &str, n: usize| ExprBuilder::con("Schema", "$Kind", c, n);

    let unwrap_refined = b.while_(
        b.var("go"),
        b.match_(
            b.app(rt(b, "Helper.schema_kind"), b.var("cur_base")),
            vec![
                arm(
                    Pat::Con(
                        kc("$Refined", 3),
                        vec![
                            Pat::Bind(b.name("inm")),
                            Pat::Bind(b.name("ibs")),
                            Pat::Wild,
                        ],
                    ),
                    b.block(
                        vec![
                            b.assign("cur_name", b.loc("inm")),
                            b.assign("cur_base", b.loc("ibs")),
                        ],
                        b.unit(),
                    ),
                ),
                arm(
                    Pat::Wild,
                    b.block(vec![b.assign("go", b.bool_(false))], b.unit()),
                ),
            ],
        ),
    );
    let refined_expr = b.block(
        vec![
            b.var_decl("cur_name", b.loc("rn")),
            b.var_decl("cur_base", b.loc("rb")),
            b.var_decl("go", b.bool_(true)),
            Stmt::Expr(unwrap_refined),
        ],
        needs_gen(b, cat(b, str_lit(b, "the check of "), b.var("cur_name"))),
    );
    let bump_enum_post = Stmt::Expr(b.if_(
        b.bin(BinOp::Le, b.add_i(b.loc("depth"), b.int(1)), b.int(3)),
        b.match_(
            step(b.var("s3")),
            vec![arm(
                pair_pat(b, "sb", "_"),
                b.block(vec![b.assign("s3", b.loc("sb"))], b.unit()),
            )],
        ),
        b.unit(),
    ));
    let pick_enum = b.block(
        vec![
            b.let_(
                "vlen",
                b.app(b.global("List.Core", "length"), b.loc("vals")),
            ),
            b.let_(
                "v",
                b.app2(
                    rt(b, "Helper.list_at"),
                    fl_int(mul_f(
                        b.loc("r"),
                        b.app(b.global("Int", "to_float"), b.loc("vlen")),
                    )),
                    b.loc("vals"),
                ),
            ),
            b.var_decl("s3", b.loc("s2")),
            bump_enum_post,
        ],
        b.tuple(vec![b.var("s3"), b.loc("v")]),
    );
    let bump_enum_pre = Stmt::Expr(b.if_(
        b.bin(BinOp::Le, b.loc("depth"), b.int(3)),
        b.match_(
            step(b.var("st")),
            vec![arm(
                pair_pat(b, "sa", "_"),
                b.block(vec![b.assign("st", b.loc("sa"))], b.unit()),
            )],
        ),
        b.unit(),
    ));
    let enum_expr = b.block(
        vec![b.var_decl("st", b.loc("seed")), bump_enum_pre],
        b.match_(
            step(b.var("st")),
            vec![arm(pair_pat(b, "s2", "r"), pick_enum)],
        ),
    );
    let size_draw = b.if_(
        b.bin(BinOp::Le, b.loc("depth"), b.int(3)),
        b.match_(
            step(b.loc("seed")),
            vec![arm(
                pair_pat(b, "ns", "r"),
                b.tuple(vec![b.loc("ns"), fl_int(mul_f(b.loc("r"), b.float(4.0)))]),
            )],
        ),
        b.tuple(vec![b.loc("seed"), b.int(0)]),
    );
    let after_size = draw_kind_expr(b);
    let kind_match = b.match_(
        b.loc("kind"),
        vec![
            arm(
                Pat::Con(kc("$Int", 0), vec![]),
                needs_gen(b, str_lit(b, "a check")),
            ),
            arm(
                Pat::Con(
                    kc("$Refined", 3),
                    vec![Pat::Bind(b.name("rn")), Pat::Bind(b.name("rb")), Pat::Wild],
                ),
                refined_expr,
            ),
            arm(
                Pat::Con(kc("$Needs", 1), vec![Pat::Bind(b.name("what"))]),
                needs_gen(b, b.loc("what")),
            ),
            arm(
                Pat::Con(kc("$Enum", 1), vec![Pat::Bind(b.name("vals"))]),
                enum_expr,
            ),
            arm(
                Pat::Wild,
                b.match_(size_draw, vec![arm(pair_pat(b, "s1", "sz"), after_size)]),
            ),
        ],
    );
    b.lam(
        &["sch", "depth", "seed"],
        b.match_(
            b.app(rt(b, "Helper.schema_gen"), b.loc("sch")),
            vec![
                arm(b.some_pat("g"), b.app(b.loc("g"), b.loc("seed"))),
                arm(
                    Pat::Wild,
                    b.block(
                        vec![b.let_("kind", b.app(rt(b, "Helper.schema_kind"), b.loc("sch")))],
                        kind_match,
                    ),
                ),
            ],
        ),
    )
}
