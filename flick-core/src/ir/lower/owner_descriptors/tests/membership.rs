use super::*;

pub(super) fn matches(pattern: &Pat, index: i32) -> bool {
    match pattern {
        Pat::Lit(Lit::Int(id)) => *id == index,
        Pat::Or(alternatives) => alternatives.iter().any(|pattern| matches(pattern, index)),
        Pat::Wild => true,
        _ => panic!("integer dispatch pattern"),
    }
}

pub(super) fn assert_impossible(expression: &Expr) {
    assert!(
        is_impossible(expression),
        "an unknown index must still fail closed"
    );
}

fn is_impossible(expression: &Expr) -> bool {
    matches!(&expression.kind, ExprKind::App { func, .. }
        if matches!(&func.kind, ExprKind::Local(name) if name.ends_with("$invalid")))
}

fn bound(condition: &Expr) -> Option<i32> {
    let ExprKind::Binary(BinOp::And, lower, upper) = &condition.kind else {
        return None;
    };
    let ExprKind::Binary(BinOp::Ge, low_subject, zero) = &lower.kind else {
        return None;
    };
    let ExprKind::Binary(BinOp::Lt, high_subject, count) = &upper.kind else {
        return None;
    };
    let (
        ExprKind::Local(low),
        ExprKind::Local(high),
        ExprKind::Lit(Lit::Int(0)),
        ExprKind::Lit(Lit::Int(count)),
    ) = (
        &low_subject.kind,
        &high_subject.kind,
        &zero.kind,
        &count.kind,
    )
    else {
        return None;
    };
    assert_eq!(low, high, "both bounds test the same original discriminant");
    Some(*count)
}

fn dispatch(expression: &Expr) -> &Expr {
    match &expression.kind {
        ExprKind::Block(_, body) => dispatch(body),
        ExprKind::If(condition, _, _) if bound(condition).is_some() => expression,
        ExprKind::If(_, yes, _) => dispatch(yes),
        ExprKind::Match(_, _) => expression,
        _ => panic!("integer dispatch"),
    }
}

pub(super) fn select(expression: &Expr, index: i32) -> Option<&Expr> {
    match &dispatch(expression).kind {
        ExprKind::If(condition, yes, no) => {
            if (0..bound(condition).expect("checked domain")).contains(&index) {
                select(yes, index)
            } else {
                assert_impossible(no);
                None
            }
        }
        ExprKind::Match(_, arms) => arms
            .iter()
            .find(|arm| matches(&arm.pat, index))
            .filter(|arm| !is_impossible(&arm.body))
            .map(|arm| &arm.body),
        _ => unreachable!(),
    }
}

pub(super) fn assert_unknown(expression: &Expr, end: i32) {
    for index in [-1, end, i32::MAX] {
        assert!(
            select(expression, index).is_none(),
            "unknown index {index} must not select a successful body"
        );
    }
}

fn domain(expression: &Expr) -> BTreeSet<i32> {
    match &dispatch(expression).kind {
        ExprKind::If(condition, _, invalid) => {
            assert_impossible(invalid);
            (0..bound(condition).expect("checked domain")).collect()
        }
        ExprKind::Match(_, arms) => {
            let mut ids = BTreeSet::new();
            for arm in arms {
                known_ids(&arm.pat, &mut ids);
            }
            ids
        }
        _ => unreachable!(),
    }
}

pub(super) fn assert_operation_domain(operations: &[Arm]) {
    let mut ids = BTreeSet::new();
    for arm in operations {
        known_ids(&arm.pat, &mut ids);
    }
    assert_eq!(
        ids,
        (0..=Op::HeadAlias as i32).collect(),
        "all operation discriminants remain complete and disjoint"
    );
    let unknown = operations.last().expect("invalid operation");
    assert!(matches!(unknown.pat, Pat::Wild));
    assert_impossible(&unknown.body);
}

pub(super) fn assert_exact_node_domains(operations: &[Arm]) {
    let operation = |op: Op| {
        &operations
            .iter()
            .find(|arm| matches(&arm.pat, op as i32))
            .expect("descriptor operation")
            .body
    };
    let expected = domain(operation(Op::Resolve));
    assert_eq!(
        expected,
        (0..expected.len() as i32).collect(),
        "original node IDs remain dense"
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
        let expression = operation(op);
        assert_eq!(
            domain(expression),
            expected,
            "every selector preserves the full original node domain"
        );
        for &id in &expected {
            assert!(select(expression, id).is_some(), "known node {id}");
        }
        assert_unknown(expression, expected.len() as i32);
    }
}

#[test]
fn dense_default_selectors_share_one_bounds_check_and_forward_the_original_descriptor() {
    let templates = alias_dispatch_fixture();
    let mut build = builder(&templates);
    let expression = build.nodes(build.local("original"), |_, _, _, _, current| current);
    let (current, fields, source) = descriptor_parts(&expression);
    assert!(matches!(&source.kind, ExprKind::Local(name) if name == "original"));
    assert_eq!(
        fields.len(),
        2,
        "the original node and lazy environment are unpacked together"
    );
    let ExprKind::If(condition, yes, invalid) = &dispatch(&expression).kind else {
        panic!("one checked domain");
    };
    assert_eq!(bound(condition), Some(8));
    assert_impossible(invalid);
    let arms = node_arms(yes);
    assert_eq!(
        arms.len(),
        1,
        "identical results share one body inside the checked original domain"
    );
    assert!(matches!(arms[0].pat, Pat::Wild));
    assert_current(&arms[0].body, current);
    for id in 0..8 {
        assert_current(select(&expression, id).unwrap(), current);
    }
    assert_unknown(&expression, 8);
}

#[test]
fn membership_is_exact_for_empty_singleton_and_original_node_domains() {
    let templates = alias_dispatch_fixture();
    for count in [0, 1, templates.exprs.len()] {
        let mut fixture = templates.clone();
        fixture.exprs.truncate(count);
        let mut build = builder(&fixture);
        let expression = build.nodes(build.local("original"), |this, _, _, _, _| {
            this.boolean(true)
        });
        assert_eq!(domain(&expression), (0..count as i32).collect());
        for id in 0..count as i32 {
            assert!(matches!(
                select(&expression, id).unwrap().kind,
                ExprKind::Lit(Lit::Bool(true))
            ));
        }
        assert_unknown(&expression, count as i32);
    }
}

#[test]
fn sparse_and_nonlocal_dispatches_preserve_membership_and_single_evaluation() {
    let templates = alias_dispatch_fixture();
    let build = builder(&templates);
    let ids = [0, 2, 4, 6, 8];
    let mut arms = ids
        .iter()
        .map(|&id| (Pat::Lit(Lit::Int(id)), build.boolean(true)))
        .collect::<Vec<_>>();
    arms.push((Pat::Wild, build.impossible()));
    let sparse = build.case(build.local("index"), arms);
    assert!(
        matches!(sparse.kind, ExprKind::Match(_, _)),
        "a sparse table must not acquire a dense range assumption"
    );
    for id in -1..=9 {
        assert_eq!(select(&sparse, id).is_some(), ids.contains(&id));
    }
    let source = build.pure(build.local("input"), build.unit());
    let mut arms = (0..8)
        .map(|id| (Pat::Lit(Lit::Int(id)), build.boolean(true)))
        .collect::<Vec<_>>();
    arms.push((Pat::Wild, build.impossible()));
    let nonlocal = build.case(source, arms);
    assert!(
        matches!(&nonlocal.kind, ExprKind::Match(subject, _) if matches!(subject.kind, ExprKind::App { .. })),
        "a nonlocal scrutinee remains evaluated once without duplicated range tests"
    );
    assert_eq!(
        count(
            &nonlocal,
            |e| matches!(&e.kind, ExprKind::App { func, .. } if matches!(&func.kind, ExprKind::Local(name) if name == "input"))
        ),
        1
    );
    for id in 0..8 {
        assert!(select(&nonlocal, id).is_some());
    }
    assert_unknown(&nonlocal, 8);
}
