use std::collections::BTreeSet;

use super::{Build, Op};
use crate::ir::tests::lower_text;
use crate::ir::{Arm, BinOp, Expr, ExprKind, ItemKind, Lit, Pat};
use crate::source::{FileId, Span};
use crate::types::OwnerTypeTemplates;

mod resolution {
    include!("tests/resolution.rs");
}

mod membership {
    include!("tests/membership.rs");
}

fn abstract_calls(expression: &Expr) -> usize {
    let mut count = usize::from(matches!(&expression.kind,
        ExprKind::App { arg, .. } if matches!(&arg.kind, ExprKind::Tuple(fields)
            if matches!(fields.first().map(|field| &field.kind), Some(ExprKind::Lit(Lit::Int(op))) if *op == Op::Abstract as i32))));
    expression.each_child(&mut |child| count += abstract_calls(child));
    count
}

fn inspect(expression: &Expr, found: &mut bool) {
    if let ExprKind::Lambda { params, body } = &expression.kind
        && params.len() == 1
        && matches!(&params[0].pat, Pat::Tuple(fields) if fields.len() == 4)
        && let ExprKind::Match(_, arms) = &body.kind
        && let Some(arm) = arms.iter().find(|arm| {
            matches!(&arm.pat,
            Pat::Lit(Lit::Int(op)) if *op == Op::Productive as i32)
        })
    {
        *found = true;
        assert_eq!(
            abstract_calls(&arm.body),
            0,
            "the P fastpath must tail-dispatch its fallback rather than duplicate full observations per syntax node"
        );
    }
    expression.each_child(&mut |child| inspect(child, found));
}

fn match_count(expression: &Expr) -> usize {
    let mut count = usize::from(matches!(&expression.kind, ExprKind::Match(..)));
    expression.each_child(&mut |child| count += match_count(child));
    count
}

fn static_condition_count(expression: &Expr) -> usize {
    let mut count = usize::from(matches!(&expression.kind,
        ExprKind::If(condition, ..) if matches!(condition.kind, ExprKind::Lit(Lit::Bool(_)))));
    expression.each_child(&mut |child| count += static_condition_count(child));
    count
}

fn literal_comparison_count(expression: &Expr) -> usize {
    let mut count = usize::from(matches!(&expression.kind,
        ExprKind::Binary(BinOp::Eq, left, right)
            if matches!(left.kind, ExprKind::Lit(_)) && matches!(right.kind, ExprKind::Lit(_))));
    expression.each_child(&mut |child| count += literal_comparison_count(child));
    count
}

fn node_arms(expression: &Expr) -> &[Arm] {
    match &expression.kind {
        ExprKind::Block(_, body) | ExprKind::If(_, body, _) => node_arms(body),
        ExprKind::Match(_, arms) => arms,
        _ => panic!("descriptor node dispatch"),
    }
}

fn known_ids(pattern: &Pat, ids: &mut BTreeSet<i32>) {
    match pattern {
        Pat::Lit(Lit::Int(id)) => assert!(ids.insert(*id), "node IDs must stay disjoint"),
        Pat::Or(alternatives) => {
            for pattern in alternatives {
                known_ids(pattern, ids);
            }
        }
        Pat::Wild => {}
        _ => panic!("descriptor dispatch only tests original node IDs"),
    }
}

fn inspect_abstract_apply(expression: &Expr, found: &mut bool) {
    if let ExprKind::Lambda { params, body } = &expression.kind
        && params.len() == 1
        && matches!(&params[0].pat, Pat::Tuple(fields) if fields.len() == 4)
        && let ExprKind::Match(_, arms) = &body.kind
        && let Some(arm) = arms.iter().find(|arm| {
            matches!(&arm.pat,
            Pat::Lit(Lit::Int(op)) if *op == Op::AbstractApply as i32)
        })
    {
        *found = true;
        membership::assert_shared_defaults(arms);
        assert_eq!(
            match_count(&arm.body),
            1,
            "abstract application must share state-field observations instead of embedding a full state switch in each application case"
        );
        assert_eq!(
            static_condition_count(&arm.body),
            0,
            "known abstract-state branches must be selected before generating the interpreter"
        );
        let dictionary = arms
            .iter()
            .find(|arm| {
                matches!(&arm.pat,
                Pat::Lit(Lit::Int(op)) if *op == Op::Dictionary as i32)
            })
            .expect("dictionary operation");
        assert_eq!(
            literal_comparison_count(&dictionary.body),
            0,
            "declared constructor classification must not repeat constant comparisons at runtime"
        );
    }
    expression.each_child(&mut |child| inspect_abstract_apply(child, found));
}

#[test]
fn abstract_application_shares_state_field_observations() {
    let module = lower_text("Test", "module Test exposing (repeat)\ntype alias Reveal a = Errors {a | none}\nopaque type alias Repeat a s = Eff {Scope s} none (Reveal (Repeat (List a) s))\nrepeat : Repeat Int s\nforeign pure repeat from \"flick:rt\" \"repeat\"\n").expect("recursive original declaration");
    let mut found = false;
    for item in &module.items {
        if let ItemKind::Value { body, .. } = &item.kind {
            inspect_abstract_apply(body, &mut found);
        }
    }
    assert!(found, "generated abstract application dispatcher");
}

#[test]
fn productivity_fallback_is_shared_instead_of_copied_per_type_node() {
    let module = lower_text("Test", "module Test exposing (repeat)\nopaque type alias Id a = a\nopaque type alias Repeat a s = Eff {Scope s} none (Id (Repeat (List a) s))\nrepeat : Repeat Int s\nforeign pure repeat from \"flick:rt\" \"repeat\"\n").expect("recursive original declaration");
    let mut found = false;
    for item in &module.items {
        if let ItemKind::Value { body, .. } = &item.kind {
            inspect(body, &mut found);
        }
    }
    assert!(found, "generated productivity dispatcher");
}

fn dynamic_descriptor_calls(expression: &Expr, operation: Op) -> usize {
    let mut count = usize::from(matches!(&expression.kind,
        ExprKind::App { arg, .. } if matches!(&arg.kind, ExprKind::Tuple(fields)
            if matches!(fields.first().map(|field| &field.kind),
                Some(ExprKind::Lit(Lit::Int(op))) if *op == operation as i32)
                && matches!(&fields[1].kind, ExprKind::Tuple(desc)
                    if desc.len() == 2 && desc.iter().all(|field| matches!(field.kind, ExprKind::Local(_)))))));
    expression.each_child(&mut |child| count += dynamic_descriptor_calls(child, operation));
    count
}

fn alias_dispatch_fixture() -> OwnerTypeTemplates {
    let names = ["A", "B", "C", "D", "E", "F", "G", "H"];
    let arities = [1, 1, 1, 2, 1, 1, 1, 2];
    // Only A and B have the exact same key. C changes row order, D arity,
    // E a result, and F an argument. G/H differ only in arity, with no rows.
    // Parameter spellings are not observed.
    let mappings = [
        vec![(vec![1], 2), (vec![3], 4)],
        vec![(vec![1], 2), (vec![3], 4)],
        vec![(vec![3], 4), (vec![1], 2)],
        vec![(vec![1, 6], 2), (vec![3, 7], 4)],
        vec![(vec![1], 9), (vec![3], 4)],
        vec![(vec![5], 2), (vec![3], 4)],
        vec![],
        vec![],
    ];
    let aliases = names.iter().enumerate().map(|(id, name)| serde_json::json!({
        "module": "Test", "name": name, "body": 0,
        "params": (0..arities[id]).map(|index| format!("p{id}_{index}")).collect::<Vec<_>>(),
    })).collect::<Vec<_>>();
    let exprs = names
        .iter()
        .map(|name| {
            serde_json::json!({
                "kind": { "Con": { "module": "Test", "name": name } }, "children": [],
            })
        })
        .collect::<Vec<_>>();
    let rows = mappings
        .iter()
        .enumerate()
        .flat_map(|(alias, rows)| {
            rows.iter().map(move |(arguments, value)| {
                serde_json::json!({
                    "alias": alias, "arguments": arguments, "value": value,
                })
            })
        })
        .collect::<Vec<_>>();
    serde_json::from_value(serde_json::json!({
        "root": 0, "recursive": true, "cyclic_aliases": [],
        "dynamic_con": 0, "dynamic_var": 0, "never": 0,
        "exprs": exprs, "aliases": aliases,
        "productivity": { "root": 0, "names": [], "labels": [], "values": [], "aliases": rows },
    }))
    .expect("synthetic exact-row dispatch fixture")
}

#[test]
fn abstract_alias_sharing_uses_exact_certificate_rows_and_preserves_original_descriptors() {
    let templates = alias_dispatch_fixture();
    let mut build = Build {
        templates: &templates,
        run: "$owner".into(),
        impossible: "$owner$invalid".into(),
        span: Span::new(FileId::new(0), 0, 0),
        next: 0,
    };
    let expression = build.abstract_type(&build.local("original"));
    let arms = node_arms(&expression);
    let mut covered = BTreeSet::new();
    let groups = arms
        .iter()
        .filter(|arm| !matches!(arm.pat, Pat::Wild))
        .map(|arm| {
            known_ids(&arm.pat, &mut covered);
            let mut ids = BTreeSet::new();
            known_ids(&arm.pat, &mut ids);
            (ids, &arm.body)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        covered,
        (0..8).collect(),
        "original IDs remain complete and disjoint"
    );
    assert_eq!(
        groups.len(),
        7,
        "only identical arity and ordered rows may share a body"
    );
    let shared = groups
        .iter()
        .find(|(ids, _)| *ids == BTreeSet::from([0, 1]))
        .expect("A and B share one dispatch body");
    assert_eq!(
        dynamic_descriptor_calls(shared.1, Op::AbstractApplyAll),
        1,
        "the shared body forwards the original dynamic (node,env), never the representative node"
    );
    let invalid = arms.last().expect("invalid node branch");
    assert!(matches!(invalid.pat, Pat::Wild));
    assert!(matches!(&invalid.body.kind, ExprKind::App { func, .. }
        if matches!(&func.kind, ExprKind::Local(name) if name == "$owner$invalid")));
}

#[test]
fn head_alias_sharing_uses_exact_parameter_names_and_keeps_body_ids_as_data() {
    let mut templates = alias_dispatch_fixture();
    templates.aliases[1].params = templates.aliases[0].params.clone();
    templates.aliases[0].body = 4;
    templates.aliases[1].body = 5;
    let mut build = Build {
        templates: &templates,
        run: "$owner".into(),
        impossible: "$owner$invalid".into(),
        span: Span::new(FileId::new(0), 0, 0),
        next: 0,
    };
    let expression = build.head(
        build.local("original"),
        build.list(Vec::new()),
        build.unit(),
    );
    let arms = node_arms(&expression);
    let mut covered = BTreeSet::new();
    let groups = arms
        .iter()
        .filter(|arm| !matches!(arm.pat, Pat::Wild))
        .map(|arm| {
            known_ids(&arm.pat, &mut covered);
            let mut ids = BTreeSet::new();
            known_ids(&arm.pat, &mut ids);
            (ids, &arm.body)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        covered,
        (0..8).collect(),
        "all original node IDs remain disjoint and complete"
    );
    assert_eq!(
        groups.len(),
        7,
        "only exact parameter vectors share the alias environment"
    );
    let shared = groups
        .iter()
        .find(|(ids, _)| *ids == BTreeSet::from([0, 1]))
        .expect("equal parameter vectors share a body despite distinct original body IDs");
    assert_eq!(
        dynamic_descriptor_calls(shared.1, Op::AliasBody),
        1,
        "the shared alias body lookup receives the original dynamic descriptor"
    );
    let invalid = arms.last().expect("invalid node branch");
    assert!(matches!(invalid.pat, Pat::Wild));
    assert!(matches!(&invalid.body.kind, ExprKind::App { func, .. }
        if matches!(&func.kind, ExprKind::Local(name) if name == "$owner$invalid")));
    let lookup = build.alias_body(build.local("original"));
    let body_ids = node_arms(&lookup)
        .iter()
        .filter_map(|arm| match (&arm.pat, &arm.body.kind) {
            (Pat::Lit(Lit::Int(node)), ExprKind::Lit(Lit::Int(body))) => Some((*node, *body)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        body_ids,
        vec![
            (0, 4),
            (1, 5),
            (2, 0),
            (3, 0),
            (4, 0),
            (5, 0),
            (6, 0),
            (7, 0)
        ],
        "the lookup retains each original alias body ID rather than choosing its group's representative"
    );
}

#[test]
fn atom_sharing_preserves_exact_names_constructor_modes_and_dynamic_nodes() {
    let mut templates = alias_dispatch_fixture();
    templates.exprs = serde_json::from_value(serde_json::json!([
        { "kind": { "Con": { "module": "Test", "name": "X" } }, "children": [] },
        { "kind": { "Con": { "module": "Test", "name": "X" } }, "children": [] },
        { "kind": { "Var": "Test.X" }, "children": [] },
        { "kind": { "Atom": { "identity": "rigid", "atom": "Test.X" } }, "children": [] },
        { "kind": "Tuple", "children": [] },
        { "kind": "App", "children": [0, 2] },
        { "kind": "DynamicCon", "children": [] },
        { "kind": "DynamicVar", "children": [] }
    ]))
    .expect("exact observable atom metadata");
    let mut build = Build {
        templates: &templates,
        run: "$owner".into(),
        impossible: "$owner$invalid".into(),
        span: Span::new(FileId::new(0), 0, 0),
        next: 0,
    };
    let expression = build.atom(build.local("original"), &build.local("mode"));
    let arms = node_arms(&expression);
    let mut covered = BTreeSet::new();
    let groups = arms
        .iter()
        .filter(|arm| !matches!(arm.pat, Pat::Wild))
        .map(|arm| {
            known_ids(&arm.pat, &mut covered);
            let mut ids = BTreeSet::new();
            known_ids(&arm.pat, &mut ids);
            (ids, &arm.body)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        covered,
        (0..8).collect(),
        "all original IDs stay complete and disjoint"
    );
    assert_eq!(
        groups.len(),
        5,
        "only exact observable static atoms share a body"
    );
    let group = |expected| {
        groups
            .iter()
            .find(|(ids, _)| *ids == BTreeSet::from(expected))
            .expect("exact atom group")
            .1
    };
    assert!(
        matches!(&group([0, 1]).kind, ExprKind::If(condition, yes, no)
        if matches!(&condition.kind, ExprKind::Binary(BinOp::Eq, mode, two)
            if matches!(&mode.kind, ExprKind::Local(name) if name == "mode")
                && matches!(two.kind, ExprKind::Lit(Lit::Int(2))))
            && matches!(yes.kind, ExprKind::Lit(Lit::Unit))
            && matches!(&no.kind, ExprKind::Lit(Lit::String(name)) if name == "Test.X")),
        "constructors still produce Unit in capture mode and the full name otherwise"
    );
    let ExprKind::Match(mode, variable_modes) = &group([2, 3]).kind else {
        panic!("variable atom modes")
    };
    assert!(matches!(&mode.kind, ExprKind::Local(name) if name == "mode"));
    assert!(matches!(
        &variable_modes[..],
        [
            Arm {
                pat: Pat::Lit(Lit::Int(0)),
                ..
            },
            Arm {
                pat: Pat::Lit(Lit::Int(1)),
                ..
            },
            Arm { pat: Pat::Wild, .. }
        ]
    ));
    let prefixes = variable_modes.iter().map(|arm| match &arm.body.kind {
        ExprKind::Binary(BinOp::Concat, prefix, name)
            if matches!(&name.kind, ExprKind::Lit(Lit::String(name)) if name == "Test.X") => {
                let ExprKind::Lit(Lit::String(prefix)) = &prefix.kind else { panic!("atom prefix") };
                prefix.as_str()
            }
        _ => panic!("complete variable atom mode"),
    }).collect::<Vec<_>>();
    assert_eq!(prefixes, ["~", "@", "^"]);
    assert!(matches!(group([4, 5]).kind, ExprKind::Lit(Lit::Unit)));
    assert!(groups.iter().any(|(ids, _)| *ids == BTreeSet::from([6])));
    assert!(groups.iter().any(|(ids, _)| *ids == BTreeSet::from([7])));
    let invalid = arms.last().expect("invalid node branch");
    assert!(matches!(invalid.pat, Pat::Wild));
    assert!(matches!(&invalid.body.kind, ExprKind::App { func, .. }
        if matches!(&func.kind, ExprKind::Local(name) if name == "$owner$invalid")));
}
