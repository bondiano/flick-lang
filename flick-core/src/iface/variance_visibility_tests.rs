use super::variance_tests::check_module_with;
use super::*;
use crate::diagnostics::codes;

const SECRET_SRC: &str = "\
private module App.Secret exposing (Inner, Key(..))
type Inner a = Inner a
type Key = Key Int
";

const PROVIDER_SRC: &str = "\
module App.Provider exposing (Wrapper(..), Tag(..), BoxErr(..), SinkErr(..), CellErr(..))
import App.Secret exposing (Inner, Key)
type Wrapper a = Wrapper (Inner a)
type Tag a =
  | TagKey : Tag Key
  | TagVal a : Tag a
type error BoxErr a = BoxErr (Inner a)
type error SinkErr a = SinkErr (a -> Unit)
type error CellErr a = CellErr (a -> a)
";

const OK_CONSUMER: &str = "\
module App.Consumer exposing (Outer, ContraOuter, widen_wrap, widen_tag, widen_err, narrow_err, widen_outer, narrow_contra)
import App.Provider exposing (Wrapper(..), Tag(..), BoxErr(..), SinkErr(..))
type Outer a = Outer (Wrapper a)
type ContraOuter a = ContraOuter (Wrapper a -> Unit)
widen_wrap : Wrapper (Float -> Float) -> Wrapper (Float ->{Async} Float)
let widen_wrap w = w
widen_tag : Tag (Float -> Float) -> Tag (Float ->{Async} Float)
let widen_tag t = t
widen_err : BoxErr (Float -> Float) -> BoxErr (Float ->{Async} Float)
let widen_err e = e
narrow_err : SinkErr (Float ->{Async} Float) -> SinkErr (Float -> Float)
let narrow_err e = e
widen_outer : Outer (Float -> Float) -> Outer (Float ->{Async} Float)
let widen_outer x = x
narrow_contra : ContraOuter (Float ->{Async} Float) -> ContraOuter (Float -> Float)
let narrow_contra x = x
";

#[test]
fn imported_visible_constructor_data_and_error_preserve_exported_variance_without_private_deps() {
    let (secret_iface, d0) = check_module_with("App.Secret", SECRET_SRC, &|_| None);
    assert!(d0.is_empty(), "{d0:?}");

    let (provider_iface, d1) = check_module_with("App.Provider", PROVIDER_SRC, &|m| {
        (m == "App.Secret").then(|| secret_iface.clone())
    });
    assert!(d1.is_empty(), "{d1:?}");
    let restored = read(
        write(&provider_iface)
            .expect("publishable interface")
            .as_bytes(),
        "App.Provider",
    )
    .expect("valid iface");
    for (name, expected) in [
        ("Wrapper", vec![Variance::Covariant]),
        ("Tag", vec![Variance::Covariant]),
        ("BoxErr", vec![Variance::Covariant]),
        ("SinkErr", vec![Variance::Contravariant]),
        ("CellErr", vec![Variance::Invariant]),
    ] {
        let def = restored
            .find(Namespace::Type, name)
            .and_then(|e| e.definition.as_ref())
            .expect(name);
        assert_eq!(def.variance, expected, "{name}");
    }

    // Consumer imports ONLY App.Provider (App.Secret is private / unavailable).
    let (consumer_iface, d2) = check_module_with("App.Consumer", OK_CONSUMER, &|m| {
        (m == "App.Provider").then(|| restored.clone())
    });
    assert!(d2.is_empty(), "{d2:?}");
    assert_eq!(
        consumer_iface
            .find(Namespace::Type, "Outer")
            .and_then(|e| e.definition.as_ref())
            .expect("Outer")
            .variance,
        vec![Variance::Covariant]
    );
    assert_eq!(
        consumer_iface
            .find(Namespace::Type, "ContraOuter")
            .and_then(|e| e.definition.as_ref())
            .expect("ContraOuter")
            .variance,
        vec![Variance::Contravariant]
    );

    for (label, sig) in [
        (
            "narrow Wrapper",
            "f : Wrapper (Float ->{Async} Float) -> Wrapper (Float -> Float)\nlet f x = x",
        ),
        (
            "narrow Tag",
            "f : Tag (Float ->{Async} Float) -> Tag (Float -> Float)\nlet f x = x",
        ),
        (
            "narrow BoxErr",
            "f : BoxErr (Float ->{Async} Float) -> BoxErr (Float -> Float)\nlet f x = x",
        ),
        (
            "widen SinkErr",
            "f : SinkErr (Float -> Float) -> SinkErr (Float ->{Async} Float)\nlet f x = x",
        ),
        (
            "widen CellErr",
            "f : CellErr (Float -> Float) -> CellErr (Float ->{Async} Float)\nlet f x = x",
        ),
    ] {
        let bad = format!(
            "module App.Bad exposing (f)\nimport App.Provider exposing (Wrapper(..), Tag(..), BoxErr(..), SinkErr(..), CellErr(..))\n{sig}\n"
        );
        let (_, diags) = check_module_with("App.Bad", &bad, &|m| {
            (m == "App.Provider").then(|| restored.clone())
        });
        assert!(
            diags.iter().any(|d| d.code == codes::T003),
            "{label} must fail with T003: {diags:?}"
        );
    }
}
