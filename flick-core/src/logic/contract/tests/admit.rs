use std::collections::{HashMap, HashSet};

use flick_ir::{Arith, BinOp, Dict, ExprKind, ItemKind, Num, Param, Pat as IrPat, Stage};

use super::super::{
    ArithOp, ContractDepNode, ContractFailure, ContractOrder, Counterexample, admit_v2,
    collect_ir_contract_deps, extract, order_contracts,
};
use super::{
    FakeProgram, bin, bool_lit, check_and_lower, expr, global, int_lit, lang, local, span, span_at,
    ty_name,
};
use crate::diagnostics::codes;
use crate::logic::{Def, Refused, Ty};
use crate::source::SourceMap;

fn sample_admission_program() -> FakeProgram {
    let mut prog = FakeProgram::default();
    // Uncontracted helper `bump` calls contracted `consume`
    prog.add_value(
        "App",
        "bump",
        bin(
            BinOp::Arith(Arith::Add, Num::Int),
            global("App", "consume"),
            int_lit(1),
        ),
    );
    // Contracted `consume` has an unsupported op to prove `admit_v2` never unfolds it
    prog.add_value(
        "App",
        "consume",
        bin(BinOp::Arith(Arith::Mul, Num::Int), int_lit(2), int_lit(3)),
    );
    let int_params = || {
        vec![
            Param {
                pat: IrPat::Bind("a".into()),
                stage: Stage::Pure,
                ty: Some(ty_name("Prelude", "Int")),
                binder_types: Vec::new(),
            },
            Param {
                pat: IrPat::Bind("b".into()),
                stage: Stage::Pure,
                ty: Some(ty_name("Prelude", "Int")),
                binder_types: Vec::new(),
            },
        ]
    };
    prog.add_instance(
        "Numeric",
        "Numeric Int",
        ("Numeric", "Numeric"),
        vec![
            (
                "add".into(),
                expr(ExprKind::Lambda {
                    params: int_params(),
                    body: Box::new(bin(
                        BinOp::Arith(Arith::Add, Num::Int),
                        local("a"),
                        local("b"),
                    )),
                }),
            ),
            (
                "mul".into(),
                expr(ExprKind::Lambda {
                    params: int_params(),
                    body: Box::new(bin(
                        BinOp::Arith(Arith::Mul, Num::Int),
                        local("a"),
                        local("b"),
                    )),
                }),
            ),
        ],
    );
    prog.add_instance(
        "App",
        "Eq Fake",
        ("Prelude", "Eq"),
        vec![("eq".into(), bool_lit(true))],
    );
    prog.add_instance(
        "App",
        "Ord Fake",
        ("Prelude", "Ord"),
        vec![("compare".into(), int_lit(0))],
    );
    prog.add_foreign("App", "native_op");
    prog.add_dict_value("App", "poly_helper", int_lit(0));
    prog.add_value("App", "loop_a", global("App", "loop_b"));
    prog.add_value("App", "loop_b", global("App", "loop_a"));
    prog
}

#[test]
fn admit_v2_unfolds_helpers_and_stops_at_contracted() {
    let prog = sample_admission_program();
    let l = lang();
    let contracted: HashSet<(String, String)> = [("App".into(), "consume".into())].into();

    let root = extract(&global("App", "bump"), &[], &prog, &l).expect("extract bump");
    let admitted = admit_v2(&[&root], &prog, &contracted, &l, span()).expect("admit bump");
    assert!(
        admitted
            .body(&Def::Value {
                module: "App".into(),
                name: "bump".into()
            })
            .is_some()
    );
    assert!(
        admitted
            .body(&Def::Value {
                module: "App".into(),
                name: "consume".into()
            })
            .is_none()
    );
    assert_eq!(
        admitted.contracted_deps(),
        &[("App".to_owned(), "consume".to_owned())]
    );
}

#[test]
fn admit_v2_filters_instances_foreigns_dicts_and_recursion() {
    let prog = sample_admission_program();
    let l = lang();
    let contracted: HashSet<(String, String)> = [("App".into(), "consume".into())].into();

    let num_int_dict = Dict::Instance {
        module: "Numeric".into(),
        name: "Numeric Int".into(),
        args: Vec::new(),
    };
    let add_method = extract(
        &expr(ExprKind::Method {
            dict: num_int_dict.clone(),
            name: "add".into(),
            type_args: Vec::new(),
        }),
        &[],
        &prog,
        &l,
    )
    .expect("extract Numeric Int add");
    let admitted_add =
        admit_v2(&[&add_method], &prog, &contracted, &l, span()).expect("admit Numeric Int add");
    assert_eq!(admitted_add.defs().len(), 1);

    let mul_method = extract(
        &expr(ExprKind::Method {
            dict: num_int_dict,
            name: "mul".into(),
            type_args: Vec::new(),
        }),
        &[],
        &prog,
        &l,
    )
    .expect("extract Numeric Int mul");
    assert!(admit_v2(&[&mul_method], &prog, &contracted, &l, span()).is_err());

    let eq_method = extract(
        &expr(ExprKind::Method {
            dict: Dict::Instance {
                module: "App".into(),
                name: "Eq Fake".into(),
                args: Vec::new(),
            },
            name: "eq".into(),
            type_args: Vec::new(),
        }),
        &[],
        &prog,
        &l,
    )
    .expect("extract Eq Fake");
    assert!(admit_v2(&[&eq_method], &prog, &contracted, &l, span()).is_err());

    let ord_method = extract(
        &expr(ExprKind::Method {
            dict: Dict::Instance {
                module: "App".into(),
                name: "Ord Fake".into(),
                args: Vec::new(),
            },
            name: "compare".into(),
            type_args: Vec::new(),
        }),
        &[],
        &prog,
        &l,
    )
    .expect("extract Ord Fake");
    assert!(admit_v2(&[&ord_method], &prog, &contracted, &l, span()).is_err());

    for bad in ["native_op", "poly_helper", "loop_a"] {
        let t = extract(&global("App", bad), &[], &prog, &l).expect("extract global");
        assert!(
            admit_v2(&[&t], &prog, &contracted, &l, span()).is_err(),
            "expected {bad} to be refused"
        );
    }
}

#[test]
fn order_contracts_topologically_sorts_and_detects_direct_indirect_and_mutual_cycles() {
    // Acyclic: caller -> mid -> leaf; independent stays in declaration order
    let nodes = vec![
        ContractDepNode {
            name: "caller".into(),
            deps: vec!["mid".into()],
        },
        ContractDepNode {
            name: "indep".into(),
            deps: vec![],
        },
        ContractDepNode {
            name: "mid".into(),
            deps: vec!["leaf".into()],
        },
        ContractDepNode {
            name: "leaf".into(),
            deps: vec![],
        },
    ];
    assert_eq!(
        order_contracts(&nodes),
        vec![
            ContractOrder::Acyclic("leaf".into()),
            ContractOrder::Acyclic("mid".into()),
            ContractOrder::Acyclic("caller".into()),
            ContractOrder::Acyclic("indep".into()),
        ]
    );

    // Self-loop and mutual cycle + acyclic caller depending on cycle
    let cyclic = vec![
        ContractDepNode {
            name: "caller_of_cycle".into(),
            deps: vec!["a".into()],
        },
        ContractDepNode {
            name: "a".into(),
            deps: vec!["b".into()],
        },
        ContractDepNode {
            name: "b".into(),
            deps: vec!["a".into()],
        },
        ContractDepNode {
            name: "self_rec".into(),
            deps: vec!["self_rec".into()],
        },
    ];
    assert_eq!(
        order_contracts(&cyclic),
        vec![
            ContractOrder::Cyclic("a".into()),
            ContractOrder::Cyclic("b".into()),
            ContractOrder::Acyclic("caller_of_cycle".into()),
            ContractOrder::Cyclic("self_rec".into()),
        ]
    );

    // Indirect cycle via uncontracted helper (`a -> helper -> a`), plus recursive helper termination
    let mut prog = FakeProgram::default();
    prog.add_value("App", "helper", global("App", "a"));
    prog.add_value("App", "spin", global("App", "spin"));
    let contracted: HashSet<String> = ["a".into(), "b".into()].into();
    let a_body = bin(
        BinOp::Arith(Arith::Add, Num::Int),
        global("App", "helper"),
        global("App", "spin"),
    );
    let deps_a = collect_ir_contract_deps(&[&a_body], "App", &contracted, &prog);
    assert_eq!(deps_a, vec!["a".to_owned()]);
}

#[test]
fn contract_failure_diagnostics_emit_t087_t088_t089_and_t082_to_t084() {
    let ce = Some(Counterexample {
        bindings: vec![
            ("available".into(), "1".into()),
            ("used".into(), "1".into()),
        ],
    });
    let d_post = ContractFailure::Postcondition {
        span: span_at(2, 5),
        decl_span: span_at(0, 8),
        counterexample: ce.clone(),
    }
    .diagnostic("consume");
    assert_eq!(d_post.code, codes::T087);
    assert!(d_post.message.contains("`available = 1, used = 1`"));
    assert_eq!(d_post.related.len(), 1);

    let d_pre = ContractFailure::Precondition {
        span: span_at(3, 6),
        callee: "consume".into(),
        callee_require: Some(span_at(1, 2)),
        counterexample: ce,
    }
    .diagnostic("take");
    assert_eq!(d_pre.code, codes::T088);
    assert!(d_pre.message.contains("precondition of `consume`"));
    assert_eq!(d_pre.related.len(), 1);

    let d_ovf = ContractFailure::Overflow {
        span: span_at(4, 5),
        op: ArithOp::Neg,
        counterexample: Some(Counterexample {
            bindings: vec![("x".into(), "-2147483648".into())],
        }),
    }
    .diagnostic("neg");
    assert_eq!(d_ovf.code, codes::T089);
    assert!(d_ovf.message.contains("unary `-`"));
    assert!(d_ovf.message.contains("`x = -2147483648`"));

    let d_refused = ContractFailure::Refused(Refused {
        span: span_at(0, 1),
        why: "`*` is outside the domain".into(),
    })
    .diagnostic("f");
    assert_eq!(d_refused.code, codes::T082);
    assert!(d_refused.message.contains("PureTotalV2"));

    let d_inv = ContractFailure::Invalid {
        span: span_at(0, 1),
        why: "mismatch".into(),
    }
    .diagnostic("f");
    assert_eq!(d_inv.code, codes::T083);

    let d_exh = ContractFailure::Exhausted {
        span: span_at(0, 1),
    }
    .diagnostic("f");
    assert_eq!(d_exh.code, codes::T084);
}

#[test]
fn type_checker_populates_contract_info_including_rec_group() {
    let source = r"module App.C3Meta exposing (..)

type Light = Red | Green

consume : Int -> Int -> Int
let consume available used
where
  require (0 <= used && used <= available)
  ensure (fun remaining -> 0 <= remaining && remaining + used == available)
=
  available - used

next : Light -> Light
let next light where ensure (fun l -> l != light) =
  match light with
  | Red -> Green
  | Green -> Red

scale : Float -> Float
let scale x where ensure (fun y -> y == x) = x

effectful : Int ->{} Int
let effectful x where ensure (fun y -> y == x) = x

constrained : Eq a => a -> a -> Bool
let constrained x y where ensure (fun _ -> true) = x == y

rec {
  even_ish : Int -> Bool
  let even_ish n
  where
    require (0 <= n)
    ensure (fun _ -> true)
  =
    if n == 0 then true else odd_ish (n - 1)

  odd_ish : Int -> Bool
  let odd_ish n where require (0 <= n) =
    if n == 0 then false else even_ish (n - 1)
}
";
    let (typing, ir_mod) = check_and_lower("App/C3Meta.fl", source);
    let by_name: HashMap<&str, _> = typing
        .contracts
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();
    assert_eq!(typing.contracts.len(), 7);

    let consume = by_name["consume"];
    assert_eq!(consume.params, vec![Ty::Int, Ty::Int]);
    assert_eq!(consume.result, Ty::Int);
    assert!(!consume.has_dicts);
    assert!(!consume.has_effects);
    assert!(!consume.recursive);

    let constrained = by_name["constrained"];
    assert!(constrained.has_dicts);
    assert!(!constrained.has_effects);
    assert!(!constrained.recursive);

    let consume_item = ir_mod
        .items
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Value {
                name, body, dicts, ..
            } if name == "consume" => Some((dicts, body)),
            _ => None,
        })
        .expect("consume item in IR");
    assert!(consume_item.0.is_empty());
    assert!(matches!(
        &consume_item.1.kind,
        ExprKind::Lambda { body, .. }
            if matches!(&body.kind, ExprKind::Binary(BinOp::Arith(Arith::Sub, Num::Int), ..))
    ));

    let next = by_name["next"];
    assert!(matches!(
        next.params.as_slice(),
        [Ty::Data { name, .. }] if name == "Light"
    ));
    assert!(
        next.data
            .get(&("App.C3Meta".into(), "Light".into()))
            .is_some_and(|d| d.constructors.len() == 2 && !d.recursive)
    );

    let scale = by_name["scale"];
    assert!(matches!(scale.params.as_slice(), [Ty::Refused(why)] if why.contains("Float")));
    assert!(matches!(&scale.result, Ty::Refused(why) if why.contains("Float")));
    assert!(by_name["effectful"].has_effects);
    assert!(by_name["even_ish"].recursive);
    assert!(by_name["odd_ish"].recursive);
}

#[test]
fn contracts_hide_proof_rules_from_contract_expressions() {
    let bad_source = r"module App.NoProofRules exposing (..)

f : Int -> Int
let f x where require (refl == refl) = x
";
    let mut sources = SourceMap::default();
    let file = sources.add("App/NoProofRules.fl", bad_source);
    let diags = crate::check(&sources, file);
    assert!(
        diags.iter().any(|d| d.code == codes::T001),
        "expected T001 for `refl` inside contract, got: {diags:?}"
    );
}
