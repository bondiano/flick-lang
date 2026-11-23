use super::*;
use crate::run;
use flick_core::iface::{Decl, Export, Namespace, SupportType};
use flick_core::ir::{Expr, ExprKind, Lit};
use flick_core::source::{FileId, Span};
use flick_core::types::Form;

#[test]
fn explicit_fact_origin_precedes_builtin_container_and_namespace_is_part_of_identity() {
    let checked = run::compile_source(
        "module Facts exposing (..)\ntype alias A = Int\ntype alias B = String\ntype alias C = Bool\n",
    )
    .expect("checked declaration fixtures");
    let fact = |name: &str| {
        checked[0]
            .declarations
            .exports
            .iter()
            .find(|export| export.entity.name == name && export.entity.namespace == Namespace::Type)
            .expect("checked alias")
            .clone()
    };
    let (a, b, c) = (fact("A"), fact("B"), fact("C"));
    let support = |export: &Export| SupportType {
        entity: export.entity.clone(),
        definition: export.definition.clone().expect("alias definition"),
        owner_path: vec![],
    };
    let child = Interface {
        module: "Facts.Child".into(),
        private: true,
        ..Interface::default()
    };
    let mut explicit = Interface {
        module: "Facts".into(),
        exports: vec![b.clone()],
        support_types: vec![support(&a)],
        modules: BTreeMap::from([("Child".into(), child.clone())]),
        private: true,
        signature_explicit: true,
        ..Interface::default()
    };
    let mut value_a = a.clone();
    value_a.entity.namespace = Namespace::Value;
    value_a.decl = Decl::Let;
    value_a.definition = None;
    value_a.scheme = b.definition.as_ref().and_then(|definition| {
        if let Form::Alias { body, .. } = &definition.form {
            Some(body.clone())
        } else {
            None
        }
    });
    let builtin = Interface {
        module: "Facts".into(),
        exports: vec![a.clone(), value_a.clone()],
        support_types: vec![support(&b), support(&c)],
        modules: BTreeMap::from([(
            "Child".into(),
            Interface {
                module: "Facts.Child".into(),
                exports: vec![c.clone()],
                ..Interface::default()
            },
        )]),
        ..Interface::default()
    };
    fill_builtin(&mut explicit, &builtin);
    assert_eq!(explicit.exports, vec![b, value_a]);
    assert_eq!(explicit.support_types, vec![support(&a), support(&c)]);
    assert_eq!(explicit.modules["Child"], child);
    assert!(explicit.private);
    assert!(explicit.signature_explicit);
}

#[test]
fn partial_destructure_projection_preserves_initializer_checks_and_every_remaining_binder() {
    let alternative = Pat::Record(vec![
        (
            "first".into(),
            Pat::Tuple(vec![
                Pat::Bind("shadowed_head".into()),
                Pat::Lit(Lit::Int(7)),
            ]),
        ),
        (
            "rest".into(),
            Pat::List(
                vec![Pat::Bind("kept".into())],
                Some(Box::new(Pat::Bind("shadowed_tail".into()))),
            ),
        ),
    ]);
    let mut original = build::builtin_modules()
        .iter()
        .find(|module| module.name == "App")
        .expect("checked builtin container")
        .clone();
    original.items = vec![Item {
        kind: ItemKind::Destructure {
            pattern: Pat::Or(vec![alternative.clone(), alternative]),
            binder_types: Vec::new(),
            body: Expr::new(
                ExprKind::Lit(Lit::Unit),
                Span::new(FileId::default(), 11, 29),
            ),
        },
        span: Span::new(FileId::default(), 7, 31),
    }];
    let original_snapshot = original.clone();
    let resolved = fallback_module(
        &original,
        &BTreeSet::from([
            Name::new("App", "shadowed_head"),
            Name::new("App", "shadowed_tail"),
        ]),
    );
    assert_eq!(original, original_snapshot);
    assert_eq!(resolved.items.len(), 1);
    assert_eq!(resolved.items[0].span, original.items[0].span);
    let ItemKind::Destructure {
        pattern: Pat::Or(alternatives),
        body,
        ..
    } = &resolved.items[0].kind
    else {
        panic!("destructure checks retained");
    };
    let ItemKind::Destructure {
        body: original_body,
        ..
    } = &original.items[0].kind
    else {
        unreachable!()
    };
    assert_eq!(body, original_body);
    for alternative in alternatives {
        let mut names = Vec::new();
        alternative.binders(&mut names);
        assert_eq!(names, vec!["kept"]);
        let Pat::Record(fields) = alternative else {
            panic!("record check retained")
        };
        assert_eq!(
            fields[0].1,
            Pat::Tuple(vec![Pat::Wild, Pat::Lit(Lit::Int(7))])
        );
        assert_eq!(
            fields[1].1,
            Pat::List(vec![Pat::Bind("kept".into())], Some(Box::new(Pat::Wild)))
        );
    }
}

#[test]
fn disjoint_app_failure_fallback_is_retained_without_mutating_the_checked_root() {
    let checked =
        run::compile_source("module App exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n")
            .expect("checked App root");
    let snapshot = checked[0].declarations.clone();
    let resolved = declarations(&checked).expect("known builtin fallback");
    let app = resolved
        .iter()
        .find(|root| root.module == "App")
        .expect("resolved App");
    let builtin = build::builtin_declarations()
        .iter()
        .find(|root| root.module == "App")
        .expect("genuine builtin App");
    let fail = builtin
        .exports
        .iter()
        .find(|export| export.entity.name == "fail")
        .expect("builtin failure declaration");
    assert!(app.exports.contains(fail));
    assert_eq!(checked[0].declarations, snapshot);
    let explicit_ir = vec![checked[0].ir.clone()];
    let resolved = modules(&explicit_ir);
    let builtin = build::builtin_modules()
        .iter()
        .find(|module| module.name == "App")
        .expect("genuine builtin App IR");
    let fail = builtin
        .items
        .iter()
        .find(|item| globals(builtin, item).contains(&Name::new("App", "fail")))
        .expect("builtin failure global");
    assert!(
        resolved
            .iter()
            .any(|module| module.name == "App" && module.items.contains(fail))
    );
    assert_eq!(checked[0].ir, explicit_ir[0]);
}

#[test]
fn instance_globals_use_the_checked_owner_instead_of_the_container_module() {
    let builtin = build::builtin_modules()
        .iter()
        .find_map(|module| {
            module.items.iter().find_map(|item| {
                matches!(item.kind, ItemKind::Instance { .. }).then_some((module, item))
            })
        })
        .expect("real checked instance");
    let mut item = builtin.1.clone();
    let ItemKind::Instance { info, .. } = &mut item.kind else {
        unreachable!()
    };
    info.module = "Canonical.Owner".into();
    let expected = Name::new(&info.module, &format!("instance {}", info.name));
    assert_eq!(globals(builtin.0, &item), vec![expected]);
}
