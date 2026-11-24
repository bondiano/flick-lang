//! The checker on statements built by hand: what it accepts, and each way
//! a certificate fails to be one.

use std::rc::Rc;
use std::slice;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::AtomicBool;

use flick_ir::Lit;

use super::{
    Cert, Ctor, Data, DataDecl, Equation, Failure, Goal, Literal, Pat, Rule, Statement, Term, Ty,
    prove,
};
#[cfg(not(target_arch = "wasm32"))]
use super::{ProofSearchRequest, ProofSearchStatus};
use crate::source::{SourceMap, Span};

/// A program without definitions.
struct Nothing;

impl super::Program for Nothing {
    fn item(&self, _: &str, _: &str) -> Option<&flick_ir::Item> {
        None
    }

    fn instance(
        &self,
        _: &str,
        _: &str,
    ) -> Option<(&flick_ir::InstanceInfo, &[(String, flick_ir::Expr)])> {
        None
    }
}

fn span() -> Span {
    Span::new(SourceMap::default().add("t.fl", ""), 0, 0)
}

fn var(x: &str) -> Term {
    Term::Var(x.into())
}

fn app(f: Term, a: Term) -> Term {
    Term::App(Rc::new(f), Rc::new(a))
}

fn lam(x: &str, b: Term) -> Term {
    Term::Lam(x.into(), Rc::new(b))
}

fn ctor(name: &str, arity: usize) -> Rc<Ctor> {
    Rc::new(Ctor {
        module: "Prelude".into(),
        ty: "Option".into(),
        name: name.into(),
        arity,
    })
}

fn some(t: Term) -> Term {
    app(Term::Con(ctor("Some", 1)), t)
}

fn none() -> Term {
    Term::Con(ctor("None", 0))
}

fn option(a: Ty) -> Ty {
    Ty::Data {
        module: "Prelude".into(),
        name: "Option".into(),
        args: vec![a],
    }
}

fn data() -> Data {
    let mut data = Data::new();
    data.insert(
        ("Prelude".into(), "Option".into()),
        DataDecl {
            params: vec!["a".into()],
            constructors: vec![
                ("None".into(), vec![]),
                ("Some".into(), vec![Ty::Rigid("a".into())]),
            ],
            recursive: false,
        },
    );
    data
}

fn cert(kind: Rule) -> Cert {
    Cert { kind, span: span() }
}

fn refl() -> Cert {
    cert(Rule::Refl)
}

fn rigid(a: &str) -> Ty {
    Ty::Rigid(a.into())
}

fn fun(a: Ty, b: Ty) -> Ty {
    Ty::Fun(Box::new(a), Box::new(b))
}

fn equal(params: Vec<(&str, Ty)>, ty: Ty, left: Term, right: Term) -> Statement {
    Statement {
        name: "p".into(),
        span: span(),
        params: params.into_iter().map(|(x, t)| (x.into(), t)).collect(),
        premises: Vec::new(),
        goal: Goal::Equal(Equation { ty, left, right }),
    }
}

fn code(found: &Result<(), Failure>) -> String {
    match found {
        Ok(()) => "ok".into(),
        Err(failure) => failure
            .diagnostic("")
            .code
            .to_string()
            .replace("FLICK-", ""),
    }
}

#[test]
fn types_are_the_same_only_when_they_are() {
    let same = |a, b| Statement {
        name: "p".into(),
        span: span(),
        params: Vec::new(),
        premises: Vec::new(),
        goal: Goal::SameType(a, b),
    };
    assert_eq!(
        code(&prove(
            &same(rigid("a"), rigid("a")),
            &refl(),
            &data(),
            &Nothing,
            &[]
        )),
        "ok"
    );
    // two variables may stand for different types: not proved, not false
    assert_eq!(
        code(&prove(
            &same(rigid("a"), rigid("b")),
            &refl(),
            &data(),
            &Nothing,
            &[]
        )),
        "T081"
    );
    assert_eq!(
        code(&prove(
            &same(Ty::Bool, option(Ty::Bool)),
            &refl(),
            &data(),
            &Nothing,
            &[]
        )),
        "T080"
    );
    let symm = cert(Rule::Symm(Box::new(refl())));
    assert_eq!(
        code(&prove(
            &same(rigid("a"), rigid("a")),
            &symm,
            &data(),
            &Nothing,
            &[]
        )),
        "T083"
    );
}

#[test]
fn functions_are_equal_by_their_values() {
    let f = fun(rigid("a"), rigid("b"));
    let eta = equal(
        vec![("f", f.clone())],
        f.clone(),
        lam("x", app(var("f"), var("x"))),
        var("f"),
    );
    // without the rule the two are different terms
    assert_eq!(code(&prove(&eta, &refl(), &data(), &Nothing, &[])), "T081");
    let funext = cert(Rule::Funext("y".into(), Box::new(refl())));
    assert_eq!(code(&prove(&eta, &funext, &data(), &Nothing, &[])), "ok");
    // a function's values at different arguments are not one value
    let pred = fun(Ty::Bool, Ty::Bool);
    let t = |b| Term::Lit(Literal(Lit::Bool(b)));
    let constant = equal(
        vec![("f", pred)],
        Ty::Bool,
        app(var("f"), t(true)),
        app(var("f"), t(false)),
    );
    assert_eq!(
        code(&prove(&constant, &refl(), &data(), &Nothing, &[])),
        "T081"
    );
    // `funext` proves only functions equal
    let at_bool = equal(vec![("x", Ty::Bool)], Ty::Bool, var("x"), var("x"));
    assert_eq!(
        code(&prove(&at_bool, &funext, &data(), &Nothing, &[])),
        "T083"
    );
}

#[test]
fn a_split_proves_each_case() {
    // match v with None -> None | Some x -> Some x  =  v
    let rebuilt = Term::Match(
        Rc::new(var("v")),
        vec![
            super::term::Arm {
                pat: Pat::Con(ctor("None", 0), vec![]),
                body: none(),
            },
            super::term::Arm {
                pat: Pat::Con(ctor("Some", 1), vec![Pat::Bind("x".into())]),
                body: some(var("x")),
            },
        ]
        .into(),
    );
    let goal = equal(
        vec![("v", option(rigid("a")))],
        option(rigid("a")),
        rebuilt,
        var("v"),
    );
    assert_eq!(code(&prove(&goal, &refl(), &data(), &Nothing, &[])), "T081");
    let arm = |name, arity, binders: Vec<Pat>| (Pat::Con(ctor(name, arity), binders), refl());
    let split = |arms| cert(Rule::Case("v".into(), arms));
    let both = split(vec![
        arm("None", 0, vec![]),
        arm("Some", 1, vec![Pat::Bind("y".into())]),
    ]);
    assert_eq!(code(&prove(&goal, &both, &data(), &Nothing, &[])), "ok");
    // every case, once
    let missing = split(vec![arm("None", 0, vec![])]);
    assert_eq!(
        code(&prove(&goal, &missing, &data(), &Nothing, &[])),
        "T083"
    );
    let twice = split(vec![
        arm("None", 0, vec![]),
        arm("None", 0, vec![]),
        arm("Some", 1, vec![Pat::Wild]),
    ]);
    assert_eq!(code(&prove(&goal, &twice, &data(), &Nothing, &[])), "T083");
    // a split of what is no variable of the statement
    let stranger = cert(Rule::Case("w".into(), vec![arm("None", 0, vec![])]));
    assert_eq!(
        code(&prove(&goal, &stranger, &data(), &Nothing, &[])),
        "T083"
    );
}

#[test]
fn different_values_are_disproved() {
    let goal = equal(
        vec![("x", rigid("a"))],
        option(rigid("a")),
        some(var("x")),
        none(),
    );
    assert_eq!(code(&prove(&goal, &refl(), &data(), &Nothing, &[])), "T080");
}

#[test]
fn mismatch_under_an_unverified_premise_does_not_refute_the_statement() {
    let boolean = |value| Term::Lit(Literal(Lit::Bool(value)));
    let mut goal = equal(vec![], Ty::Bool, boolean(true), boolean(false));
    let Goal::Equal(equation) = &goal.goal else {
        unreachable!()
    };
    goal.premises.push(("h".into(), equation.clone()));
    let failure =
        prove(&goal, &refl(), &data(), &Nothing, &[]).expect_err("refl has unequal sides");
    assert!(matches!(failure.cause(), Failure::StepMismatch { .. }));
    assert!(prove(&goal, &cert(Rule::Fact("h".into())), &data(), &Nothing, &[]).is_ok());
}

#[test]
fn trans_needs_both_of_its_steps() {
    let ty = option(rigid("a"));
    let goal = equal(
        vec![("x", rigid("a"))],
        ty,
        some(var("x")),
        app(lam("y", some(var("y"))), var("x")),
    );
    let middle = some(var("x"));
    let good = cert(Rule::Trans(
        middle.clone(),
        Box::new(refl()),
        Box::new(refl()),
    ));
    assert_eq!(code(&prove(&goal, &good, &data(), &Nothing, &[])), "ok");
    let wrong = cert(Rule::Trans(none(), Box::new(refl()), Box::new(refl())));
    assert_eq!(code(&prove(&goal, &wrong, &data(), &Nothing, &[])), "T090");
}

#[test]
fn cong_reads_its_proof_from_a_premise() {
    let a = rigid("a");
    let mut goal = equal(
        vec![("x", a.clone()), ("y", a.clone())],
        option(a.clone()),
        some(var("x")),
        some(var("y")),
    );
    let cong = |p| cert(Rule::Cong("z".into(), some(var("z")), Box::new(p)));
    // `refl` says nothing of which equation it proves
    assert_eq!(
        code(&prove(&goal, &cong(refl()), &data(), &Nothing, &[])),
        "T083"
    );
    goal.premises.push((
        "h".into(),
        Equation {
            ty: a,
            left: var("x"),
            right: var("y"),
        },
    ));
    assert_eq!(
        code(&prove(
            &goal,
            &cong(cert(Rule::Fact("h".into()))),
            &data(),
            &Nothing,
            &[]
        )),
        "ok"
    );
    // the premise proves `x = y`, not `y = x`
    let flipped = cert(Rule::Symm(Box::new(cert(Rule::Fact("h".into())))));
    assert_eq!(
        code(&prove(&goal, &cong(flipped), &data(), &Nothing, &[])),
        "T081"
    );
    assert_eq!(
        code(&prove(
            &goal,
            &cert(Rule::Fact("absent".into())),
            &data(),
            &Nothing,
            &[]
        )),
        "T083"
    );
}

#[test]
fn types_outside_the_domain_are_refused() {
    let unknown = equal(
        vec![("x", Ty::Refused("its type is not fully known".into()))],
        Ty::Bool,
        var("x"),
        var("x"),
    );
    assert_eq!(
        code(&prove(&unknown, &refl(), &data(), &Nothing, &[])),
        "T082"
    );
    let mut recursive = data();
    recursive
        .get_mut(&("Prelude".into(), "Option".into()))
        .expect("declared")
        .recursive = true;
    let goal = equal(
        vec![("v", option(Ty::Bool))],
        option(Ty::Bool),
        var("v"),
        var("v"),
    );
    assert_eq!(
        code(&prove(&goal, &refl(), &recursive, &Nothing, &[])),
        "T082"
    );
    // a constructor of a type the proof does not know
    let goal = equal(vec![], option(Ty::Bool), none(), none());
    assert_eq!(
        code(&prove(&goal, &refl(), &Data::new(), &Nothing, &[])),
        "T082"
    );
}

#[test]
fn a_proof_that_runs_out_proves_nothing() {
    // nine doublings: `f` applied 512 times, past the depth of a normal form
    let f = fun(Ty::Bool, Ty::Bool);
    let twice = lam("g", lam("y", app(var("g"), app(var("g"), var("y")))));
    let mut nested = var("f");
    for _ in 0..9 {
        nested = app(var("t"), nested);
    }
    let left = app(app(lam("t", nested), twice), var("x"));
    let goal = equal(
        vec![("f", f), ("x", Ty::Bool)],
        Ty::Bool,
        left,
        app(var("f"), var("x")),
    );
    assert_eq!(code(&prove(&goal, &refl(), &data(), &Nothing, &[])), "T084");
    // the same, every time
    assert_eq!(code(&prove(&goal, &refl(), &data(), &Nothing, &[])), "T084");
}

#[test]
fn traced_trans_uses_each_goal_and_preserves_the_verdict() {
    let mut goal = equal(
        vec![("x", Ty::Bool), ("y", Ty::Bool)],
        Ty::Bool,
        var("x"),
        var("y"),
    );
    goal.premises.push((
        "h".into(),
        Equation {
            ty: Ty::Bool,
            left: var("x"),
            right: var("y"),
        },
    ));
    let proof = cert(Rule::Trans(
        var("x"),
        Box::new(refl()),
        Box::new(cert(Rule::Fact("h".into()))),
    ));

    let mut steps = Vec::new();
    let traced = super::prove_traced(&goal, &proof, &data(), &Nothing, &[], &mut steps);

    assert_eq!(traced, Ok(()));
    assert_eq!(traced, prove(&goal, &proof, &data(), &Nothing, &[]));
    assert_eq!(
        steps
            .iter()
            .map(|step| step.goal.as_str())
            .collect::<Vec<_>>(),
        ["x = y", "x = x", "x = y"]
    );
    assert_eq!(steps[2].premises, [("h".to_owned(), "x = y".to_owned())]);
    assert!(steps[0].normalization_steps > 0);
    assert_eq!(
        steps[0].normalization_steps,
        steps[1].normalization_steps + steps[2].normalization_steps
    );
}

#[test]
fn recovery_keeps_each_case_context_when_an_arm_has_no_certificate() {
    use flick_ir::{Arm, Expr, ExprKind, Pat};

    let goal = equal(vec![("v", Ty::Bool)], Ty::Bool, var("v"), var("v"));
    let expr = |kind| Expr {
        kind,
        span: span(),
        ty: None,
    };
    let malformed = expr(ExprKind::Lit(Lit::Bool(true)));
    let name = expr(ExprKind::Global {
        module: super::RULES.into(),
        name: "refl".into(),
        type_args: Vec::new(),
    });
    let arm = |b, body| Arm {
        pat: Pat::Lit(Lit::Bool(b)),
        binder_types: Vec::new(),
        guard: None,
        body,
    };
    let body = expr(ExprKind::Match(
        Box::new(expr(ExprKind::Local("v".into()))),
        vec![arm(false, malformed), arm(true, name)],
    ));

    assert!(super::cert(&body, &[]).is_err());
    let mut steps = Vec::new();
    super::observe(&goal, &body, &data(), &[], &mut steps);

    assert_eq!(steps.len(), 3);
    assert_eq!(steps[1].goal, "false = false");
    assert_eq!(
        steps[1].substitutions,
        [("v".to_owned(), "false".to_owned())]
    );
    assert_eq!(steps[2].goal, "true = true");
    assert_eq!(
        steps[2].substitutions,
        [("v".to_owned(), "true".to_owned())]
    );
    assert!(steps.iter().all(|step| step.normalization_steps == 0));
}

#[test]
fn checked_draft_suggestions_use_the_live_goal_and_visible_premises() {
    let mut draft = equal(
        vec![("x", Ty::Bool), ("y", Ty::Bool)],
        Ty::Bool,
        var("y"),
        var("x"),
    );
    draft.premises.push((
        "h".into(),
        Equation {
            ty: Ty::Bool,
            left: var("x"),
            right: var("y"),
        },
    ));
    let aliases = super::LemmaAliases::new();
    let suggest =
        |draft: &Statement| super::suggest_header(draft, &data(), &Nothing, &[], &aliases);
    assert_eq!(suggest(&draft), ["symm h"]);

    draft.params.push(("symm".into(), Ty::Bool));
    assert!(suggest(&draft).is_empty(), "a local shadows the proof rule");
    draft.params.pop();
    draft.params.push(("h$1".into(), Ty::Bool));
    assert!(
        suggest(&draft).is_empty(),
        "the outer premise cannot be written here"
    );
}

#[test]
fn a_qualified_suggestion_instantiates_and_checks_lemma_premises() {
    let mut proved = equal(
        vec![("x", Ty::Bool), ("y", Ty::Bool)],
        Ty::Bool,
        var("x"),
        var("y"),
    );
    proved.name = "Module.earlier".into();
    let Goal::Equal(goal) = &proved.goal else {
        unreachable!()
    };
    proved.premises.push(("p".into(), goal.clone()));
    let proof = cert(Rule::Fact("p".into()));
    assert_eq!(prove(&proved, &proof, &data(), &Nothing, &[]), Ok(()));
    let lemma = super::Lemma {
        statement: proved.clone(),
        order: vec![("x".into(), false), ("y".into(), false), ("p".into(), true)],
    };
    let aliases = super::LemmaAliases::from([("Module.earlier".into(), vec!["M.earlier".into()])]);
    let mut draft = proved;
    draft.premises[0].1 = draft.premises[0].1.swapped();
    let suggestions =
        super::suggest_header(&draft, &data(), &Nothing, slice::from_ref(&lemma), &aliases);
    assert!(suggestions.iter().any(|s| s == "M.earlier x y (symm p)"));

    draft.premises.clear();
    let rejected = super::suggest_header(&draft, &data(), &Nothing, &[lemma], &aliases);
    assert!(
        rejected.is_empty(),
        "no proof for the lemma's premise exists"
    );
}

#[test]
fn a_checked_lemma_for_equal_definitions_keeps_its_exact_subject() {
    use flick_ir::{Expr, ExprKind, Item, ItemKind};

    use super::Def;

    struct Flag(Item);

    impl super::Program for Flag {
        fn item(&self, module: &str, name: &str) -> Option<&Item> {
            (matches!(module, "A" | "B") && name == "flag").then_some(&self.0)
        }

        fn instance(
            &self,
            _: &str,
            _: &str,
        ) -> Option<(&flick_ir::InstanceInfo, &[(String, Expr)])> {
            None
        }
    }

    let program = Flag(Item {
        span: span(),
        kind: ItemKind::Value {
            name: "flag".into(),
            type_params: Vec::new(),
            dicts: Vec::new(),
            dict_constraints: Vec::new(),
            ty: None,
            body: Expr::new(ExprKind::Lit(Lit::Bool(true)), span()),
        },
    });
    let flag = |module: &str| {
        Term::Def(Rc::new(Def::Value {
            module: module.into(),
            name: "flag".into(),
        }))
    };
    let equation = |module| {
        equal(
            vec![],
            Ty::Bool,
            flag(module),
            Term::Lit(Literal(Lit::Bool(true))),
        )
    };
    let mut earlier = equation("A");
    earlier.name = "earlier".into();
    assert_eq!(prove(&earlier, &refl(), &data(), &program, &[]), Ok(()));
    let lemmas = [super::Lemma {
        statement: earlier,
        order: Vec::new(),
    }];
    let aliases = super::LemmaAliases::new();
    assert_eq!(
        super::suggest_header(&equation("A"), &data(), &program, &lemmas, &aliases),
        ["refl", "earlier"]
    );
    assert_eq!(
        super::suggest_header(&equation("B"), &data(), &program, &lemmas, &aliases),
        ["refl"]
    );
}

#[test]
fn a_synthesized_proof_term_requires_a_rule_that_states_its_equation() {
    let mut goal = equal(vec![("x", Ty::Bool)], Ty::Bool, var("x"), var("x"));
    let Goal::Equal(equation) = &goal.goal else {
        unreachable!()
    };
    goal.premises.push(("h".into(), equation.clone()));
    let proof = cert(Rule::Cong(
        "v".into(),
        var("v"),
        Box::new(cert(Rule::Fact("h".into()))),
    ));
    let mut steps = Vec::new();
    assert_eq!(
        super::prove_with_suggestions(
            &goal,
            &proof,
            &data(),
            &Nothing,
            &[],
            &super::LemmaAliases::new(),
            &mut steps,
        ),
        Ok(())
    );
    assert!(steps[0].suggestions.iter().any(|s| s == "refl"));
    assert_eq!(steps[1].suggestions, ["h", "symm h"]);
}

#[cfg(not(target_arch = "wasm32"))]
fn search_at(offset: u32, cancelled: bool) -> ProofSearchRequest {
    ProofSearchRequest {
        declaration: (0, 100),
        offset,
        cancelled: Arc::new(AtomicBool::new(cancelled)),
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn explicit_search_composes_visible_facts_in_a_deterministic_order() {
    let mut goal = equal(
        vec![("x", Ty::Bool), ("y", Ty::Bool), ("z", Ty::Bool)],
        Ty::Bool,
        var("x"),
        var("z"),
    );
    goal.premises = [("first", "x", "y"), ("second", "z", "y")]
        .into_iter()
        .map(|(h, left, right)| {
            (
                h.into(),
                Equation {
                    ty: Ty::Bool,
                    left: var(left),
                    right: var(right),
                },
            )
        })
        .collect();
    let aliases = super::LemmaAliases::new();
    assert!(super::suggest_header(&goal, &data(), &Nothing, &[], &aliases).is_empty());
    let search = || {
        super::search_proof(
            &goal,
            None,
            &data(),
            &Nothing,
            &[],
            &aliases,
            &search_at(11, false),
        )
    };
    let checked = search();
    assert_eq!(checked.status, ProofSearchStatus::Found);
    assert_eq!(checked.suggestions, ["trans y first (symm second)"]);
    assert_eq!(checked.span, None, "the draft has no existing term");
    assert_eq!(checked.suggestions, search().suggestions);
    assert_eq!(checked.candidates, search().candidates);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn a_search_targets_the_obligation_of_an_incomplete_proof_argument() {
    use flick_ir::{Expr, ExprKind, Stage};

    let mut goal = equal(
        vec![("x", Ty::Bool), ("y", Ty::Bool)],
        Ty::Bool,
        var("y"),
        var("x"),
    );
    goal.premises.push((
        "h".into(),
        Equation {
            ty: Ty::Bool,
            left: var("x"),
            right: var("y"),
        },
    ));
    let file = span().file;
    let name = Expr::new(
        ExprKind::Global {
            module: super::RULES.into(),
            name: "symm".into(),
            type_args: Vec::new(),
        },
        Span::new(file, 12, 16),
    );
    let selected = Span::new(file, 17, 21);
    let body = Expr::new(
        ExprKind::App {
            func: Box::new(name),
            arg: Box::new(Expr::new(ExprKind::Lit(Lit::Bool(true)), selected)),
            stage: Stage::Pure,
            arg_first: false,
        },
        Span::new(file, 12, 21),
    );
    let found = super::search_proof(
        &goal,
        Some(&body),
        &data(),
        &Nothing,
        &[],
        &super::LemmaAliases::new(),
        &search_at(19, false),
    );
    assert_eq!(found.status, ProofSearchStatus::Found);
    assert_eq!(found.suggestions, ["h"]);
    assert_eq!(found.span, Some(selected));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn search_distinguishes_cancelled_empty_and_budget_exhausted() {
    let check = |goal: &Statement, cancelled| {
        super::search_proof(
            goal,
            None,
            &data(),
            &Nothing,
            &[],
            &super::LemmaAliases::new(),
            &search_at(9, cancelled),
        )
    };
    let literal = |value| Term::Lit(Literal(Lit::Bool(value)));
    let different = equal(Vec::new(), Ty::Bool, literal(true), literal(false));
    let empty = check(&different, false);
    assert_eq!(empty.status, ProofSearchStatus::NoCandidate);
    assert!(empty.suggestions.is_empty());
    let cancelled = check(&different, true);
    assert_eq!(cancelled.status, ProofSearchStatus::Cancelled);
    assert_eq!(cancelled.candidates, 0);
    let nested = (0..450).fold(literal(true), |term, _| app(lam("z", var("z")), term));
    let deep = equal(Vec::new(), Ty::Bool, nested.clone(), nested);
    let exhausted = check(&deep, false);
    assert_eq!(exhausted.status, ProofSearchStatus::BudgetExhausted);
    assert!(exhausted.suggestions.is_empty());
    assert!(exhausted.normalization_steps > 0);
}
