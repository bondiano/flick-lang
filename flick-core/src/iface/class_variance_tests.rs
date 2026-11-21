use std::slice;

use super::variance_tests::check_module_with;
use super::*;
use crate::api_diff::{Bump, bump, diff};
use crate::diagnostics::codes;
use crate::types;
use serde_json::json;

const CLASS_PROVIDER: &str = "\
module App.Classes exposing (Box(..), Default(..), Describe(..), Convert(..), Combine(..), Marker(..), MyFunctor(..), SuperDesc(..))

type Box a = Box { value : a }

class Default a =
  default : a

class Describe a =
  describe : a -> String

class Describe a => SuperDesc a =
  super_desc : a -> String

class Convert a b =
  convert : a -> b

class Combine a =
  combine : a -> a -> a

class Marker a =
  marker : String

class MyFunctor (f : Type -> Type) =
  my_map : (a -> b) -> f a -> f b

instance Default (Box (Unit -> Unit)) =
  let default = Box { value = fun () -> () }

instance Describe (Unit ->{Async} Unit) =
  let describe _ = \"async\"
";

#[test]
fn exported_class_variance_round_trips_and_applies() {
    let (interface, diagnostics) = check_module_with("App.Classes", CLASS_PROVIDER, &|_| None);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let json = write(&interface);
    let encoded: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let class_variance = |name: &str| {
        encoded["exports"]
            .as_array()
            .expect("exports")
            .iter()
            .find(|e| e["namespace"] == "type" && e["name"] == name)
            .expect(name)["definition"]
            .get("variance")
            .cloned()
    };
    assert_eq!(class_variance("Default"), Some(json!(["covariant"])));
    assert_eq!(class_variance("Describe"), Some(json!(["contravariant"])));
    assert_eq!(class_variance("SuperDesc"), Some(json!(["contravariant"])));
    assert_eq!(
        class_variance("Convert"),
        Some(json!(["contravariant", "covariant"]))
    );
    assert_eq!(class_variance("Combine"), Some(json!(["invariant"])));
    assert_eq!(class_variance("Marker"), Some(json!(["bivariant"])));
    assert_eq!(class_variance("MyFunctor"), Some(json!(["invariant"])));

    let prelude = types::builtins()
        .iter()
        .find(|i| i.module == "Prelude")
        .expect("Prelude");
    for (name, expected) in [
        ("Eq", vec![Variance::Contravariant]),
        ("Ord", vec![Variance::Contravariant]),
        ("Show", vec![Variance::Contravariant]),
        ("Into", vec![Variance::Contravariant, Variance::Covariant]),
        ("Semigroup", vec![Variance::Invariant]),
        ("Functor", vec![Variance::Invariant]),
    ] {
        let def = prelude
            .find(Namespace::Type, name)
            .and_then(|e| e.definition.as_ref())
            .expect(name);
        assert_eq!(def.variance, expected, "Prelude.{name}");
    }

    let restored = read(json.as_bytes(), "App.Classes").expect("valid iface");
    assert_eq!(restored, interface);

    let consumer = "\
module App.Consumer exposing (..)
import App.Classes exposing (..)

box_async : Box (Unit ->{Async} Unit)
let box_async = default

desc_pure : String
let desc_pure = describe (fun () -> ())
";
    let (_, diagnostics) = check_module_with("App.Consumer", consumer, &|m| {
        (m == "App.Classes").then(|| restored.clone())
    });
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn class_variance_fallback_stays_invariant_and_tracks_api_diff() {
    let (interface, diagnostics) = check_module_with("App.Classes", CLASS_PROVIDER, &|_| None);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let mut unknown = interface.clone();
    for name in ["Default", "Describe"] {
        unknown
            .exports
            .iter_mut()
            .find(|e| e.entity.namespace == Namespace::Type && e.entity.name == name)
            .and_then(|e| e.definition.as_mut())
            .expect(name)
            .variance
            .clear();
    }
    let unknown = read(write(&unknown).as_bytes(), "App.Classes").expect("unknown is valid");
    let consumer = "\
module App.Consumer exposing (..)
import App.Classes exposing (..)

box_async : Box (Unit ->{Async} Unit)
let box_async = default

desc_pure : String
let desc_pure = describe (fun () -> ())
";
    let (_, diagnostics) = check_module_with("App.Consumer", consumer, &|m| {
        (m == "App.Classes").then(|| unknown.clone())
    });
    assert!(
        diagnostics.iter().filter(|d| d.code == codes::C001).count() >= 2,
        "missing imported class variance must stay invariant: {diagnostics:?}"
    );

    let invariant_provider = CLASS_PROVIDER
        .replace(
            "class Describe a =\n  describe : a -> String",
            "class Describe a =\n  describe : a -> a",
        )
        .replace(
            "instance Describe (Unit ->{Async} Unit) =\n  let describe _ = \"async\"",
            "instance Describe (Unit ->{Async} Unit) =\n  let describe f = f",
        );
    let (updated, d2) = check_module_with("App.Classes", &invariant_provider, &|_| None);
    assert!(d2.is_empty(), "{d2:?}");
    assert_ne!(fingerprint(&interface), fingerprint(&updated));
    let changes = diff(slice::from_ref(&interface), slice::from_ref(&updated));
    assert_eq!(
        bump(&changes),
        Bump::Major,
        "changing class variance must be a major API change: {changes:?}"
    );
}
