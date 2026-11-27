//! The declarations of a type the JS backend may have its own of.

use super::Projection;
use super::tests::checked;
use crate::types;

/// A type the JS backend has its own of is a brand without constructors,
/// unless the build selects no `@native` clause: then its constructors are
/// its values, and the declaration says so.
#[test]
fn a_native_type_is_a_brand_where_the_build_selects_its_clause() {
    let text =
        "module App.Shapes exposing (Box(..))\n\ntype Box a = Box a\n  @native when js || go\n";
    let (interface, lookup) = checked(text);
    let package = |_: &str| Some("@acme/shapes".to_owned());
    let declared = |natives| {
        let projection = Projection::new("App.Shapes", &lookup, &package).selecting(natives);
        projection.declarations(&interface).0.join("\n")
    };
    let branded = declared(None);
    assert!(!branded.contains("readonly _0"), "{branded}");
    let mut plain = types::NativeOverrideEnv::default_js().clone();
    plain.disable_native_overrides = true;
    let constructed = declared(Some(&plain));
    assert!(constructed != branded, "{constructed}");
    assert!(constructed.contains("Box"), "{constructed}");
}
