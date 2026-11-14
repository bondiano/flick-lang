//! A canonical evidence bracket never re-evaluates spilled operands inside C.
use std::collections::HashMap;

use crate::ir::{Arm, Expr, ExprKind, Lit, Pat, Stage, Stmt};
use crate::source::{FileId, Span};
use crate::types::Dict;

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
        super::simplify(&mut body);
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
    assert_eq!(super::uses(&body, "module"), 1);
    let mut strict = Vec::new();
    super::walk::each_strict_use(&body, &mut |name| strict.push(name.to_owned()));
    assert_eq!(strict, ["module"]);
    super::rename(&mut body, "$copy");
    assert_eq!(super::uses(&body, "module"), 0);
    assert_eq!(super::uses(&body, "module$copy"), 1);
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
    assert_eq!(super::uses(&body, "module"), 0);
    assert_eq!(super::uses(&body, "alias"), 1);
}
