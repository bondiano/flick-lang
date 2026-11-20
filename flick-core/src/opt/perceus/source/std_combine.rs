//! Audited `Combiner` and `Reducer` pure IR adapters (`packages/std/src/{Combiner,Reducer}.fl`).

use flick_ir::{Arith, BinOp, Foreign, ForeignMode, Num};

use super::expr_builder::{ExprBuilder, arm};
use super::lazy_scope::has_standard_shape;
use super::std_functional::str_lit;
use super::{Code, Env, Expr, ExprKind, Local, Lower, LowerError, Name, Pat, Span, Stmt};

impl Lower<'_> {
    pub(super) fn std_combine_foreign(
        &mut self,
        value: &Foreign,
        name: &Name,
        span: Span,
        block: &mut Code,
    ) -> Option<Result<Local, LowerError>> {
        if value.mode != ForeignMode::Pure || !has_standard_shape(value) {
            return None;
        }
        let from = value.binding.from.as_deref()?;
        let bind = value.binding.name.as_str();
        let stages = value.stages.len();
        let b = ExprBuilder::new(span, "$comb");

        // `Combiner.struct` and `Reducer.struct` carry 1 `Record.Map` dictionary.
        if from == "flick:Struct" && value.dicts == 1 && stages == 1 {
            let expr = match bind {
                "flickMakeCombiner" => Some(struct_combiner(&b)),
                "flickMakeReducer" => Some(struct_reducer(&b)),
                _ => None,
            }?;
            return Some(self.expr(&expr, &Env::new(), block));
        }
        if value.dicts != 0 {
            return None;
        }
        let expr = combiner_template(&b, from, bind, stages, &name.name)
            .or_else(|| reducer_template(&b, from, bind, stages, &name.name))?;
        Some(self.expr(&expr, &Env::new(), block))
    }
}

pub(super) fn make_combiner_val(b: &ExprBuilder, f: Expr) -> Expr {
    b.app(
        b.at(ExprKind::Con(ExprBuilder::con(
            "Combiner",
            "Combiner",
            "$Combiner",
            1,
        ))),
        f,
    )
}

pub(super) fn make_reducer_val(
    b: &ExprBuilder,
    f: Expr,
    initial: Expr,
    custom_all: Option<Expr>,
) -> Expr {
    let all_fn = custom_all.unwrap_or_else(|| {
        b.lam(
            &["xs"],
            b.app3(
                b.global("List.Core", "fold_left"),
                f.clone(),
                initial.clone(),
                b.loc("xs"),
            ),
        )
    });
    b.app3(
        b.at(ExprKind::Con(ExprBuilder::con(
            "Reducer", "Reducer", "$Reducer", 3,
        ))),
        f,
        initial,
        all_fn,
    )
}

fn comb_fn(b: &ExprBuilder, c: Expr) -> Expr {
    b.match_(
        c,
        vec![
            arm(
                Pat::Con(
                    ExprBuilder::con("Combiner", "Combiner", "$Combiner", 1),
                    vec![Pat::Bind(b.name("cf"))],
                ),
                b.loc("cf"),
            ),
            arm(
                Pat::Con(
                    ExprBuilder::con("Reducer", "Reducer", "$Reducer", 3),
                    vec![Pat::Bind(b.name("rf")), Pat::Wild, Pat::Wild],
                ),
                b.loc("rf"),
            ),
        ],
    )
}

fn red_init(b: &ExprBuilder, r: Expr) -> Expr {
    b.match_(
        r,
        vec![arm(
            Pat::Con(
                ExprBuilder::con("Reducer", "Reducer", "$Reducer", 3),
                vec![Pat::Wild, Pat::Bind(b.name("ri")), Pat::Wild],
            ),
            b.loc("ri"),
        )],
    )
}

fn red_all(b: &ExprBuilder, r: Expr) -> Expr {
    b.match_(
        r,
        vec![arm(
            Pat::Con(
                ExprBuilder::con("Reducer", "Reducer", "$Reducer", 3),
                vec![Pat::Wild, Pat::Wild, Pat::Bind(b.name("ra"))],
            ),
            b.loc("ra"),
        )],
    )
}

fn combiner_template(
    b: &ExprBuilder,
    from: &str,
    bind: &str,
    stages: usize,
    item_name: &str,
) -> Option<Expr> {
    match (from, bind, stages) {
        ("flick:Combiner", "make", 1) => Some(b.lam(&["f"], make_combiner_val(b, b.loc("f")))),
        ("flick:Combiner", "flickCombine", 3) => Some(b.lam(
            &["c", "x", "y"],
            b.app2(comb_fn(b, b.loc("c")), b.loc("x"), b.loc("y")),
        )),
        ("flick:Combiner", "flip", 1) => Some(b.lam(
            &["c"],
            make_combiner_val(
                b,
                b.lam(
                    &["x", "y"],
                    b.app2(comb_fn(b, b.loc("c")), b.loc("y"), b.loc("x")),
                ),
            ),
        )),
        ("flick:Combiner", "flickMin", 1) => Some(combiner_extremum(b, "GT")),
        ("flick:Combiner", "flickMax", 1) => Some(combiner_extremum(b, "LT")),
        ("flick:Combiner", "first", 1) => Some(b.lam(
            &["_u"],
            make_combiner_val(b, b.lam(&["x", "_y"], b.loc("x"))),
        )),
        ("flick:Combiner", "last", 1) => Some(b.lam(
            &["_u"],
            make_combiner_val(b, b.lam(&["_x", "y"], b.loc("y"))),
        )),
        ("flick:Combiner", "constant", 1) => Some(b.lam(
            &["v"],
            make_combiner_val(b, b.lam(&["_x", "_y"], b.loc("v"))),
        )),
        ("flick:Combiner", "intercalate", 2) => Some(b.lam(
            &["mid", "c"],
            make_combiner_val(
                b,
                b.lam(
                    &["x", "y"],
                    b.block(
                        vec![b.let_("f", comb_fn(b, b.loc("c")))],
                        b.app2(
                            b.loc("f"),
                            b.loc("x"),
                            b.app2(b.loc("f"), b.loc("mid"), b.loc("y")),
                        ),
                    ),
                ),
            ),
        )),
        ("flick:BigInt", "CombinerMax", 0) => Some(make_combiner_val(
            b,
            b.lam(
                &["x", "y"],
                b.app2(b.global("BigInt", "max"), b.loc("y"), b.loc("x")),
            ),
        )),
        ("flick:BigInt", "CombinerMin", 0) => Some(make_combiner_val(
            b,
            b.lam(
                &["x", "y"],
                b.app2(b.global("BigInt", "min"), b.loc("y"), b.loc("x")),
            ),
        )),
        ("flick:Option", "makeCombinerFailFast", 1) => Some(b.lam(
            &["c"],
            make_combiner_val(b, option_fail_fast_fn(b, b.loc("c"))),
        )),
        ("flick:Tuple", "makeCombiner", 1) => Some(tuple_combiner(b, item_name == "tuple3")),
        _ => None,
    }
}

fn combiner_extremum(b: &ExprBuilder, lose_ord: &str) -> Expr {
    b.lam(
        &["cmp"],
        make_combiner_val(
            b,
            b.lam(
                &["x", "y"],
                b.if_(
                    b.bin(
                        BinOp::Eq,
                        b.app2(b.loc("cmp"), b.loc("x"), b.loc("y")),
                        b.ord(lose_ord),
                    ),
                    b.loc("y"),
                    b.loc("x"),
                ),
            ),
        ),
    )
}

fn reducer_template(
    b: &ExprBuilder,
    from: &str,
    bind: &str,
    stages: usize,
    item_name: &str,
) -> Option<Expr> {
    match (from, bind, stages) {
        ("flick:Reducer", "make", 2) => Some(b.lam(
            &["f", "init"],
            make_reducer_val(b, b.loc("f"), b.loc("init"), None),
        )),
        ("flick:Reducer", "flickCombine", 3) => Some(b.lam(
            &["r", "x", "y"],
            b.app2(comb_fn(b, b.loc("r")), b.loc("x"), b.loc("y")),
        )),
        ("flick:Reducer", "flickInitialValue", 1) => Some(b.lam(&["r"], red_init(b, b.loc("r")))),
        ("flick:Reducer", "flickCombineAll", 2) => {
            Some(b.lam(&["r", "xs"], b.app(red_all(b, b.loc("r")), b.loc("xs"))))
        }
        ("flick:Reducer", "flickToCombiner", 1) => {
            Some(b.lam(&["r"], make_combiner_val(b, comb_fn(b, b.loc("r")))))
        }
        ("flick:Reducer", "flip", 1) => Some(b.lam(
            &["r"],
            make_reducer_val(
                b,
                b.lam(
                    &["x", "y"],
                    b.app2(comb_fn(b, b.loc("r")), b.loc("y"), b.loc("x")),
                ),
                red_init(b, b.loc("r")),
                None,
            ),
        )),
        ("flick:Option", "makeReducer", 1) => Some(option_reducer(b)),
        ("flick:Option", "makeReducerFailFast", 1) => Some(option_reducer_fail_fast(b)),
        ("flick:Record", "makeReducerUnion", 1) => Some(record_reducer(b, true)),
        ("flick:Record", "makeReducerIntersection", 1) => Some(record_reducer(b, false)),
        ("flick:Tuple", "makeReducer", 1) => Some(tuple_reducer(b, item_name == "tuple3")),
        _ => reducer_builtins(b, from, bind, stages),
    }
}

fn reducer_builtins(b: &ExprBuilder, from: &str, bind: &str, stages: usize) -> Option<Expr> {
    match (from, bind, stages) {
        ("flick:Number", "ReducerSum", 0) => Some(make_reducer_val(
            b,
            b.lam(
                &["x", "y"],
                b.bin(BinOp::Arith(Arith::Add, Num::Float), b.loc("x"), b.loc("y")),
            ),
            b.float(0.0),
            None,
        )),
        ("flick:Number", "ReducerMultiply", 0) => Some(make_reducer_val(
            b,
            b.lam(
                &["x", "y"],
                b.bin(BinOp::Arith(Arith::Mul, Num::Float), b.loc("x"), b.loc("y")),
            ),
            b.float(1.0),
            Some(b.global("Float", "multiply_all")),
        )),
        ("flick:Number", "ReducerMax", 0) => Some(make_reducer_val(
            b,
            b.lam(
                &["x", "y"],
                b.app2(b.global("Float", "max"), b.loc("x"), b.loc("y")),
            ),
            b.float(f64::NEG_INFINITY),
            None,
        )),
        ("flick:Number", "ReducerMin", 0) => Some(make_reducer_val(
            b,
            b.lam(
                &["x", "y"],
                b.app2(b.global("Float", "min"), b.loc("x"), b.loc("y")),
            ),
            b.float(f64::INFINITY),
            None,
        )),
        ("flick:BigInt", "ReducerSum", 0) => Some(make_reducer_val(
            b,
            b.lam(
                &["x", "y"],
                b.bin(
                    BinOp::Arith(Arith::Add, Num::BigInt),
                    b.loc("x"),
                    b.loc("y"),
                ),
            ),
            b.bigint("0"),
            None,
        )),
        ("flick:BigInt", "ReducerMultiply", 0) => Some(make_reducer_val(
            b,
            b.lam(
                &["x", "y"],
                b.bin(
                    BinOp::Arith(Arith::Mul, Num::BigInt),
                    b.loc("x"),
                    b.loc("y"),
                ),
            ),
            b.bigint("1"),
            Some(b.global("BigInt", "multiply_all")),
        )),
        ("flick:String", "ReducerConcat", 0) => Some(make_reducer_val(
            b,
            b.lam(&["x", "y"], b.bin(BinOp::Concat, b.loc("x"), b.loc("y"))),
            str_lit(b, ""),
            None,
        )),
        ("flick:Array", "getReadonlyReducerConcat", 1) => Some(b.lam(
            &["_u"],
            make_reducer_val(
                b,
                b.lam(
                    &["x", "y"],
                    b.bin(BinOp::ListConcat, b.loc("x"), b.loc("y")),
                ),
                b.list(vec![]),
                None,
            ),
        )),
        _ => None,
    }
}

fn option_fail_fast_fn(b: &ExprBuilder, comb_expr: Expr) -> Expr {
    b.lam(
        &["ox", "oy"],
        b.match_(
            b.loc("ox"),
            vec![
                arm(
                    b.some_pat("vx"),
                    b.match_(
                        b.loc("oy"),
                        vec![
                            arm(
                                b.some_pat("vy"),
                                b.some(b.app2(comb_fn(b, comb_expr), b.loc("vx"), b.loc("vy"))),
                            ),
                            arm(Pat::Wild, b.none()),
                        ],
                    ),
                ),
                arm(Pat::Wild, b.none()),
            ],
        ),
    )
}

fn option_reducer(b: &ExprBuilder) -> Expr {
    b.lam(
        &["c"],
        make_reducer_val(
            b,
            b.lam(
                &["ox", "oy"],
                b.match_(
                    b.loc("ox"),
                    vec![
                        arm(
                            b.some_pat("vx"),
                            b.match_(
                                b.loc("oy"),
                                vec![
                                    arm(
                                        b.some_pat("vy"),
                                        b.some(b.app2(
                                            comb_fn(b, b.loc("c")),
                                            b.loc("vx"),
                                            b.loc("vy"),
                                        )),
                                    ),
                                    arm(Pat::Wild, b.loc("ox")),
                                ],
                            ),
                        ),
                        arm(Pat::Wild, b.loc("oy")),
                    ],
                ),
            ),
            b.none(),
            None,
        ),
    )
}

fn option_reducer_fail_fast(b: &ExprBuilder) -> Expr {
    b.lam(
        &["r"],
        make_reducer_val(
            b,
            option_fail_fast_fn(b, b.loc("r")),
            b.some(red_init(b, b.loc("r"))),
            None,
        ),
    )
}

fn record_reducer(b: &ExprBuilder, is_union: bool) -> Expr {
    let op_name = if is_union { "union" } else { "intersection" };
    b.lam(
        &["c"],
        make_reducer_val(
            b,
            b.lam(
                &["a", "b"],
                b.app3(
                    b.global("Record", op_name),
                    b.loc("b"),
                    comb_fn(b, b.loc("c")),
                    b.loc("a"),
                ),
            ),
            b.app(b.global("Record", "empty"), b.unit()),
            None,
        ),
    )
}

fn struct_combiner_fn(b: &ExprBuilder) -> Expr {
    let get = |k: Expr, rec: Expr| {
        b.match_(
            b.app2(b.global("Record", "get_key"), k, rec),
            vec![arm(b.some_pat("fv"), b.loc("fv"))],
        )
    };
    let kv_pat = Pat::Tuple(vec![Pat::Bind(b.name("k")), Pat::Bind(b.name("fc"))]);
    b.lam(
        &["a", "b"],
        b.app(
            b.global("Record", "from_entries"),
            b.gen_strict(b.each(
                "pair",
                b.app(b.global("Record", "to_entries"), b.loc("cs")),
                b.match_(
                    b.loc("pair"),
                    vec![arm(
                        kv_pat,
                        b.yield_(b.tuple(vec![
                            b.loc("k"),
                            b.app2(
                                comb_fn(b, b.loc("fc")),
                                get(b.loc("k"), b.loc("a")),
                                get(b.loc("k"), b.loc("b")),
                            ),
                        ])),
                    )],
                ),
            )),
        ),
    )
}

fn struct_combiner(b: &ExprBuilder) -> Expr {
    b.lam(
        &["_labels", "cs"],
        make_combiner_val(b, struct_combiner_fn(b)),
    )
}

fn struct_reducer(b: &ExprBuilder) -> Expr {
    let init_rec = b.app2(
        b.global("Record", "map"),
        b.lam(&["fr", "_k"], red_init(b, b.loc("fr"))),
        b.loc("cs"),
    );
    b.lam(
        &["_labels", "cs"],
        make_reducer_val(b, struct_combiner_fn(b), init_rec, None),
    )
}

fn tuple_combiner(b: &ExprBuilder, triple: bool) -> Expr {
    let pat = |names: &[&str]| Pat::Tuple(names.iter().map(|n| Pat::Bind(b.name(n))).collect());
    let (c_names, a_names, b_names): (&[&str], &[&str], &[&str]) = if triple {
        (
            &["c1", "c2", "c3"],
            &["a1", "a2", "a3"],
            &["b1", "b2", "b3"],
        )
    } else {
        (&["c1", "c2"], &["a1", "a2"], &["b1", "b2"])
    };
    let fields: Vec<Expr> = c_names
        .iter()
        .zip(a_names)
        .zip(b_names)
        .map(|((c, a), bv)| b.app2(comb_fn(b, b.loc(c)), b.loc(a), b.loc(bv)))
        .collect();
    b.lam(
        &["cs"],
        b.match_(
            b.loc("cs"),
            vec![arm(
                pat(c_names),
                make_combiner_val(
                    b,
                    b.lam(
                        &["x", "y"],
                        b.match_(
                            b.loc("x"),
                            vec![arm(
                                pat(a_names),
                                b.match_(b.loc("y"), vec![arm(pat(b_names), b.tuple(fields))]),
                            )],
                        ),
                    ),
                ),
            )],
        ),
    )
}

fn tuple_reducer(b: &ExprBuilder, triple: bool) -> Expr {
    let pat = |names: &[&str]| Pat::Tuple(names.iter().map(|n| Pat::Bind(b.name(n))).collect());
    let (c_names, a_names, b_names): (&[&str], &[&str], &[&str]) = if triple {
        (
            &["c1", "c2", "c3"],
            &["a1", "a2", "a3"],
            &["b1", "b2", "b3"],
        )
    } else {
        (&["c1", "c2"], &["a1", "a2"], &["b1", "b2"])
    };
    let inits: Vec<Expr> = c_names.iter().map(|c| red_init(b, b.loc(c))).collect();
    let fields: Vec<Expr> = c_names
        .iter()
        .zip(a_names)
        .zip(b_names)
        .map(|((c, a), bv)| b.app2(comb_fn(b, b.loc(c)), b.loc(a), b.loc(bv)))
        .collect();
    let body = b.block(
        vec![Stmt::Let(pat(c_names), b.loc("rs"))],
        make_reducer_val(
            b,
            b.lam(
                &["x", "y"],
                b.match_(
                    b.loc("x"),
                    vec![arm(
                        pat(a_names),
                        b.match_(b.loc("y"), vec![arm(pat(b_names), b.tuple(fields))]),
                    )],
                ),
            ),
            b.tuple(inits),
            None,
        ),
    );
    b.lam(&["rs"], body)
}
