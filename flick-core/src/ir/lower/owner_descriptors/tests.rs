use std::collections::BTreeSet;

use super::{Build, Op, alias_groups};
use crate::ir::tests::lower_text;
use crate::ir::{Arm, BinOp, Expr, ExprKind, ItemKind, Lit, Pat, Stmt};
use crate::source::{FileId, Span};
use crate::types::OwnerTypeTemplates;

mod resolution {
    include!("tests/resolution.rs");
}
mod membership {
    include!("tests/membership.rs");
}

fn builder(templates: &OwnerTypeTemplates) -> Build<'_> {
    let (alias_groups, alias_representatives) = alias_groups(templates);
    let mut head_representatives = Vec::<usize>::new();
    let head_groups = templates
        .aliases
        .iter()
        .enumerate()
        .map(|(id, alias)| {
            head_representatives
                .iter()
                .position(|&other| templates.aliases[other].params == alias.params)
                .unwrap_or_else(|| {
                    head_representatives.push(id);
                    head_representatives.len() - 1
                })
        })
        .collect();
    Build {
        templates,
        run: "$owner".into(),
        temporaries: "$ot".into(),
        impossible: "$owner$invalid".into(),
        span: Span::new(FileId::new(0), 0, 0),
        next: 0,
        alias_groups,
        alias_representatives,
        head_groups,
        head_representatives,
    }
}

fn node_arms(expression: &Expr) -> &[Arm] {
    match &expression.kind {
        ExprKind::Block(_, body) | ExprKind::If(_, body, _) => node_arms(body),
        ExprKind::Match(_, arms) => arms,
        _ => panic!("descriptor dispatch"),
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
        _ => panic!("integer dispatch pattern"),
    }
}

fn calls(expression: &Expr, operation: Op) -> Vec<Vec<Expr>> {
    let mut found = Vec::new();
    if let ExprKind::App { arg, .. } = &expression.kind
        && let ExprKind::Tuple(fields) = &arg.kind
        && matches!(fields.first().map(|field| &field.kind), Some(ExprKind::Lit(Lit::Int(id))) if *id == operation as i32)
    {
        found.push(fields.clone());
    }
    expression.each_child(&mut |child| found.extend(calls(child, operation)));
    found
}

fn count(expression: &Expr, predicate: fn(&Expr) -> bool) -> usize {
    let mut found = usize::from(predicate(expression));
    expression.each_child(&mut |child| found += count(child, predicate));
    found
}

fn descriptor_parts(expression: &Expr) -> (&str, &[Pat], &Expr) {
    if let ExprKind::Block(statements, body) = &expression.kind {
        if let [Stmt::Let(Pat::Bind(current), source)] = &statements[..]
            && let ExprKind::Block(unpack, _) = &body.kind
            && let [Stmt::Let(Pat::Tuple(fields), value)] = &unpack[..]
            && matches!(&value.kind, ExprKind::Local(name) if name == current)
        {
            return (current, fields, source);
        }
        return descriptor_parts(body);
    }
    panic!("the original descriptor is bound once before its node and environment are unpacked");
}

fn assert_current(expression: &Expr, current: &str) {
    assert!(
        matches!(&expression.kind, ExprKind::Local(name) if name == current),
        "forward the original descriptor, including its lazy environment, rather than a representative ID"
    );
}

fn inspect(expression: &Expr, found: &mut bool) {
    if let ExprKind::Lambda { params, body } = &expression.kind
        && params.len() == 1
        && matches!(&params[0].pat, Pat::Tuple(fields) if fields.len() == 4)
        && let ExprKind::Match(_, operations) = &body.kind
        && let Some(productive) = operations
            .iter()
            .find(|arm| membership::matches(&arm.pat, Op::Productive as i32))
    {
        *found = true;
        assert_eq!(
            calls(&productive.body, Op::Abstract).len(),
            0,
            "productivity must tail-dispatch its fallback instead of copying full observations per node"
        );
        membership::assert_operation_domain(operations);
        membership::assert_exact_node_domains(operations);
        let application = operations
            .iter()
            .find(|arm| membership::matches(&arm.pat, Op::AbstractApply as i32))
            .expect("abstract application");
        assert_eq!(
            count(
                &application.body,
                |e| matches!(&e.kind, ExprKind::If(condition, ..) if matches!(condition.kind, ExprKind::Lit(Lit::Bool(_))))
            ),
            0,
            "known abstract-state branches are selected before generating the interpreter"
        );
        let dictionary = operations
            .iter()
            .find(|arm| membership::matches(&arm.pat, Op::Dictionary as i32))
            .expect("dictionary operation");
        assert_eq!(
            count(
                &dictionary.body,
                |e| matches!(&e.kind, ExprKind::Binary(BinOp::Eq, left, right) if matches!(left.kind, ExprKind::Lit(_)) && matches!(right.kind, ExprKind::Lit(_)))
            ),
            0,
            "declared constructor classification does not repeat constant comparisons at runtime"
        );
    }
    expression.each_child(&mut |child| inspect(child, found));
}

#[test]
fn productivity_fallback_and_static_classification_remain_shared() {
    let module = lower_text("Test", "module Test exposing (repeat)\ntype alias Reveal a = Errors {a | none}\nopaque type alias Id a = a\nopaque type alias Repeat a s = Eff {Scope s} none (Id (Reveal (Repeat (List a) s)))\nrepeat : Repeat Int s\nforeign pure repeat from \"flick:rt\" \"repeat\"\n").expect("recursive original declaration");
    let mut found = false;
    for item in &module.items {
        if let ItemKind::Value { body, .. } = &item.kind {
            inspect(body, &mut found);
        }
    }
    assert!(found, "generated owner dispatcher");
}

fn alias_dispatch_fixture() -> OwnerTypeTemplates {
    let names = ["A", "B", "C", "D", "E", "F", "G", "H"];
    let arities = [1, 1, 1, 2, 1, 1, 1, 2];
    // A/B/C have the same unique argument-to-result map in different orders.
    // D changes arity, E a result, F an argument; G/H have different empty arities.
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
    .expect("exact certified-map fixture")
}

fn certificate_rows(expression: &Expr) -> Vec<(Vec<i32>, i32)> {
    let mut rows = Vec::new();
    if let ExprKind::Match(_, arms) = &expression.kind {
        for arm in arms {
            if let (Pat::Tuple(arguments), ExprKind::Lit(Lit::Int(value))) =
                (&arm.pat, &arm.body.kind)
            {
                let arguments = arguments
                    .iter()
                    .map(|argument| match argument {
                        Pat::Lit(Lit::Int(id)) => *id,
                        _ => panic!("exact certificate argument ID"),
                    })
                    .collect();
                rows.push((arguments, *value));
            }
        }
    }
    expression.each_child(&mut |child| rows.extend(certificate_rows(child)));
    rows
}

#[test]
fn abstract_alias_groups_preserve_exact_certificate_maps_and_original_descriptors() {
    let templates = alias_dispatch_fixture();
    let mut build = builder(&templates);
    assert_eq!(build.alias_groups, [0, 0, 0, 1, 2, 3, 4, 5]);
    assert_eq!(build.alias_representatives, [0, 3, 4, 5, 6, 7]);
    let expression = build.abstract_type(&build.local("original"));
    let (current, _, _) = descriptor_parts(&expression);
    let mut covered = BTreeSet::new();
    for arm in node_arms(&expression) {
        known_ids(&arm.pat, &mut covered);
    }
    assert_eq!(
        covered,
        (0..8).collect(),
        "all original IDs remain disjoint and complete"
    );
    for id in 0..8 {
        let branch = membership::select(&expression, id).expect("original node");
        let observed = calls(branch, Op::AbstractAlias);
        assert_eq!(observed.len(), 1);
        assert_current(&observed[0][1], current);
        let index = usize::try_from(id).expect("nonnegative original alias node");
        let expected_group = i32::try_from(build.alias_groups[index]).expect("small alias group");
        assert!(
            matches!(observed[0][3].kind, ExprKind::Lit(Lit::Int(group)) if group == expected_group)
        );
    }
    membership::assert_unknown(&expression, 8);
    let dispatch = build.operation(
        Op::AbstractAlias,
        build.local("original"),
        build.local("arguments"),
        build.local("group"),
    );
    for group in 0..6 {
        let branch = membership::select(&dispatch, group).expect("certified group");
        let rows = certificate_rows(branch);
        let index = usize::try_from(group).expect("nonnegative certified group");
        let alias = build.alias_representatives[index];
        let mut expected = templates
            .productivity
            .as_ref()
            .expect("fixture productivity certificate")
            .aliases
            .iter()
            .filter(|row| row.alias == alias)
            .map(|row| {
                (
                    row.arguments
                        .iter()
                        .map(|&id| {
                            i32::try_from(id).expect("certificate argument fits owner index")
                        })
                        .collect::<Vec<_>>(),
                    i32::try_from(row.value).expect("certificate result fits owner index"),
                )
            })
            .collect::<Vec<_>>();
        let mut actual = rows;
        actual.sort();
        expected.sort();
        assert_eq!(
            actual, expected,
            "group {group} retains every certificate argument and result ID"
        );
        let arms = node_arms(branch);
        assert!(
            matches!(arms[0].pat, Pat::List(ref fields, Some(_)) if fields.len() == templates.aliases[alias].params.len())
        );
        membership::assert_impossible(&arms.last().expect("alias argument fallback").body);
        let forwards = calls(branch, Op::AbstractApplyAll);
        assert_eq!(forwards.len(), 1);
        assert_current(&forwards[0][1], "original");
    }
    membership::assert_unknown(&dispatch, 6);
}

#[test]
fn head_alias_groups_keep_exact_parameter_names_and_original_body_ids() {
    let mut templates = alias_dispatch_fixture();
    templates.aliases[1].params = templates.aliases[0].params.clone();
    templates.aliases[0].body = 4;
    templates.aliases[1].body = 5;
    let mut build = builder(&templates);
    assert_eq!(build.head_groups, [0, 0, 1, 2, 3, 4, 5, 6]);
    let expression = build.head(
        build.local("original"),
        build.list(Vec::new()),
        build.unit(),
    );
    let (current, _, _) = descriptor_parts(&expression);
    for id in 0..8 {
        let branch = membership::select(&expression, id).expect("original alias node");
        let observed = calls(branch, Op::HeadAlias);
        assert_eq!(observed.len(), 1);
        assert_current(&observed[0][1], current);
        let index = usize::try_from(id).expect("nonnegative original head node");
        let expected_group = i32::try_from(build.head_groups[index]).expect("small head group");
        let expected_body =
            i32::try_from(templates.aliases[index].body).expect("alias body fits owner index");
        assert!(
            matches!(&observed[0][3].kind, ExprKind::Tuple(data)
            if matches!(data[0].kind, ExprKind::Lit(Lit::Int(group)) if group == expected_group)
            && matches!(data[1].kind, ExprKind::Lit(Lit::Int(body)) if body == expected_body)),
            "each occurrence supplies its original body ID independently of its shared parameter group"
        );
    }
    membership::assert_unknown(&expression, 8);
    let dispatch = build.head_alias(
        &build.local("original"),
        &build.local("arguments"),
        build.local("data"),
    );
    let ExprKind::Block(unpack, _) = &dispatch.kind else {
        panic!("head group data");
    };
    let Stmt::Let(Pat::Tuple(data), _) = &unpack[0] else {
        panic!("group and body");
    };
    let Pat::Bind(body) = &data[1] else {
        panic!("original body binding");
    };
    for group in 0..7 {
        let branch = membership::select(&dispatch, group).expect("parameter group");
        let arms = node_arms(branch);
        let index = usize::try_from(group).expect("nonnegative parameter group");
        let alias = build.head_representatives[index];
        assert!(
            matches!(&arms[0].pat, Pat::List(fields, Some(_)) if fields.len() == templates.aliases[alias].params.len())
        );
        assert!(
            matches!(&arms.last().expect("partial head fallback").body.kind, ExprKind::Tuple(fields)
            if matches!(&fields[0].kind, ExprKind::Local(name) if name == "original")
            && matches!(&fields[1].kind, ExprKind::Local(name) if name == "arguments")),
            "a partially applied head retains the original descriptor and arguments"
        );
        let observed = calls(&arms[0].body, Op::Head);
        assert_eq!(observed.len(), 1);
        assert!(
            matches!(&observed[0][1].kind, ExprKind::Tuple(fields)
            if matches!(&fields[0].kind, ExprKind::Local(name) if name == body)),
            "shared environments consume the original body ID supplied as data"
        );
        let ExprKind::Tuple(fields) = &observed[0][1].kind else {
            panic!("head descriptor");
        };
        let ExprKind::Lambda {
            body: environment, ..
        } = &fields[1].kind
        else {
            panic!("parameter environment");
        };
        let parameters = node_arms(environment)
            .iter()
            .filter_map(|arm| match &arm.pat {
                Pat::Lit(Lit::String(name)) => Some(name.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            parameters, templates.aliases[alias].params,
            "parameter spellings remain exact"
        );
    }
    membership::assert_unknown(&dispatch, 7);
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
    .expect("observable atom metadata");
    let mut build = builder(&templates);
    let expression = build.atom(build.local("original"), &build.local("mode"));
    let mut covered = BTreeSet::new();
    for arm in node_arms(&expression) {
        known_ids(&arm.pat, &mut covered);
    }
    assert_eq!(covered, (0..8).collect());
    let constructor = membership::select(&expression, 0).expect("first static constructor node");
    assert_eq!(
        constructor,
        membership::select(&expression, 1).expect("duplicate static constructor node")
    );
    assert!(matches!(&constructor.kind, ExprKind::If(condition, yes, no)
        if matches!(&condition.kind, ExprKind::Binary(BinOp::Eq, mode, two)
            if matches!(&mode.kind, ExprKind::Local(name) if name == "mode") && matches!(two.kind, ExprKind::Lit(Lit::Int(2))))
        && matches!(yes.kind, ExprKind::Lit(Lit::Unit))
        && matches!(&no.kind, ExprKind::Lit(Lit::String(name)) if name == "Test.X")));
    let variable = membership::select(&expression, 2).expect("static variable node");
    assert_eq!(
        variable,
        membership::select(&expression, 3).expect("rigid atom node")
    );
    let ExprKind::Match(mode, modes) = &variable.kind else {
        panic!("variable modes");
    };
    assert!(matches!(&mode.kind, ExprKind::Local(name) if name == "mode"));
    assert!(matches!(modes[0].pat, Pat::Lit(Lit::Int(0))));
    assert!(matches!(modes[1].pat, Pat::Lit(Lit::Int(1))));
    assert!(matches!(modes[2].pat, Pat::Wild));
    let prefixes = modes.iter().map(|arm| match &arm.body.kind {
        ExprKind::Binary(BinOp::Concat, prefix, name) if matches!(&name.kind, ExprKind::Lit(Lit::String(name)) if name == "Test.X") => {
            let ExprKind::Lit(Lit::String(prefix)) = &prefix.kind else { panic!("atom prefix"); };
            prefix.as_str()
        }
        _ => panic!("complete atom mode"),
    }).collect::<Vec<_>>();
    assert_eq!(prefixes, ["~", "@", "^"]);
    assert!(matches!(
        membership::select(&expression, 4).expect("tuple node").kind,
        ExprKind::Lit(Lit::Unit)
    ));
    assert!(matches!(
        membership::select(&expression, 5)
            .expect("application node")
            .kind,
        ExprKind::Lit(Lit::Unit)
    ));
    assert_ne!(
        membership::select(&expression, 6).expect("dynamic constructor node"),
        membership::select(&expression, 7).expect("dynamic variable node")
    );
    membership::assert_unknown(&expression, 8);
}
