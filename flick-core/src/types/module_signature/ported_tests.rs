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
