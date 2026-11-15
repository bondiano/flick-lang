use super::*;
use crate::ir::Stmt;

#[test]
fn default_selectors_use_one_shared_known_node_membership_observation() {
    let templates = alias_dispatch_fixture();
    let mut build = Build {
        templates: &templates,
        run: "$owner".into(),
        impossible: "$owner$invalid".into(),
        span: Span::new(FileId::new(0), 0, 0),
        next: 0,
    };
    let expression = build.nodes_with_default(
        build.local("original"),
        |_, current| current,
        |_, _, _, _| None,
    );
    let arms = node_arms(&expression);
    assert_eq!(
        arms.len(),
        1,
        "a default-only selector must not copy the known-ID table"
    );
    assert!(matches!(arms[0].pat, Pat::Wild));
    let (subject, _) = dispatch(&expression);
    let ExprKind::If(condition, current, invalid) = &arms[0].body.kind else {
        panic!("one shared observation must guard the default from unknown IDs");
    };
    assert_membership_call(condition, subject);
    assert_impossible(invalid);
    let ExprKind::Block(statements, _) = &expression.kind else {
        panic!("unpack descriptor");
    };
    let Stmt::Let(Pat::Tuple(bindings), _) = &statements[0] else {
        panic!("original descriptor fields");
    };
    assert!(
        matches!(&current.kind, ExprKind::Tuple(fields) if fields.len() == 2 && fields.iter().zip(bindings).all(|(field, binding)|
        matches!((&field.kind, binding), (ExprKind::Local(actual), Pat::Bind(expected)) if actual == expected))),
        "the shared default receives exactly the original node and lazy environment"
    );
}

fn dispatch(expression: &Expr) -> (&Expr, &[Arm]) {
    match &expression.kind {
        ExprKind::Block(_, body) | ExprKind::If(_, body, _) => dispatch(body),
        ExprKind::Match(subject, arms) => (subject, arms),
        _ => panic!("descriptor node dispatch"),
    }
}

fn assert_impossible(expression: &Expr) {
    assert!(
        matches!(&expression.kind,
        ExprKind::App { func, .. } if matches!(&func.kind,
            ExprKind::Local(name) if name.ends_with("$invalid"))),
        "an unknown node index must still fail closed"
    );
}

fn assert_membership_call(expression: &Expr, subject: &Expr) {
    let ExprKind::App { func, arg, .. } = &expression.kind else {
        panic!("shared membership call");
    };
    assert!(matches!(&func.kind, ExprKind::Local(_)));
    let ExprKind::Tuple(fields) = &arg.kind else {
        panic!("membership arguments");
    };
    assert!(matches!(fields[0].kind, ExprKind::Lit(Lit::Int(op)) if op == Op::KnownNode as i32));
    assert!(
        matches!((&fields[1].kind, &subject.kind),
        (ExprKind::Local(actual), ExprKind::Local(expected)) if actual == expected),
        "membership must test the same original node as the selector"
    );
}

pub(super) fn assert_shared_defaults(operations: &[Arm]) {
    let membership = operations
        .iter()
        .find(|arm| matches!(arm.pat, Pat::Lit(Lit::Int(op)) if op == Op::KnownNode as i32))
        .expect("one shared membership operation");
    let (_, membership_arms) = dispatch(&membership.body);
    assert_eq!(membership_arms.len(), 2);
    assert!(matches!(membership_arms[0].pat, Pat::Or(_)));
    assert!(matches!(
        membership_arms[0].body.kind,
        ExprKind::Lit(Lit::Bool(true))
    ));
    assert!(matches!(membership_arms[1].pat, Pat::Wild));
    assert!(matches!(
        membership_arms[1].body.kind,
        ExprKind::Lit(Lit::Bool(false))
    ));
    let mut expected = BTreeSet::new();
    known_ids(&membership_arms[0].pat, &mut expected);
    assert_eq!(
        expected.iter().copied().collect::<Vec<_>>(),
        (0..i32::try_from(expected.len()).expect("few nodes")).collect::<Vec<_>>()
    );
    for op in [
        Op::Resolve,
        Op::Spine,
        Op::Head,
        Op::Productive,
        Op::Row,
        Op::Adapt,
        Op::IsSymbol,
        Op::Dictionary,
    ] {
        let operation = operations
            .iter()
            .find(|arm| matches!(arm.pat, Pat::Lit(Lit::Int(id)) if id == op as i32))
            .expect("descriptor operation");
        let (subject, arms) = dispatch(&operation.body);
        let fallback = arms.last().expect("invalid node arm");
        assert!(matches!(fallback.pat, Pat::Wild));
        let mut specialized = BTreeSet::new();
        for arm in &arms[..arms.len() - 1] {
            known_ids(&arm.pat, &mut specialized);
        }
        assert!(specialized.is_subset(&expected));
        if let ExprKind::If(condition, _, invalid) = &fallback.body.kind {
            assert_membership_call(condition, subject);
            assert_impossible(invalid);
            assert!(
                specialized.len() < expected.len(),
                "only selectors with defaults need membership"
            );
        } else {
            assert_impossible(&fallback.body);
            assert_eq!(
                specialized, expected,
                "explicit selectors preserve every original ID"
            );
        }
    }
}

#[test]
fn membership_is_exact_for_empty_singleton_and_original_node_domains() {
    let mut templates = alias_dispatch_fixture();
    for count in [0, 1, templates.exprs.len()] {
        let mut fixture = templates.clone();
        fixture.exprs.truncate(count);
        let build = Build {
            templates: &fixture,
            run: "$owner".into(),
            impossible: "$owner$invalid".into(),
            span: Span::new(FileId::new(0), 0, 0),
            next: 0,
        };
        let expression = build.known_node(build.local("original"));
        if count == 0 {
            assert!(matches!(expression.kind, ExprKind::Lit(Lit::Bool(false))));
        } else {
            let (subject, arms) = dispatch(&expression);
            assert!(matches!(&subject.kind, ExprKind::Local(name) if name == "original"));
            assert_eq!(arms.len(), 2);
            let mut ids = BTreeSet::new();
            known_ids(&arms[0].pat, &mut ids);
            assert_eq!(ids, (0..i32::try_from(count).expect("few nodes")).collect());
            assert!(matches!(arms[0].body.kind, ExprKind::Lit(Lit::Bool(true))));
            assert!(matches!(arms[1].pat, Pat::Wild));
            assert!(matches!(arms[1].body.kind, ExprKind::Lit(Lit::Bool(false))));
        }
    }
    templates.exprs.truncate(1);
    let mut build = Build {
        templates: &templates,
        run: "$owner".into(),
        impossible: "$owner$invalid".into(),
        span: Span::new(FileId::new(0), 0, 0),
        next: 0,
    };
    let all_special = build.nodes_with_default(
        build.local("original"),
        |_, _| panic!("unused default must not be built"),
        |this, _, _, _| Some(this.boolean(true)),
    );
    let (_, arms) = dispatch(&all_special);
    assert_eq!(arms.len(), 2);
    assert_impossible(&arms[1].body);
}
