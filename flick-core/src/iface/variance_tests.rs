use crate::types::ForeignBackendSpec;
use std::slice;

use super::*;
use crate::api_diff::{Bump, bump, diff};
use crate::diagnostics::{Diagnostic, codes};
use crate::resolve::{self, Module};
use crate::source::SourceMap;
use crate::{syntax, types};
use serde_json::json;

pub(super) fn check_module_with(
    name: &str,
    text: &str,
    imported: &dyn Fn(&str) -> Option<Interface>,
) -> (Interface, Vec<Diagnostic>) {
    let mut sources = SourceMap::default();
    let file = sources.add("m.fl", text);
    let parse = syntax::parse(text, file);
    let syntax = resolve::module_syntax(&parse.tree, text, file);
    let module = Module {
        name,
        file,
        text,
        tree: &parse.tree,
        syntax: &syntax,
        generated: resolve::NO_GENERATED,
    };
    let (mut interface, mut diagnostics) = resolve::resolve(&module, imported);
    let local = |candidate: &str| candidate == name;
    let options = types::Options {
        local: &local,
        limits: types::Limits::default(),
        no_let_assert: false,
        thresholds: types::Thresholds::default(),
        watched: types::Watched::none(),
        stages_by_class: false,
        foreign_backend: ForeignBackendSpec::default_js(),
        validate_foreign_metadata: true,
        native_overrides: None,
    };
    diagnostics.extend(types::check(
        &module,
        imported,
        types::builtins(),
        options,
        &mut interface,
    ));
    diagnostics.retain(|d| !d.unnecessary());
    (interface, diagnostics)
}

#[test]
fn exported_data_variance_round_trips_and_applies_without_provider_source() {
    let provider = "\
module App.Provider exposing (Box, Sink, Cell, Phantom, Secret, Thunk, Alias, Abstract, private_box)
opaque type Box a = Box { value : a }
opaque type Sink a = Sink (a -> Unit)
opaque type Cell a = Cell (Ref a)
opaque type Phantom a = Phantom
type Secret a = Secret a
opaque type Thunk (r : ServiceRow) a = Thunk (Unit ->{r} a)
opaque type alias Alias a = List a
type Abstract a
type PrivateBox a = PrivateBox a
private_box : PrivateBox a -> PrivateBox a
let private_box x = x
";
    let (interface, diagnostics) = check_module_with("App.Provider", provider, &|_| None);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let json = write(&interface).expect("publishable interface");
    let encoded: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let export_variance = |name: &str| {
        encoded["exports"]
            .as_array()
            .expect("exports")
            .iter()
            .find(|e| e["namespace"] == "type" && e["name"] == name)
            .expect(name)["definition"]
            .get("variance")
            .cloned()
    };
    assert_eq!(export_variance("Box"), Some(json!(["covariant"])));
    assert_eq!(export_variance("Sink"), Some(json!(["contravariant"])));
    assert_eq!(export_variance("Cell"), Some(json!(["invariant"])));
    assert_eq!(export_variance("Phantom"), Some(json!(["bivariant"])));
    assert_eq!(export_variance("Secret"), Some(json!(["covariant"])));
    assert_eq!(
        export_variance("Thunk"),
        Some(json!(["invariant", "covariant"]))
    );
    assert_eq!(export_variance("Alias"), None);
    assert_eq!(export_variance("Abstract"), None);
    let support = encoded["support_types"]
        .as_array()
        .expect("support_types")
        .iter()
        .find(|s| s["entity"]["name"] == "PrivateBox")
        .expect("PrivateBox support");
    assert_eq!(support["definition"]["variance"], json!(["covariant"]));

    let list_def = types::builtins()
        .iter()
        .find(|i| i.module == "Prelude")
        .expect("Prelude")
        .find(Namespace::Type, "List")
        .expect("List")
        .definition
        .as_ref()
        .expect("List def");
    assert_eq!(list_def.variance, vec![Variance::Covariant]);

    let restored = read(json.as_bytes(), "App.Provider").expect("valid iface");
    assert_eq!(restored, interface);

    let consumer = "\
module App.Consumer exposing (..)
import App.Provider exposing (Box, Sink, Cell, Secret, Thunk, Alias, Abstract)

widen_box : Box (Float -> Float) -> Box (Float ->{Async} Float)
let widen_box b = b

narrow_sink : Sink (Float ->{Async} Float) -> Sink (Float -> Float)
let narrow_sink s = s

widen_secret : Secret (Float -> Float) -> Secret (Float ->{Async} Float)
let widen_secret s = s

widen_thunk : (Thunk none (Float -> Float))^{c} -> (Thunk none (Float ->{Async} Float))^{c}
let widen_thunk t = t
";
    let (_, diagnostics) = check_module_with("App.Consumer", consumer, &|m| {
        (m == "App.Provider").then(|| restored.clone())
    });
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    // Stripping serialized variance leaves opaque data conservatively invariant.
    let mut unknown = restored.clone();
    unknown
        .exports
        .iter_mut()
        .find(|e| e.entity.namespace == Namespace::Type && e.entity.name == "Box")
        .and_then(|e| e.definition.as_mut())
        .expect("Box def")
        .variance
        .clear();
    let unknown = read(
        write(&unknown).expect("publishable interface").as_bytes(),
        "App.Provider",
    )
    .expect("unknown is valid");
    let (_, diagnostics) = check_module_with(
        "App.Consumer",
        "module App.Consumer exposing (..)\nimport App.Provider exposing (Box)\nwiden_box : Box (Float -> Float) -> Box (Float ->{Async} Float)\nlet widen_box b = b\n",
        &|m| (m == "App.Provider").then(|| unknown.clone()),
    );
    assert!(
        diagnostics.iter().any(|d| d.code == codes::T003),
        "unknown opaque variance must stay invariant: {diagnostics:?}"
    );
}

#[test]
fn rejects_malformed_serialized_variance_and_tracks_fingerprint_and_api_diff() {
    let covariant_src = "\
module App.Var exposing (Box, Thunk, Id, Alias, Show, Map)
opaque type Box a = Box a
opaque type Thunk (r : ServiceRow) a = Thunk (Unit ->{r} a)
opaque type Id = Id Int
type alias Alias a = List a
class Show a =
  show_it : a -> String
class Map (f : Type -> Type) =
  map_it : (a -> b) -> f a -> f b
";
    let invariant_src = covariant_src.replace(
        "opaque type Box a = Box a",
        "opaque type Box a = Box (Ref a)",
    );
    let (covariant, d1) = check_module_with("App.Var", covariant_src, &|_| None);
    let (invariant, d2) = check_module_with("App.Var", &invariant_src, &|_| None);
    assert!(d1.is_empty() && d2.is_empty(), "{d1:?} {d2:?}");

    assert_ne!(
        fingerprint(&covariant).expect("publishable interface"),
        fingerprint(&invariant).expect("publishable interface")
    );
    let changes = diff(slice::from_ref(&covariant), slice::from_ref(&invariant));
    assert_eq!(
        bump(&changes),
        Bump::Major,
        "changing opaque variance must be a major API change: {changes:?}"
    );

    let set_variance = |base: &Interface, name: &str, variance: Vec<Variance>| {
        let mut forged = base.clone();
        forged
            .exports
            .iter_mut()
            .find(|e| e.entity.namespace == Namespace::Type && e.entity.name == name)
            .and_then(|e| e.definition.as_mut())
            .expect(name)
            .variance = variance;
        forged
    };
    let consumer = "module Consumer exposing (..)\nimport App.Var exposing (Box)\nx : Box Int -> Box Int\nlet x v = v\n";

    for (label, forged) in [
        (
            "wrong arity",
            set_variance(
                &covariant,
                "Box",
                vec![Variance::Covariant, Variance::Covariant],
            ),
        ),
        (
            "zero-param variance",
            set_variance(&covariant, "Id", vec![Variance::Covariant]),
        ),
        (
            "non-Type parameter covariant",
            set_variance(
                &covariant,
                "Thunk",
                vec![Variance::Covariant, Variance::Covariant],
            ),
        ),
        (
            "alias variance",
            set_variance(&covariant, "Alias", vec![Variance::Covariant]),
        ),
        (
            "class wrong arity",
            set_variance(
                &covariant,
                "Show",
                vec![Variance::Contravariant, Variance::Contravariant],
            ),
        ),
        (
            "higher-kinded class parameter covariant",
            set_variance(&covariant, "Map", vec![Variance::Covariant]),
        ),
    ] {
        assert!(
            read(
                write(&forged).expect("publishable interface").as_bytes(),
                "App.Var"
            )
            .is_err(),
            "iface::read must reject {label}"
        );
        assert!(
            support::validate(&forged).is_err(),
            "support::validate must reject {label}"
        );
        let (_, diagnostics) = check_module_with("Consumer", consumer, &|m| {
            (m == "App.Var").then(|| forged.clone())
        });
        assert!(
            !diagnostics.is_empty(),
            "checker must reject malformed imported variance for {label}"
        );
    }
}

#[test]
fn contravariant_wrappers_and_cycles_are_order_independent_and_deterministic() {
    let provider = "\
module App.Provider exposing (AWrapSink, Sink, SecretSink, WrapPhantom, DoubleSink, WrapContraPhantom, ContraPhantom, Phantom, CycleA, CycleB, Mix)
opaque type AWrapSink a = AWrapSink (Sink a)
opaque type Sink a = Sink (a -> Unit)
type SecretSink a = SecretSink (a -> Unit)
opaque type WrapPhantom a = WrapPhantom (Phantom a)
opaque type DoubleSink a = DoubleSink (Sink (Sink a))
opaque type WrapContraPhantom a = WrapContraPhantom (ContraPhantom a)
type ContraPhantom a = ContraPhantom (Phantom a -> Unit)
opaque type Phantom a = Phantom
type CycleA a = CycleA (CycleB a)
type CycleB a = CycleB (CycleA a)
type Mix a = Mix (CycleA a) (CycleB a -> Unit)
";
    let (first, d1) = check_module_with("App.Provider", provider, &|_| None);
    let (second, d2) = check_module_with("App.Provider", provider, &|_| None);
    assert!(d1.is_empty() && d2.is_empty(), "{d1:?} {d2:?}");
    assert_eq!(
        fingerprint(&first).expect("publishable interface"),
        fingerprint(&second).expect("publishable interface")
    );

    let encoded: serde_json::Value =
        serde_json::from_str(&write(&first).expect("publishable interface")).expect("valid JSON");
    let variance_of = |name: &str| {
        encoded["exports"]
            .as_array()
            .expect("exports")
            .iter()
            .find(|e| e["namespace"] == "type" && e["name"] == name)
            .expect(name)["definition"]
            .get("variance")
            .cloned()
    };
    for (name, expected) in [
        ("AWrapSink", json!(["contravariant"])),
        ("Sink", json!(["contravariant"])),
        ("SecretSink", json!(["contravariant"])),
        ("WrapPhantom", json!(["covariant"])),
        ("DoubleSink", json!(["invariant"])),
        ("WrapContraPhantom", json!(["contravariant"])),
        ("ContraPhantom", json!(["contravariant"])),
        ("Phantom", json!(["bivariant"])),
        ("CycleA", json!(["covariant"])),
        ("CycleB", json!(["covariant"])),
        ("Mix", json!(["invariant"])),
    ] {
        assert_eq!(variance_of(name), Some(expected), "{name}");
    }

    let restored = read(
        write(&first).expect("publishable interface").as_bytes(),
        "App.Provider",
    )
    .expect("valid iface");
    let consumer = "\
module App.Consumer exposing (WrapSink, WrapSecretSink, narrow_sink, narrow_secret)
import App.Provider exposing (Sink, SecretSink)

type WrapSink a = WrapSink (Sink a)
type WrapSecretSink a = WrapSecretSink (SecretSink a)

narrow_sink : WrapSink (Float ->{Async} Float) -> WrapSink (Float -> Float)
let narrow_sink w = w

narrow_secret : WrapSecretSink (Float ->{Async} Float) -> WrapSecretSink (Float -> Float)
let narrow_secret w = w
";
    let (consumer_iface, diagnostics) = check_module_with("App.Consumer", consumer, &|m| {
        (m == "App.Provider").then(|| restored.clone())
    });
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    for name in ["WrapSink", "WrapSecretSink"] {
        let def = consumer_iface
            .find(Namespace::Type, name)
            .and_then(|e| e.definition.as_ref())
            .expect(name);
        assert_eq!(def.variance, vec![Variance::Contravariant], "{name}");
    }

    let bad = "\
module App.Bad exposing (widen)
import App.Provider exposing (Sink)
type WrapSink a = WrapSink (Sink a)
widen : WrapSink (Float -> Float) -> WrapSink (Float ->{Async} Float)
let widen w = w
";
    let (_, diagnostics) = check_module_with("App.Bad", bad, &|m| {
        (m == "App.Provider").then(|| restored.clone())
    });
    assert!(
        diagnostics.iter().any(|d| d.code == codes::T003),
        "contravariant wrapper must reject widening: {diagnostics:?}"
    );
}

fn forged_contract(req: types::ModuleRequirement) -> Interface {
    Interface {
        module: "App.Mod".into(),
        exports: vec![Export {
            entity: Entity {
                module: "App.Mod".into(),
                namespace: Namespace::Type,
                name: "Sig".into(),
            },
            decl: Decl::ModuleType,
            parent: None,
            scheme: None,
            definition: Some(TypeDef {
                kind: types::Kind::Module,
                form: types::Form::ModuleContract {
                    requirements: vec![req],
                },
                variance: Vec::new(),
            }),
            blocked: None,
            at: None,
            via: Vec::new(),
        }],
        ..Interface::default()
    }
}

fn forged_parameterized(in_param: bool, req: types::ModuleRequirement) -> Interface {
    let bad = types::ModuleSignature::Structure {
        requirements: vec![req],
    };
    let empty = types::ModuleSignature::Structure {
        requirements: Vec::new(),
    };
    let (parameter, result) = if in_param { (bad, empty) } else { (empty, bad) };
    Interface {
        module: "App.Mod".into(),
        parameterized: Some(types::ModuleSignature::Parameterized {
            group_size: 1,
            binder: "X".into(),
            parameter: Box::new(parameter),
            result: Box::new(result),
        }),
        ..Interface::default()
    }
}

#[test]
fn rejects_forged_variance_in_nested_module_signatures_and_anonymous_nominals() {
    use crate::types::{Form, Kind, ModuleNominalKind, ModuleRequirement, ModuleSignature};

    let bad_class = TypeDef {
        kind: Kind::arrow(Kind::Type, Kind::Constraint),
        form: Form::Class {
            params: vec!["a".into()],
            supers: Vec::new(),
            fundeps: Vec::new(),
            methods: Vec::new(),
            defaults: Vec::new(),
            default_sigs: Vec::new(),
            derive: false,
        },
        variance: vec![Variance::Contravariant, Variance::Covariant],
    };
    let bad_kind = TypeDef {
        kind: Kind::Named {
            module: "App.Mod".into(),
            name: "Mode".into(),
        },
        form: Form::Kind,
        variance: vec![Variance::Covariant],
    };
    let anon_nominal = ModuleRequirement::Nominal {
        name: "C".into(),
        category: ModuleNominalKind::Class,
        identity: None,
        definition: Some(Box::new(bad_class)),
        methods: Vec::new(),
    };
    let kind_req = ModuleRequirement::Kind {
        name: "Mode".into(),
        identity: None,
        definition: Box::new(bad_kind),
    };
    let nested_mod = ModuleRequirement::Module {
        name: "Sub".into(),
        binder: String::new(),
        signature: Box::new(ModuleSignature::Structure {
            requirements: vec![anon_nominal.clone()],
        }),
    };

    for (label, forged) in [
        (
            "anonymous nominal in module contract",
            forged_contract(anon_nominal.clone()),
        ),
        ("kind in module contract", forged_contract(kind_req.clone())),
        ("nested module in contract", forged_contract(nested_mod)),
        (
            "anonymous nominal in parameterized parameter",
            forged_parameterized(true, anon_nominal.clone()),
        ),
        (
            "anonymous nominal in parameterized result",
            forged_parameterized(false, anon_nominal),
        ),
        (
            "kind in parameterized result",
            forged_parameterized(false, kind_req),
        ),
    ] {
        assert!(
            read(
                write(&forged).expect("publishable interface").as_bytes(),
                "App.Mod"
            )
            .is_err(),
            "iface::read must reject {label}"
        );
        assert!(
            support::validate(&forged).is_err(),
            "support::validate must reject {label}"
        );
    }
}
