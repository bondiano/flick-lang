use super::*;
use crate::ir::Stmt;

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
fn resolution_shares_exact_ordinary_names_and_preserves_dictionary_fallback_ids() {
    let templates = resolution_fixture();
    let mut build = Build {
        templates: &templates,
        run: "$owner".into(),
        impossible: "$owner$invalid".into(),
        span: Span::new(FileId::new(0), 0, 0),
        next: 0,
    };
    let expression = build.resolve(build.local("original"));
    let arms = node_arms(&expression);
    let shared = arms
        .iter()
        .find(|arm| {
            let mut ids = BTreeSet::new();
            known_ids(&arm.pat, &mut ids);
            ids == BTreeSet::from([0, 1])
        })
        .expect("identical ordinary variable names must share one body");
    let ExprKind::Block(unpack, _) = &expression.kind else {
        panic!("descriptor unpack");
    };
    let Stmt::Let(Pat::Tuple(fields), _) = &unpack[0] else {
        panic!("original descriptor fields");
    };
    let ExprKind::Block(lookup, branch) = &shared.body.kind else {
        panic!("single name lookup");
    };
    assert_eq!(lookup.len(), 1);
    let Stmt::Let(_, value) = &lookup[0] else {
        panic!("name lookup binding");
    };
    assert!(matches!(&value.kind, ExprKind::App { func, arg, .. }
        if matches!((&func.kind, &fields[1]), (ExprKind::Local(actual), Pat::Bind(expected)) if actual == expected)
        && matches!(&arg.kind, ExprKind::Lit(Lit::String(name)) if name == "a")));
    let ExprKind::If(_, fallback, recurse) = &branch.kind else {
        panic!("unbound variable fallback");
    };
    assert_original_descriptor(fallback, fields);
    assert!(
        matches!(&recurse.kind, ExprKind::App { arg, .. }
        if matches!(&arg.kind, ExprKind::Tuple(args) if matches!(args[0].kind,
            ExprKind::Lit(Lit::Int(op)) if op == Op::Resolve as i32))),
        "a resolved environment value must still be recursively resolved"
    );
    let mut covered = BTreeSet::new();
    for arm in &arms[..arms.len() - 1] {
        known_ids(&arm.pat, &mut covered);
    }
    assert_eq!(
        covered,
        (0..8).collect(),
        "original IDs remain complete and disjoint"
    );
    for id in [3, 4] {
        let arm = arms
            .iter()
            .find(|arm| matches!(arm.pat, Pat::Lit(Lit::Int(node)) if node == id))
            .expect("each dictionary occurrence stays unique");
        let mut fallbacks = Vec::new();
        root_environment_ids(&arm.body, &mut fallbacks);
        assert!(!fallbacks.is_empty());
        assert!(
            fallbacks.iter().all(|node| *node == id),
            "dictionary derivation must retain this occurrence's original fallback ID"
        );
    }
    for id in [2, 7] {
        assert!(
            arms.iter().any(|arm| {
                let mut ids = BTreeSet::new();
                known_ids(&arm.pat, &mut ids);
                ids == BTreeSet::from([id])
            }),
            "distinct and differently cased names never share"
        );
    }
    let unchanged = arms
        .iter()
        .find(|arm| {
            let mut ids = BTreeSet::new();
            known_ids(&arm.pat, &mut ids);
            ids == BTreeSet::from([5, 6])
        })
        .expect("non-variable identity remains shared");
    assert_original_descriptor(&unchanged.body, fields);
    let invalid = arms.last().expect("unknown ID");
    assert!(matches!(invalid.pat, Pat::Wild));
    assert!(matches!(&invalid.body.kind, ExprKind::App { func, .. }
        if matches!(&func.kind, ExprKind::Local(name) if name.ends_with("$invalid"))));
}

fn assert_original_descriptor(expression: &Expr, fields: &[Pat]) {
    assert!(
        matches!(&expression.kind, ExprKind::Tuple(values) if values.len()==2 && values.iter().zip(fields).all(|(value, field)|
        matches!((&value.kind, field), (ExprKind::Local(actual), Pat::Bind(expected)) if actual == expected))),
        "both original node ID and lazy parameter environment must be forwarded"
    );
}

fn root_environment_ids(expression: &Expr, ids: &mut Vec<i32>) {
    if let ExprKind::Tuple(fields) = &expression.kind
        && fields.len() == 2
        && let ExprKind::Lit(Lit::Int(id)) = fields[0].kind
        && matches!(&fields[1].kind, ExprKind::Lambda { params, body }
            if params.len()==1 && matches!(params[0].pat, Pat::Wild) && matches!(body.kind, ExprKind::Lit(Lit::Unit)))
    {
        ids.push(id);
    }
    expression.each_child(&mut |child| root_environment_ids(child, ids));
}
