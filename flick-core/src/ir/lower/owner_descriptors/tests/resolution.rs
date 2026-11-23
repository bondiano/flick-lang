use super::*;

fn resolution_fixture() -> OwnerTypeTemplates {
    let kinds = [
        serde_json::json!({"Var":"a"}),
        serde_json::json!({"Var":"a"}),
        serde_json::json!({"Var":"b"}),
        serde_json::json!({"Var":"a$Dict"}),
        serde_json::json!({"Var":"a$Dict"}),
        serde_json::json!({"Con":{"module":"Test","name":"A"}}),
        serde_json::json!("Tuple"),
        serde_json::json!({"Var":"a$dict"}),
    ];
    let mut templates = alias_dispatch_fixture();
    templates.exprs = kinds
        .into_iter()
        .map(|kind| {
            serde_json::from_value(serde_json::json!({"kind":kind,"children":[]}))
                .expect("original node")
        })
        .collect();
    templates
}

#[test]
fn resolution_preserves_exact_names_original_environments_and_dictionary_fallback_ids() {
    let templates = resolution_fixture();
    let mut build = builder(&templates);
    let expression = build.resolve(build.local("original"));
    let (current, fields, source) = descriptor_parts(&expression);
    assert!(matches!(&source.kind, ExprKind::Local(name) if name == "original"));
    let Pat::Bind(environment) = &fields[1] else {
        panic!("lazy environment binding");
    };
    let arms = node_arms(&expression);
    let mut covered = BTreeSet::new();
    for arm in arms {
        known_ids(&arm.pat, &mut covered);
    }
    assert_eq!(
        covered,
        (0..8).collect(),
        "all original IDs remain disjoint and complete"
    );
    for (id, expected) in [
        (0, "a"),
        (1, "a"),
        (2, "b"),
        (3, "a$Dict"),
        (4, "a$Dict"),
        (7, "a$dict"),
    ] {
        let branch = membership::select(&expression, id).expect("original variable node");
        let ExprKind::Block(lookup, result) = &branch.kind else {
            panic!("one environment lookup");
        };
        let [Stmt::Let { value, .. }] = &lookup[..] else {
            panic!("single lookup binding");
        };
        assert!(
            matches!(&value.kind, ExprKind::App { func, arg, .. }
            if matches!(&func.kind, ExprKind::Local(name) if name == environment)
            && matches!(&arg.kind, ExprKind::Lit(Lit::String(name)) if name == expected)),
            "node {id} must look up its exact spelling in the original lazy environment"
        );
        let ExprKind::If(_, fallback, recurse) = &result.kind else {
            panic!("unbound fallback");
        };
        if !expected.ends_with("$Dict") {
            assert_current(fallback, current);
        }
        assert_eq!(
            calls(recurse, Op::Resolve).len(),
            1,
            "a bound environment descriptor is recursively resolved"
        );
    }
    for id in [3, 4] {
        let branch = membership::select(&expression, id).expect("original dictionary node");
        let mut fallbacks = Vec::new();
        root_environment_ids(branch, &mut fallbacks);
        assert!(!fallbacks.is_empty());
        assert!(
            fallbacks.iter().all(|node| *node == id),
            "each dictionary occurrence retains its own original fallback ID"
        );
    }
    for id in [5, 6] {
        assert_current(
            membership::select(&expression, id).expect("unchanged nonvariable node"),
            current,
        );
    }
    membership::assert_unknown(&expression, 8);
}

fn root_environment_ids(expression: &Expr, ids: &mut Vec<i32>) {
    if let ExprKind::Tuple(fields) = &expression.kind
        && fields.len() == 2
        && let ExprKind::Lit(Lit::Int(id)) = fields[0].kind
        && matches!(&fields[1].kind, ExprKind::Lambda { params, body }
            if params.len() == 1 && matches!(params[0].pat, Pat::Wild) && matches!(body.kind, ExprKind::Lit(Lit::Unit)))
    {
        ids.push(id);
    }
    expression.each_child(&mut |child| root_environment_ids(child, ids));
}
