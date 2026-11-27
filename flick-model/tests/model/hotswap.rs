//! Unit tests for hot-swap scheme compatibility and global-cell swap
//! semantics (`docs/formal/hot-swap.md`, `formal/Formal/HotSwap.lean`).

use std::collections::BTreeMap;
use std::rc::Rc;

use flick_core::numeric::{Arith, Num};
use flick_model::check::{check_scheme_compatibility, check_swap_value};
use flick_model::hotswap::{
    GlobalDecl, LiveSession, ModuleDef, SessionOutcomeSummary, StepResult, SwapRequest,
    summarize_outcome,
};
use flick_model::syntax::{Const, Lit, Path, Prim, Term};
use flick_model::types::{Caps, Fields, Kind, Row, Scheme, Stage, Type, Var};

use crate::props::GEN;

fn hot(name: &str) -> Path {
    Path::new("Hot", name)
}

fn int_ty() -> Type {
    Type::con("Prelude", "Int")
}

fn unit_ty() -> Type {
    Type::con("Prelude", "Unit")
}

fn pair_ty(a: Type, b: Type) -> Type {
    Type::Tuple(Rc::from([a, b]))
}

fn owner_ty(region: &str) -> Type {
    Type::app(
        Type::con("Fiber", "Owner"),
        Type::Var(Var::Named(region.into())),
    )
}

fn mono(body: Type) -> Scheme {
    Scheme::new(Vec::new(), body)
}

fn add_int(a: Term, b: Term) -> Term {
    Term::app(
        Term::app(
            Term::konst(Const::Prim(Prim::Arith(Arith::Add, Num::Int))),
            a,
        ),
        b,
    )
}

fn unit_thunk(caps: Caps) -> Type {
    let arrow = Type::fun(
        unit_ty(),
        Stage::Runtime(Row::none(), Row::none()),
        unit_ty(),
    );
    if caps.is_empty() {
        arrow
    } else {
        Type::Captured(Rc::new(arrow), caps)
    }
}

#[test]
fn compatibility_accepts_generalization_and_rejects_narrowing() {
    GEN.with(|(decls, _)| {
        let mono_int = mono(Type::fun(int_ty(), Stage::Pure, int_ty()));
        let poly_id = Scheme::new(
            vec![("a".into(), Kind::Type)],
            Type::fun(
                Type::Var(Var::Named("a".into())),
                Stage::Pure,
                Type::Var(Var::Named("a".into())),
            ),
        );
        assert!(
            check_scheme_compatibility(decls, &poly_id, &mono_int).is_ok(),
            "generalizing Int -> Int to forall a. a -> a must succeed"
        );
        assert!(
            check_scheme_compatibility(decls, &mono_int, &poly_id).is_err(),
            "narrowing forall a. a -> a to Int -> Int must fail"
        );
    });
}

#[test]
fn compatibility_rejects_effect_widening_and_accepts_effect_narrowing() {
    GEN.with(|(decls, _)| {
        let log = Row::one(Path::new("Fixture", "Log"));
        let counter = Row::one(Path::new("Fixture", "Counter"));
        let log_and_counter = log.sum(&counter).expect("distinct services");
        let oops = Row::one(Path::new("Fixture", "Oops"));
        let bad = Row::one(Path::new("Fixture", "Bad"));
        let oops_and_bad = oops.sum(&bad).expect("distinct errors");

        let pure_fn = mono(Type::fun(int_ty(), Stage::Pure, int_ty()));
        let rt_empty = mono(Type::fun(
            int_ty(),
            Stage::Runtime(Row::none(), Row::none()),
            int_ty(),
        ));
        let rt_log = mono(Type::fun(
            int_ty(),
            Stage::Runtime(log.clone(), Row::none()),
            int_ty(),
        ));
        let rt_log_counter = mono(Type::fun(
            int_ty(),
            Stage::Runtime(log_and_counter, Row::none()),
            int_ty(),
        ));
        let rt_oops = mono(Type::fun(
            int_ty(),
            Stage::Runtime(Row::none(), oops.clone()),
            int_ty(),
        ));
        let rt_oops_bad = mono(Type::fun(
            int_ty(),
            Stage::Runtime(Row::none(), oops_and_bad),
            int_ty(),
        ));
        let poly_row = Scheme::new(
            vec![("r".into(), Kind::Services)],
            Type::fun(
                int_ty(),
                Stage::Runtime(Row::var(Var::Named("r".into())), Row::none()),
                int_ty(),
            ),
        );

        // Effect widening rejected:
        assert!(check_scheme_compatibility(decls, &rt_empty, &pure_fn).is_err());
        assert!(check_scheme_compatibility(decls, &rt_log, &pure_fn).is_err());
        assert!(check_scheme_compatibility(decls, &rt_log_counter, &rt_log).is_err());
        assert!(check_scheme_compatibility(decls, &rt_oops_bad, &rt_oops).is_err());
        assert!(check_scheme_compatibility(decls, &rt_log, &poly_row).is_err());

        // Effect narrowing & row generalization accepted:
        assert!(check_scheme_compatibility(decls, &pure_fn, &rt_empty).is_ok());
        assert!(check_scheme_compatibility(decls, &pure_fn, &rt_log).is_ok());
        assert!(check_scheme_compatibility(decls, &rt_log, &rt_log_counter).is_ok());
        assert!(check_scheme_compatibility(decls, &rt_oops, &rt_oops_bad).is_ok());
        assert!(check_scheme_compatibility(decls, &poly_row, &rt_log).is_ok());
    });
}

#[test]
fn compatibility_checks_region_and_capture_qualifiers() {
    GEN.with(|(decls, _)| {
        let param_s = Scheme::new(
            vec![("s".into(), Kind::Region)],
            Type::fun(
                unit_thunk(Caps::atom(Var::Named("s".into()))),
                Stage::Runtime(Row::none(), Row::none()),
                unit_ty(),
            ),
        );
        let param_empty = mono(Type::fun(
            unit_thunk(Caps::default()),
            Stage::Runtime(Row::none(), Row::none()),
            unit_ty(),
        ));
        let param_poly_c = Scheme::new(
            vec![("c".into(), Kind::Captures)],
            Type::fun(
                unit_thunk(Caps::var(Var::Named("c".into()))),
                Stage::Runtime(Row::none(), Row::none()),
                unit_ty(),
            ),
        );

        // Narrowing parameter capture qualifier `^{s}` -> `^∅` rejected:
        assert!(check_scheme_compatibility(decls, &param_empty, &param_s).is_err());
        // Widening/generalizing parameter capture qualifier `^∅` -> `^c` accepted:
        assert!(check_scheme_compatibility(decls, &param_poly_c, &param_empty).is_ok());
        // Generalizing `^{s}` -> `^c` accepted:
        assert!(check_scheme_compatibility(decls, &param_poly_c, &param_s).is_ok());

        let thunk_rec = |caps: Caps| {
            let mut fields = BTreeMap::new();
            fields.insert("run".into(), unit_thunk(caps));
            Type::Record(Fields { fields, tail: None })
        };
        let ret_empty = Scheme::new(
            vec![("s".into(), Kind::Region)],
            Type::fun(owner_ty("s"), Stage::Pure, thunk_rec(Caps::default())),
        );
        let ret_s = Scheme::new(
            vec![("s".into(), Kind::Region)],
            Type::fun(
                owner_ty("s"),
                Stage::Pure,
                thunk_rec(Caps::atom(Var::Named("s".into()))),
            ),
        );

        // Widening returned callable's capture qualifier `^∅` -> `^{s}` rejected:
        assert!(check_scheme_compatibility(decls, &ret_s, &ret_empty).is_err());
        // Shrinking returned callable's capture qualifier `^{s}` -> `^∅` accepted:
        assert!(check_scheme_compatibility(decls, &ret_empty, &ret_s).is_ok());
    });
}

#[test]
fn swap_rejects_open_or_non_value_replacements() {
    GEN.with(|(decls, _)| {
        let int_to_int = mono(Type::fun(int_ty(), Stage::Pure, int_ty()));
        // Open lambda mentioning unbound variable `y` must be rejected:
        let open_lam = Term::lam("x", add_int(Term::Var("x".into()), Term::Var("y".into())));
        assert!(check_swap_value(decls, &hot("f"), &int_to_int, &open_lam).is_err());
        // Non-value expression must be rejected:
        let non_val = Term::app(
            Term::lam("x", Term::Var("x".into())),
            Term::lam("z", Term::Var("z".into())),
        );
        assert!(check_swap_value(decls, &hot("f"), &int_to_int, &non_val).is_err());
    });
}

#[test]
fn old_frame_keeps_captured_code_while_new_calls_enter_swapped_code() {
    GEN.with(|(decls, program)| {
        let mut session = LiveSession::new(decls.clone(), program.clone(), 10_000);
        let int_to_int = mono(Type::fun(int_ty(), Stage::Pure, int_ty()));

        let module = ModuleDef {
            name: "Hot".into(),
            globals: vec![
                GlobalDecl {
                    path: hot("f"),
                    scheme: int_to_int.clone(),
                    value: Term::lam("x", add_int(Term::Var("x".into()), Term::Lit(Lit::Int(1)))),
                },
                GlobalDecl {
                    path: hot("run"),
                    scheme: mono(Type::fun(
                        int_ty(),
                        Stage::Pure,
                        pair_ty(int_ty(), int_ty()),
                    )),
                    value: Term::lam(
                        "x",
                        Term::Let(
                            "old_f".into(),
                            None,
                            Rc::new(Term::Global(hot("f"))),
                            Rc::new(Term::Let(
                                "a".into(),
                                None,
                                Rc::new(Term::app(
                                    Term::lam(
                                        "y",
                                        Term::app(Term::Var("old_f".into()), Term::Var("y".into())),
                                    ),
                                    Term::Var("x".into()),
                                )),
                                Rc::new(Term::Let(
                                    "b".into(),
                                    None,
                                    Rc::new(Term::app(
                                        Term::Global(hot("f")),
                                        Term::Var("x".into()),
                                    )),
                                    Rc::new(Term::Tuple(Rc::from([
                                        Term::Var("a".into()),
                                        Term::Var("b".into()),
                                    ]))),
                                )),
                            )),
                        ),
                    ),
                },
            ],
            state_cells: Vec::new(),
        };
        session.load_module(module).expect("module loads");

        let entry = Term::app(Term::Global(hot("run")), Term::Lit(Lit::Int(10)));
        session
            .start_entry(entry, pair_ty(int_ty(), int_ty()))
            .expect("entry types");

        // Advance 10 steps: `old_f` has captured v1 (`x + 1`) and is evaluating `a`,
        // while `Hot.f` for `b` has NOT been dereferenced yet.
        let step = session.step_n(10, true).expect("steps preserve typing");
        assert!(
            matches!(step, StepResult::Running { .. }),
            "entry must still be paused mid-execution"
        );

        // Hot-swap `Hot.f` to v2 (`x + 100`) with a compatible scheme.
        session
            .swap(SwapRequest {
                target: hot("f"),
                new_scheme: int_to_int,
                new_value: Term::lam(
                    "x",
                    add_int(Term::Var("x".into()), Term::Lit(Lit::Int(100))),
                ),
            })
            .expect("compatible swap succeeds");
        session
            .check_configuration()
            .expect("paused configuration stays well typed after swap");

        // Resume to completion: `a` runs old captured code (11), `b` runs new code (110).
        let outcome = session
            .run_to_end(true)
            .expect("post-swap steps preserve typing")
            .expect("entry finishes");
        assert_eq!(
            summarize_outcome(&outcome),
            SessionOutcomeSummary::Value("(11, 110)".into())
        );
    });
}
