use crate::ir::tests::check_text;

#[test]
fn ported_review_dependent_variance_uses_expected_parameter_equalities() {
    let input = |manifest| ModuleSignature::Structure {
        requirements: vec![R::Type {
            name: "Key".into(),
            params: vec![],
            manifest,
        }],
    };
    let output = |ty| ModuleSignature::Structure {
        requirements: vec![R::Type {
            name: "Value".into(),
            params: vec![],
            manifest: Some(ty),
        }],
    };
    let provided = factory(input(None), output(Type::con("$module:K", "Key")));
    let expected = factory(
        input(Some(Type::con("Prelude", "String"))),
        output(Type::con("Prelude", "String")),
    );
    assert!(provided.is_subtype_of(&expected));
    assert!(!expected.is_subtype_of(&provided));
}

#[test]
fn ported_review_result_sealing_preserves_server_reachability_metadata() {
    let signature = structure(&["main"]);
    let mut actual = signature.interface("Web");
    actual.servers.push((
        "main".into(),
        vec!["Web.page".into(), "Page.load".into(), "Server.node".into()],
    ));
    assert_eq!(signature.seal(&actual, "Web").servers, actual.servers);
}

#[test]
fn ported_review_inferred_kind_witnesses_reconstruct_kind_declarations() {
    let (_, interface) = check_text("Modes", "module Modes exposing (Mode(..), Box, box, identity)\nkind Mode = Fast | Slow\ntype Box (mode : Mode) = Box\nbox : Box Fast\nlet box = Box\nidentity : forall mode. Box mode -> Box mode\nlet identity value = value\n").expect("finite-kind source checks");
    let signature = ModuleSignature::from_interface(&interface);
    let witness = signature.interface("Witness");
    assert_eq!(
        witness.find(Namespace::Type, "Mode").expect("kind").decl,
        Decl::Kind
    );
    assert_eq!(
        witness.find(Namespace::Type, "Fast").expect("member").decl,
        Decl::KindMember
    );
    assert!(signature.accepts(&witness));
}
#[test]
fn inferred_kind_members_shadow_outer_type_names() {
    let (_, child) = check_text("Child", "module Child exposing (Mode(..), Box, box)\nkind Mode = Fast | Slow\ntype Box (mode : Mode) = Box\nbox : Box Fast\nlet box = Box\n").expect("finite-kind child checks");
    let mut parent = structure(&[]).interface("Outer");
    let (_, outer) = check_text(
        "Outer",
        "module Outer exposing (Fast)\ntype alias Fast = Int\n",
    )
    .expect("outer type checks");
    parent.exports = outer.exports;
    parent.modules.insert("Child".into(), child);
    let signature = ModuleSignature::from_interface(&parent);
    let witness = signature.interface("Witness");
    let child = witness.modules.get("Child").expect("child witness");
    assert_eq!(
        child
            .find(Namespace::Value, "box")
            .and_then(|e| e.scheme.clone()),
        Some(Type::app(
            Type::con("Witness.Child", "Box"),
            Type::con("Witness.Child", "Fast")
        ))
    );
    assert!(signature.accepts(&witness));
}
