use super::*;
use crate::diagnostics::Severity;
use crate::types::Elem;
use crate::types::ty::{Binder, Effects};
use crate::types::unify::test_checker::with_source;
use crate::{ir, resolve};

const SOURCE: &str =
    "module Probe exposing ()\nforeign pure delayed from \"host\"\n  retention call\n";

fn foreign(tree: &Node) -> &Node {
    tree.nodes()
        .find(|node| node.kind == K::ForeignDecl)
        .expect("foreign declaration")
}

fn scheme(body: Type, scoped: bool) -> Type {
    if scoped {
        Type::Forall {
            vars: vec![Binder {
                name: "s".into(),
                kind: Kind::Region,
            }],
            constraints: Vec::new(),
            body: Box::new(body),
        }
    } else {
        body
    }
}

#[test]
fn late_function_data_and_scope_solutions_reextract_every_foreign_field() {
    let list = Type::app(Type::lang(Lang::List), Type::lang(Lang::Int));
    let data = Type::pure_fun(list.clone(), list.clone());
    let callback = Type::pure_fun(
        Type::pure_fun(
            Type::app(Type::lang(Lang::Option), Type::lang(Lang::Int)),
            list,
        ),
        Type::lang(Lang::Int),
    );
    let scope = Type::Fun {
        param: Box::new(Type::lang(Lang::Unit)),
        effects: Some(Box::new(Effects::new(
            Type::Row(super::super::ty::Row::closed(vec![(
                Elem::named(lang::label(Lang::Scope).expect("Scope label")),
                Type::app(Type::lang(Lang::Scope), Type::Var("s".into())),
            )])),
            Type::empty_row(),
        ))),
        result: Box::new(Type::lang(Lang::Unit)),
    };
    for (body, scoped) in [(data, false), (callback, false), (scope, true)] {
        let source = if scoped {
            SOURCE.replace("foreign pure", "foreign effect")
        } else {
            SOURCE.into()
        };
        let expected = with_source(&source, |checker, tree| {
            checker.foreign_info(foreign(tree), &scheme(body.clone(), scoped));
            let typing = checker.finish_typing();
            assert!(checker.diagnostics.is_empty(), "{:?}", checker.diagnostics);
            serde_json::to_value(
                typing
                    .foreigns
                    .get(&foreign(tree).range())
                    .expect("final info"),
            )
            .expect("closed metadata")
        });
        with_source(&source, |checker, tree| {
            let pending = checker.fresh();
            let original = scheme(pending.clone(), scoped);
            checker.foreign_info(foreign(tree), &original);
            assert!(
                checker.typing.foreigns.is_empty(),
                "unknown representation must not publish a completed negative policy"
            );
            checker
                .unify(&pending, &body)
                .expect("later closed solution");
            let typing = checker.finish_typing();
            assert!(checker.diagnostics.is_empty(), "{:?}", checker.diagnostics);
            let actual = serde_json::to_value(
                typing
                    .foreigns
                    .get(&foreign(tree).range())
                    .expect("late finalized info"),
            )
            .expect("closed metadata");
            assert_eq!(
                actual, expected,
                "original declaration must be re-extracted, not patched or inferred from arguments"
            );
        });
    }
}

#[test]
fn an_unresolved_original_declaration_cannot_lower_or_publish_metadata() {
    with_source(SOURCE, |checker, tree| {
        let original = checker.fresh();
        checker.foreign_info(foreign(tree), &original);
        let typing = checker.finish_typing();
        assert!(typing.foreigns.is_empty());
        assert!(
            serde_json::to_string(&original).is_err(),
            "unfinished type is not an artifact"
        );
        let pending = typing.pending_foreign_diagnostics();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].code, codes::T013);
        assert_eq!(pending[0].span, Some(checker.span(foreign(tree))));
        assert_eq!(
            checker
                .diagnostics
                .iter()
                .filter(|d| d.code == codes::T013)
                .count(),
            1
        );
        let syntax = resolve::module_syntax(tree, SOURCE, checker.file);
        let module = resolve::Module {
            name: "Probe",
            file: checker.file,
            text: SOURCE,
            tree,
            syntax: &syntax,
            generated: resolve::NO_GENERATED,
        };
        let rejected = ir::lower(&module, &typing).expect_err("pending is not default ForeignInfo");
        assert_eq!(rejected.len(), 1);
        assert_eq!(rejected[0].code, codes::T013);
        assert_eq!(rejected[0].span, pending[0].span);
        let debug = format!("{:?}", typing.pending_foreigns);
        assert!(!debug.contains("ScopeId"));
        assert!(!debug.contains("MetaOccurrence"));
    });
}

#[test]
fn accepted_source_catch_mapping_is_inferred_once_before_finalization() {
    const TEXT: &str = "module Probe exposing ()\ntype error Broken = Broken String\nfrom_js : JsValue -> Broken\nlet from_js _ = Broken \"mapped\"\ndelayed : Unit ->{; Broken} Unit\nforeign delayed from \"host\"\n  catch from_js\n";
    with_source(TEXT, |checker, tree| {
        checker.module(tree);
        assert!(
            !checker
                .diagnostics
                .iter()
                .any(|d| d.severity == Severity::Error),
            "{:?}",
            checker.diagnostics
        );
        assert!(checker.typing.foreigns.is_empty());
        let mark = checker.fresh().meta_root().expect("allocation mark");
        let typing = checker.finish_typing();
        assert!(
            !checker
                .diagnostics
                .iter()
                .any(|d| d.severity == Severity::Error),
            "{:?}",
            checker.diagnostics
        );
        assert_eq!(
            checker.fresh().meta_root(),
            Some(mark + 1),
            "finalization must not infer catch a second time"
        );
        assert!(typing.foreigns[&foreign(tree).range()].catch.is_some());
        assert!(typing.pending_foreign_diagnostics().is_empty());
    });
}

#[test]
fn late_alias_body_and_binder_kind_solutions_are_not_frozen_in_owner_templates() {
    use crate::types::{Form, TypeDef};
    let body = Type::pure_fun(Type::lang(Lang::Unit), Type::lang(Lang::Int));
    let known_alias = with_source(SOURCE, |checker, tree| {
        checker.env.define_type(
            "Later",
            TypeDef {
                kind: Kind::Type,
                form: Form::Alias {
                    params: Vec::new(),
                    body: body.clone(),
                    opaque: false,
                },
            },
        );
        checker.foreign_info(foreign(tree), &Type::con("Probe", "Later"));
        let typing = checker.finish_typing();
        serde_json::to_value(&typing.foreigns[&foreign(tree).range()])
            .expect("closed alias metadata")
    });
    with_source(SOURCE, |checker, tree| {
        let hole = checker.fresh();
        checker.env.define_type(
            "Later",
            TypeDef {
                kind: Kind::Type,
                form: Form::Alias {
                    params: Vec::new(),
                    body: hole.clone(),
                    opaque: false,
                },
            },
        );
        checker.foreign_info(foreign(tree), &Type::con("Probe", "Later"));
        checker.unify(&hole, &body).expect("later alias body");
        let typing = checker.finish_typing();
        assert!(typing.pending_foreign_diagnostics().is_empty());
        assert_eq!(
            serde_json::to_value(&typing.foreigns[&foreign(tree).range()])
                .expect("closed alias metadata"),
            known_alias
        );
    });
    with_source(SOURCE, |checker, tree| {
        let kind = checker.kinds.fresh();
        let original = Type::Forall {
            vars: vec![Binder {
                name: "a".into(),
                kind: kind.clone(),
            }],
            constraints: Vec::new(),
            body: Box::new(body.clone()),
        };
        checker.foreign_info(foreign(tree), &original);
        checker
            .kinds
            .unify(&kind, &Kind::Type)
            .expect("later binder kind");
        let typing = checker.finish_typing();
        assert!(typing.pending_foreign_diagnostics().is_empty());
        assert!(typing.foreigns.contains_key(&foreign(tree).range()));
    });
}

#[test]
fn a_late_concrete_capture_label_does_not_remain_permanently_pending() {
    use crate::types::{Atom, Elided, Row};
    with_source(SOURCE, |checker, tree| {
        let hole = checker.fresh();
        let body = Type::Captured {
            ty: Box::new(Type::pure_fun(
                Type::lang(Lang::Unit),
                Type::lang(Lang::Int),
            )),
            captures: Box::new(Type::Row(Row::closed(vec![(
                Elem::Capture(Atom::meta(hole.meta_root().expect("root"))),
                hole.clone(),
            )]))),
            value: false,
            elided: Elided::default(),
        };
        checker.foreign_info(foreign(tree), &body);
        checker
            .unify(&hole, &Type::lang(Lang::String))
            .expect("closed data captures nothing");
        let typing = checker.finish_typing();
        assert!(
            typing.pending_foreign_diagnostics().is_empty(),
            "solved capture atom is not an unresolved artifact"
        );
        assert!(!checker.diagnostics.iter().any(|d| d.code == codes::T013));
        assert!(typing.foreigns.contains_key(&foreign(tree).range()));
    });
}

#[test]
fn a_late_constructor_kind_is_checked_without_defaulting_or_dropping_metadata() {
    use crate::types::{Form, TypeDef};
    with_source(SOURCE, |checker, tree| {
        let kind = checker.kinds.fresh();
        checker.env.define_type(
            "Later",
            TypeDef {
                kind: kind.clone(),
                form: Form::Nominal {
                    literals: Vec::new(),
                    hidden: false,
                },
            },
        );
        let original = Type::pure_fun(Type::con("Probe", "Later"), Type::lang(Lang::Unit));
        checker.foreign_info(foreign(tree), &original);
        assert!(checker.owner_type_is_pending(&original));
        checker
            .kinds
            .unify(&kind, &Kind::Type)
            .expect("completed original constructor kind");
        let typing = checker.finish_typing();
        assert!(!checker.owner_type_is_pending(&original));
        assert!(typing.pending_foreign_diagnostics().is_empty());
        assert!(typing.foreigns.contains_key(&foreign(tree).range()));
    });
}
