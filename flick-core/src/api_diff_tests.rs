use super::*;
use crate::source::SourceMap;
use std::slice;

use crate::{resolve, syntax, types};

fn interface(text: &str) -> Interface {
    let mut sources = SourceMap::default();
    let file = sources.add("Lib.fl", text.to_owned());
    let parse = syntax::parse(text, file);
    let syntax = resolve::module_syntax(&parse.tree, text, file);
    let module = resolve::Module {
        name: "Lib",
        file,
        text,
        tree: &parse.tree,
        syntax: &syntax,
        generated: resolve::NO_GENERATED,
    };
    let builtin = |name: &str| types::builtins().iter().find(|i| i.module == name).cloned();
    let (mut interface, _) = resolve::resolve(&module, &builtin);
    let local = |n: &str| n == "Lib";
    let options = types::Options {
        local: &local,
        limits: types::Limits::default(),
        no_let_assert: false,
        thresholds: types::Thresholds::default(),
        effects: true,
        watched: types::Watched::none(),
        stages_by_class: false,
    };
    let _ = types::check(
        &module,
        &builtin,
        types::builtins(),
        options,
        &mut interface,
    );
    interface
}

#[test]
fn higher_order_signature_changes_are_public_api() {
    use crate::types::{ModuleRequirement as R, ModuleSignature as S};
    let signature = |binder: &str, input: &str| S::Parameterized {
        group_size: 1,
        binder: binder.into(),
        parameter: Box::new(S::Structure {
            requirements: vec![R::Value {
                name: "seed".into(),
                ty: Type::con("Prelude", input),
            }],
        }),
        result: Box::new(S::Structure {
            requirements: vec![R::Value {
                name: "answer".into(),
                ty: Type::con("Prelude", "Int"),
            }],
        }),
    };
    let old = Interface {
        module: "Lib.Make".into(),
        parameterized: Some(signature("K", "Int")),
        ..Interface::default()
    };
    let renamed = Interface {
        parameterized: Some(signature("Other", "Int")),
        ..old.clone()
    };
    assert!(diff(slice::from_ref(&old), &[renamed]).is_empty());
    let changed = Interface {
        parameterized: Some(signature("K", "String")),
        ..old.clone()
    };
    assert_eq!(bump(&diff(slice::from_ref(&old), &[changed])), Bump::Major);
}

#[test]
fn nested_static_members_are_public_api_but_template_bodies_are_not() {
    use crate::iface::ModuleTemplate;
    let child = interface("module Lib exposing (answer)\nanswer : Int\nlet answer = 42\n");
    let mut old = Interface {
        module: "Api".into(),
        ..Interface::default()
    };
    old.modules.insert("Child".into(), child);
    let mut changed = old.clone();
    changed
        .modules
        .get_mut("Child")
        .expect("child")
        .exports
        .clear();
    assert_eq!(bump(&diff(slice::from_ref(&old), &[changed])), Bump::Major);
    let mut implementation = old.clone();
    implementation.template = Some(ModuleTemplate {
        source: "implementation changes".into(),
        capture: None,
    });
    assert!(diff(&[old], &[implementation]).is_empty());
}

#[test]
fn private_static_artifacts_do_not_become_public_api() {
    let old = interface("module Lib exposing (answer)\nanswer : Int\nlet answer = 42\n");
    let mut private = old.clone();
    private.private = true;
    let mut changed = private.clone();
    changed.exports.clear();
    assert!(diff(slice::from_ref(&private), &[changed]).is_empty());
    assert_eq!(
        bump(&diff(slice::from_ref(&old), slice::from_ref(&private))),
        Bump::Major
    );
    assert_eq!(bump(&diff(&[private], &[old])), Bump::Minor);
}

#[test]
fn generated_members_are_compared_through_their_parent_view() {
    use crate::iface::StaticMember;
    let mut member = Interface {
        module: "Api.Hidden".into(),
        ..Interface::default()
    };
    member.member_of = Some(StaticMember {
        parent: "Api".into(),
        name: "Hidden".into(),
    });
    // A standalone implementation artifact is not another public module.
    assert!(diff(&[], &[member]).is_empty());
}

#[test]
fn canonical_service_atoms_follow_alpha_renaming() {
    use crate::types::{Binder, Kind};
    let scheme = |name: &str| Type::Forall {
        vars: vec![Binder {
            name: name.into(),
            kind: Kind::Service,
        }],
        constraints: Vec::new(),
        body: Box::new(Type::apps(
            Type::con("Prelude", "Eff"),
            [
                Type::Row(Row::closed(vec![(
                    format!("@{name}"),
                    Type::Var(name.into()),
                )])),
                Type::empty_row(),
                Type::con("Prelude", "Unit"),
            ],
        )),
    };
    assert_eq!(
        canonical_type(&scheme("first")),
        canonical_type(&scheme("second"))
    );
}

#[test]
fn removed_and_changed_exports_are_major_added_ones_minor() {
    let old = interface(
        "module Lib exposing (..)\n\nf : Float -> Float\nlet f x = x\n\ng : Float\nlet g = 1\n",
    );
    let new = interface(
        "module Lib exposing (..)\n\nf : String -> String\nlet f x = x\n\nh : Float\nlet h = 2\n",
    );
    let changes = diff(slice::from_ref(&old), &[new]);
    assert_eq!(
        changes[0].changes,
        [
            Change::Changed {
                name: "value f".into(),
                was: "Float -> Float".into(),
                now: "String -> String".into(),
            },
            Change::Removed("value g".into()),
            Change::Added("value h".into()),
        ]
    );
    assert_eq!(bump(&changes), Bump::Major);
    let more = interface(
        "module Lib exposing (..)\n\nf : Float -> Float\nlet f x = x\n\ng : Float\nlet g = 1\n\nk : Float\nlet k = 3\n",
    );
    assert_eq!(bump(&diff(slice::from_ref(&old), &[more])), Bump::Minor);
    assert_eq!(
        bump(&diff(slice::from_ref(&old), slice::from_ref(&old))),
        Bump::Patch
    );
    assert_eq!(next_version("1.2.3", Bump::Major).as_deref(), Some("2.0.0"));
    assert_eq!(next_version("0.2.3", Bump::Major).as_deref(), Some("0.3.0"));
    assert_eq!(next_version("1.2.3", Bump::Patch).as_deref(), Some("1.2.4"));
}

#[test]
fn a_removed_instance_or_a_stronger_context_is_major() {
    let thing = "module Lib exposing (..)\n\ntype Thing = Thing\n\ntype Box a = Box a\n\n";
    let base = interface(&format!(
        "{thing}instance Show Thing =\n  let show _ = \"thing\"\n\ninstance Show (Box a) =\n  let show _ = \"box\"\n"
    ));
    let removed = interface(&format!(
        "{thing}instance Show (Box a) =\n  let show _ = \"box\"\n"
    ));
    let changes = diff(slice::from_ref(&base), &[removed]);
    assert_eq!(
        changes[0].changes,
        [Change::Removed("instance Show Thing".into())]
    );
    assert_eq!(bump(&changes), Bump::Major);
    assert_eq!(
        next_version("1.2.3", bump(&changes)).as_deref(),
        Some("2.0.0")
    );
    let stronger = interface(&format!(
        "{thing}instance Show Thing =\n  let show _ = \"thing\"\n\ninstance Show a => Show (Box a) =\n  let show (Box x) = show x\n"
    ));
    assert_eq!(
        bump(&diff(slice::from_ref(&base), slice::from_ref(&stronger))),
        Bump::Major
    );
    // the reverse takes a constraint away: importers lose nothing
    assert_eq!(
        bump(&diff(slice::from_ref(&stronger), slice::from_ref(&base))),
        Bump::Minor
    );
    // the same instances in another order are the same API
    let moved = interface(&format!(
        "{thing}instance Show (Box a) =\n  let show _ = \"box\"\n\ninstance Show Thing =\n  let show _ = \"thing\"\n"
    ));
    assert_eq!(bump(&diff(slice::from_ref(&base), &[moved])), Bump::Patch);
}

#[test]
fn a_renamed_type_variable_is_no_change() {
    let api = |a: &str, b: &str| {
        interface(&format!(
            "module Lib exposing (..)\n\nf : {a} -> {b} -> {a}\nlet f x _ = x\n\n\
             type Box {a} = Box {a}\n\ntype alias Pair {a} = ({a}, {a})\n\n\
             class Sized {a} =\n  size : {a} -> Float\n\n\
             instance Show (Box {a}) =\n  let show _ = \"box\"\n"
        ))
    };
    let old = api("a", "b");
    assert_eq!(diff(slice::from_ref(&old), &[api("item", "other")]), []);
    assert_eq!(
        diff(slice::from_ref(&old), &[api("b", "a")]),
        [],
        "names swapped"
    );
    let changes = diff(slice::from_ref(&old), &[api("a", "a")]);
    assert_eq!(
        changes[0].changes,
        [Change::Changed {
            name: "value f".into(),
            was: "forall a b. a -> b -> a".into(),
            now: "forall a. a -> a -> a".into(),
        }]
    );
}
