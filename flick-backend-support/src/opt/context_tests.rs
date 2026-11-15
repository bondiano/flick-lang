use crate::{Known, opt};
use flick_ir::{Dict, Expr, ExprKind, FileId, LangItem, Lit, Namespace, Span, Stage};

fn table() -> Vec<LangItem> {
    [("numeric", "Closed"), ("int", "Scalar"), ("add", "plus")]
        .into_iter()
        .map(|(lang, name)| LangItem {
            lang: lang.into(),
            module: "Renamed".into(),
            namespace: if lang == "add" {
                Namespace::Value
            } else {
                Namespace::Type
            },
            name: name.into(),
        })
        .collect()
}

fn call(module: &str, method: &str) -> Expr {
    let at = Span::new(FileId::new(0), 0, 0);
    let method = Expr::new(
        ExprKind::Method {
            dict: Dict::Instance {
                module: module.into(),
                name: "Closed Scalar".into(),
                args: vec![],
            },
            name: method.into(),
        },
        at,
    );
    let apply = |func, value| {
        Expr::new(
            ExprKind::App {
                func: Box::new(func),
                arg: Box::new(Expr::new(ExprKind::Lit(Lit::Int(value)), at)),
                stage: Stage::Pure,
                arg_first: false,
            },
            at,
        )
    };
    apply(apply(method, 20), 22)
}

#[test]
fn specialization_uses_declared_roles_instead_of_prelude_names() {
    let table = table();
    let known = Known::of(&table);
    let mut value = call("Renamed", "plus");
    opt::simplify(&mut value, &known);
    assert_eq!(value.kind, ExprKind::Lit(Lit::Int(42)));
    let mut impostor = call("Numeric", "add");
    let original = impostor.clone();
    opt::simplify(&mut impostor, &known);
    assert_eq!(impostor, original);
}

#[test]
fn missing_roles_do_not_guess_the_numeric_dictionary() {
    let known = Known::of(&[]);
    let mut value = call("Numeric", "add");
    let original = value.clone();
    opt::simplify(&mut value, &known);
    assert_eq!(value, original);
}

// A canonical evidence bracket never re-evaluates spilled operands inside C.
use crate::ir::{Arm, Pat, Stmt};
use std::collections::HashMap;

fn expr(kind: ExprKind) -> Expr {
    Expr::new(kind, Span::new(FileId::new(0), 0, 0))
}

fn local(name: &str) -> Expr {
    expr(ExprKind::Local(name.into()))
}

fn invocation(position: usize) -> Expr {
    if position == 2 {
        expr(ExprKind::Await(Box::new(local("spilled"))))
    } else {
        expr(ExprKind::App {
            func: Box::new(local(if position == 0 { "spilled" } else { "callee" })),
            arg: Box::new(local(if position == 1 { "spilled" } else { "argument" })),
            stage: Stage::Runtime,
            arg_first: position == 1,
        })
    }
}

fn bracket(position: usize) -> Expr {
    expr(ExprKind::WithOwnerEvidence(
        Box::new(local("owner")),
        Box::new(invocation(position)),
    ))
}

fn global() -> Expr {
    expr(ExprKind::Global {
        module: "Test".into(),
        name: "value".into(),
    })
}

#[test]
fn shared_substitution_keeps_spilled_evidence_operands_evaluated() {
    for position in 0..3 {
        let mut body = bracket(position);
        let original = body.clone();
        super::place(
            &mut body,
            &mut HashMap::from([("spilled".into(), global())]),
        );
        assert_eq!(body, original, "position={position}");
    }
}

#[test]
fn named_match_retains_a_binder_needed_by_an_evidence_bracket() {
    for position in 0..3 {
        let mut body = expr(ExprKind::Match(
            Box::new(global()),
            vec![Arm {
                pat: Pat::Bind("spilled".into()),
                guard: None,
                body: bracket(position),
            }],
        ));
        super::simplify(&mut body, &crate::Known::of(&[]));
        let ExprKind::Match(_, arms) = &body.kind else {
            panic!("spilled global must still be evaluated outside evidence: {body:?}");
        };
        assert_eq!(arms[0].pat, Pat::Bind("spilled".into()));
        assert_eq!(arms[0].body, bracket(position));
    }
}

#[test]
fn already_evaluated_aliases_and_old_context_owner_can_still_substitute() {
    for replacement in [local("alias"), expr(ExprKind::Lit(Lit::Unit))] {
        let mut body = bracket(0);
        super::place(
            &mut body,
            &mut HashMap::from([("spilled".into(), replacement.clone())]),
        );
        let ExprKind::WithOwnerEvidence(_, invocation) = body.kind else {
            unreachable!()
        };
        let ExprKind::App { func, .. } = invocation.kind else {
            unreachable!()
        };
        assert_eq!(*func, replacement);
    }
    let mut body = bracket(0);
    super::place(&mut body, &mut HashMap::from([("owner".into(), global())]));
    let ExprKind::WithOwnerEvidence(owner, _) = body.kind else {
        unreachable!()
    };
    assert_eq!(*owner, global(), "candidate is evaluated in the old C");
}

fn opened() -> Expr {
    expr(ExprKind::Method {
        dict: Dict::Super {
            dict: Box::new(Dict::Opened {
                binder: "module".into(),
                field: "renderer".into(),
            }),
            class: "Base".into(),
        },
        name: "render".into(),
    })
}

#[test]
fn dictionary_metadata_reads_participate_in_use_counts_and_alpha_renaming() {
    let mut body = expr(ExprKind::Block(
        vec![Stmt::Let(Pat::Bind("module".into()), global())],
        Box::new(opened()),
    ));
    assert_eq!(body.uses("module"), 1);
    let mut strict = Vec::new();
    super::walk::each_strict_use(&body, &mut |name| strict.push(name.to_owned()));
    assert_eq!(strict, ["module"]);
    super::rename(&mut body, "$copy");
    assert_eq!(body.uses("module"), 0);
    assert_eq!(body.uses("module$copy"), 1);
}

#[test]
fn dictionary_substitution_accepts_local_aliases_but_retains_nonlocal_bindings() {
    let mut body = opened();
    super::place(&mut body, &mut HashMap::from([("module".into(), global())]));
    assert_eq!(body, opened());
    super::place(
        &mut body,
        &mut HashMap::from([("module".into(), local("alias"))]),
    );
    assert_eq!(body.uses("module"), 0);
    assert_eq!(body.uses("alias"), 1);
}

#[test]
fn propagation_without_named_lets_preserves_statements_and_metadata() {
    let mut statements = vec![
        Stmt::Let(
            Pat::Tuple(vec![Pat::Bind("part".into())]),
            expr(ExprKind::Tuple(vec![global()])),
        ),
        Stmt::Fun {
            name: "helper".into(),
            rec: true,
            dicts: vec![],
            value: bracket(0),
        },
        Stmt::Var("state".into(), local("part")),
        Stmt::Assign("state".into(), local("helper")),
        Stmt::Expr(opened()),
    ];
    let mut result = bracket(1);
    let original = (statements.clone(), result.clone());

    assert!(!super::propagate(&mut statements, &mut result, &|_, _| {
        None
    }));
    assert_eq!((statements, result), original);
}

#[test]
fn simplification_visits_named_lets_inside_function_only_blocks() {
    let answer = expr(ExprKind::Lit(Lit::Int(42)));
    let function = |body| Stmt::Fun {
        name: "helper".into(),
        rec: false,
        dicts: vec![],
        value: expr(ExprKind::Lambda {
            params: vec![flick_ir::Param {
                pat: Pat::Wild,
                stage: Stage::Pure,
                ty: None,
            }],
            body: Box::new(body),
        }),
    };
    let mut body = expr(ExprKind::Block(
        vec![function(expr(ExprKind::Block(
            vec![Stmt::Let(Pat::Bind("answer".into()), answer.clone())],
            Box::new(local("answer")),
        )))],
        Box::new(local("helper")),
    ));

    super::simplify(&mut body, &Known::of(&[]));
    assert_eq!(
        body,
        expr(ExprKind::Block(
            vec![function(answer)],
            Box::new(local("helper")),
        )),
    );
}

#[test]
fn nonmovable_named_lets_leave_function_bodies_untouched() {
    let raw = expr(ExprKind::App {
        func: Box::new(global()),
        arg: Box::new(local("owner")),
        stage: Stage::Pure,
        arg_first: false,
    });
    let mut statements = vec![
        Stmt::Fun {
            name: "helper".into(),
            rec: true,
            dicts: vec![],
            value: bracket(0),
        },
        Stmt::Let(Pat::Bind("raw".into()), raw),
    ];
    let mut result = expr(ExprKind::Tuple(vec![local("raw"), local("helper")]));
    let original = (statements.clone(), result.clone());
    assert!(!super::propagate(&mut statements, &mut result, &|_, _| {
        Some(1)
    }));
    assert_eq!((statements, result), original);
}

#[test]
fn partial_applications_can_move_to_their_completing_use() {
    let apply = |func, arg| {
        expr(ExprKind::App {
            func: Box::new(func),
            arg: Box::new(arg),
            stage: Stage::Pure,
            arg_first: false,
        })
    };
    let function = |body| {
        expr(ExprKind::Lambda {
            params: vec![flick_ir::Param {
                pat: Pat::Bind("second".into()),
                stage: Stage::Pure,
                ty: None,
            }],
            body: Box::new(body),
        })
    };
    let partial = apply(global(), local("first"));
    let mut statements = vec![Stmt::Let(Pat::Bind("partial".into()), partial.clone())];
    let mut result = function(apply(local("partial"), local("second")));
    assert!(super::propagate(
        &mut statements,
        &mut result,
        &|_, _| Some(2)
    ));
    assert!(statements.is_empty());
    assert_eq!(result, function(apply(partial, local("second"))));
}
