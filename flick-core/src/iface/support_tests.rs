use crate::types::ForeignBackendSpec;
use std::io;
use std::path::Path;
use std::slice;

use super::{Interface, Namespace, read, write};
use crate::api_diff::{self, Bump, Change};
use crate::diagnostics::{Diagnostic, Severity};
use crate::project::{self, Host};
use crate::source::SourceMap;
use crate::types::{Form, Kind, ModuleRequirement, Type};

struct Source<'a>(&'a str);

impl Host for Source<'_> {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        if path == Path::new("/p/Probe.fl") {
            Ok(self.0.as_bytes().to_vec())
        } else {
            Err(io::ErrorKind::NotFound.into())
        }
    }

    fn display(&self, path: &Path) -> String {
        path.display().to_string()
    }
}

fn provider(text: &str) -> Interface {
    let mut sources = SourceMap::default();
    let (diagnostics, compiled) =
        project::compile(&Source(text), &mut sources, None, &["/p/Probe.fl".into()]);
    assert!(
        diagnostics.iter().all(|d| d.severity != Severity::Error),
        "{diagnostics:?}"
    );
    compiled
        .modules
        .into_iter()
        .find(|m| m.name == "Probe")
        .expect("root module")
        .interface
}

#[test]
fn exported_existential_alias_retains_its_private_contract_without_exporting_it() {
    let interface = provider(
        "module Probe exposing (PackedA, PackedB)\ntype module S =\n  type Box a = List a\ntype alias PackedA = exists K : S. K.Box Int\ntype alias PackedB = exists J : S. J.Box Int\n",
    );
    assert!(interface.find(Namespace::Type, "S").is_none());
    let Form::ModuleContract { requirements } = &interface.support_types[0].definition.form else {
        panic!("private contract");
    };
    let ModuleRequirement::Type {
        params,
        manifest: Some(Type::App { head, arg }),
        ..
    } = &requirements[0]
    else {
        panic!("parameterized manifest");
    };
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].name, "a");
    assert_eq!(params[0].kind, Kind::Type);
    assert!(matches!(head.as_ref(), Type::Con { name, .. } if name == "List"));
    assert_eq!(**arg, Type::Var("a".into()));
    let serialized = serde_json::to_value(&interface).expect("interface JSON");
    let support = serialized
        .get("support_types")
        .and_then(serde_json::Value::as_array)
        .expect("private contract support is persisted");
    assert_eq!(support.len(), 1);
    assert_eq!(support[0]["entity"]["module"], "Probe");
    assert_eq!(support[0]["entity"]["name"], "S");
    assert_eq!(support[0]["owner_path"], serde_json::json!([]));
    assert_eq!(
        read(
            write(&interface).expect("publishable interface").as_bytes(),
            "Probe"
        ),
        Ok(interface)
    );
}

#[test]
fn exported_alias_retains_required_private_descendant_contract_without_a_public_child() {
    let interface = provider(
        "module Probe exposing (PackedA, PackedB)\nmodule Hidden exposing (PackedA, PackedB) =\n  type module S =\n    type Box a = List a\n  type alias PackedA = exists K : S. K.Box Int\n  type alias PackedB = exists J : S. J.Box Int\ntype alias PackedA = Hidden.PackedA\ntype alias PackedB = Hidden.PackedB\n",
    );
    assert!(interface.modules.is_empty());
    let serialized = serde_json::to_value(&interface).expect("interface JSON");
    let support = serialized
        .get("support_types")
        .and_then(serde_json::Value::as_array)
        .expect("discarded child support survives in parent");
    assert_eq!(support.len(), 1);
    assert_eq!(support[0]["entity"]["module"], "Probe.Hidden");
    assert_eq!(support[0]["entity"]["name"], "S");
    assert_eq!(support[0]["owner_path"], serde_json::json!(["Hidden"]));
    assert_eq!(
        read(
            write(&interface).expect("publishable interface").as_bytes(),
            "Probe"
        ),
        Ok(interface)
    );
}

#[test]
fn support_validation_rejects_forged_owners_namespaces_and_duplicates() {
    let interface = provider(
        "module Probe exposing (Packed)\ntype module S =\n  type Box a = List a\ntype alias Packed = exists K : S. K.Box Int\n",
    );
    for member in ["", "Hidden.More", "../Hidden", "Hidden name", "@Hidden"] {
        let mut bad = interface.clone();
        bad.support_types[0].owner_path = vec![member.into()];
        assert!(
            read(
                write(&bad).expect("publishable interface").as_bytes(),
                "Probe"
            )
            .is_err(),
            "{member}"
        );
    }
    let mut bad = interface.clone();
    bad.support_types[0].entity.namespace = Namespace::Value;
    assert!(
        read(
            write(&bad).expect("publishable interface").as_bytes(),
            "Probe"
        )
        .is_err()
    );
    let mut bad = interface.clone();
    bad.support_types.push(bad.support_types[0].clone());
    assert!(
        read(
            write(&bad).expect("publishable interface").as_bytes(),
            "Probe"
        )
        .is_err()
    );
    let mut json: serde_json::Value =
        serde_json::from_str(&write(&interface).expect("publishable interface"))
            .expect("test fixture shape");
    json["support_types"][0]
        .as_object_mut()
        .expect("test fixture shape")
        .remove("owner_path");
    assert!(read(json.to_string().as_bytes(), "Probe").is_err());
}

#[test]
fn support_semantics_change_fingerprints_and_api_diff_and_schema_25_is_rejected() {
    let before = provider(
        "module Probe exposing (Packed)\ntype module S =\n  type Box a = List a\ntype alias Packed = exists K : S. K.Box Int\n",
    );
    let mut after = before.clone();
    after.support_types[0].definition.kind = Kind::Type;
    assert_ne!(
        super::fingerprint(&before).expect("publishable interface"),
        super::fingerprint(&after).expect("publishable interface")
    );
    let changes = api_diff::diff(slice::from_ref(&before), &[after]);
    assert_eq!(api_diff::bump(&changes), Bump::Major);
    assert!(
        changes[0]
            .changes
            .iter()
            .any(|c| matches!(c, Change::Changed { name, .. } if name == "type support"))
    );
    let mut json: serde_json::Value =
        serde_json::from_str(&write(&before).expect("publishable interface"))
            .expect("test fixture shape");
    json["iface-version"] = 25.into();
    assert!(
        read(json.to_string().as_bytes(), "Probe")
            .expect_err("old schema must be rejected")
            .contains("not supported")
    );
}

fn consumer(interface: &Interface, text: &str) -> Vec<Diagnostic> {
    use crate::{resolve, syntax, types};
    let mut sources = SourceMap::default();
    let file = sources.add("Consumer.fl", text);
    let parsed = syntax::parse(text, file);
    let syntax = resolve::module_syntax(&parsed.tree, text, file);
    let module = resolve::Module {
        name: "Consumer",
        file,
        text,
        tree: &parsed.tree,
        syntax: &syntax,
        generated: resolve::NO_GENERATED,
    };
    // No provider source, private child interface or producer checker survives.
    let imported = |name: &str| (name == "Probe").then(|| interface.clone());
    let (mut checked, mut diagnostics) = resolve::resolve(&module, &imported);
    diagnostics.extend(types::check(
        &module,
        &imported,
        types::builtins(),
        types::Options {
            local: &|name| name == "Consumer",
            limits: types::Limits::default(),
            no_let_assert: false,
            thresholds: types::Thresholds::default(),
            watched: types::Watched::none(),
            stages_by_class: false,
            foreign_backend: ForeignBackendSpec::default_js(),
            validate_foreign_metadata: true,
            native_overrides: None,
        },
        &mut checked,
    ));
    diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect()
}

#[test]
fn fresh_consumer_learns_private_descendant_semantics_but_not_its_source_name() {
    let published = provider(
        "module Probe exposing (Packed)\nmodule Hidden exposing (Packed) =\n  type module S =\n    type Box a = List a\n  type alias Packed = exists K : S. K.Box Int\ntype alias Packed = Hidden.Packed\n",
    );
    let restored = read(
        write(&published).expect("publishable interface").as_bytes(),
        "Probe",
    )
    .expect("fresh interface");
    let diagnostics = consumer(
        &restored,
        "module Consumer exposing (Again)\nimport Probe\ntype alias Again = Probe.Packed\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let diagnostics = consumer(
        &restored,
        "module Consumer exposing (Leak)\nimport Probe\ntype alias Leak = Probe.Hidden.S\n",
    );
    assert!(
        !diagnostics.is_empty(),
        "private contract must not become a source export"
    );
}

#[test]
fn support_closure_is_required_only_and_hides_opaque_representations() {
    let interface = provider(
        "module Probe exposing (Packed)\nopaque type Secret = Secret Int\ntype Unused\ntype module S =\n  type Box a = List a\n  tag : Secret\ntype alias Packed = exists K : S. K.Box Int\n",
    );
    let names: Vec<_> = interface
        .support_types
        .iter()
        .map(|s| s.entity.name.as_str())
        .collect();
    assert_eq!(names, ["S", "Secret"]);
    assert_eq!(interface.support_types[1].definition.form, Form::NOMINAL);
    assert!(interface.find(Namespace::Constructor, "Secret").is_none());
    assert!(interface.find(Namespace::Type, "Secret").is_none());
    assert_eq!(
        read(
            write(&interface).expect("publishable interface").as_bytes(),
            "Probe"
        ),
        Ok(interface)
    );
}

#[test]
fn support_projection_closure_handles_cycles_and_drops_unused_records() {
    use super::{Decl, Entity, Export, SupportType};
    use crate::types::TypeDef;
    let record = |name: &str, next: &str| SupportType {
        entity: Entity {
            module: "Probe".into(),
            namespace: Namespace::Type,
            name: name.into(),
        },
        definition: TypeDef {
            kind: Kind::Type,
            form: Form::Alias {
                params: vec![],
                body: Type::con("Probe", next),
                opaque: false,
            },
            variance: Vec::new(),
        },
        owner_path: vec![],
    };
    let mut interface = Interface {
        module: "Probe".into(),
        exports: vec![Export {
            entity: Entity {
                module: "Probe".into(),
                namespace: Namespace::Value,
                name: "root".into(),
            },
            decl: Decl::Let,
            parent: None,
            scheme: Some(Type::con("Probe", "A")),
            definition: None,
            blocked: None,
            at: None,
            via: Vec::new(),
        }],
        support_types: vec![
            record("A", "B"),
            record("B", "A"),
            record("Unused", "Unused"),
        ],
        ..Interface::default()
    };
    super::support::prune(&mut interface);
    assert_eq!(interface.support_types.len(), 2);
    let once = interface.clone();
    super::support::prune(&mut interface);
    assert_eq!(interface, once);
    assert_eq!(
        read(
            write(&interface).expect("publishable interface").as_bytes(),
            "Probe"
        ),
        Ok(interface)
    );
}

#[test]
fn generated_atomic_owner_names_survive_but_prefix_lookalikes_do_not() {
    let mut interface = provider(
        "module Probe exposing (Packed)\ntype module S =\n  type Box a = List a\ntype alias Packed = exists K : S. K.Box Int\n",
    );
    interface.support_types[0].owner_path = vec!["Hidden$generated_1".into()];
    interface.support_types[0].entity.module = "Probe.Hidden$generated_1".into();
    assert_eq!(
        read(
            write(&interface).expect("publishable interface").as_bytes(),
            "Probe"
        ),
        Ok(interface.clone())
    );
    interface.support_types[0].entity.module = "ProbeExtra.Hidden$generated_1".into();
    assert!(
        read(
            write(&interface).expect("publishable interface").as_bytes(),
            "Probe"
        )
        .is_err()
    );
}

#[test]
fn parameterized_projection_keeps_only_required_type_support() {
    use super::{Entity, SupportType};
    use crate::types::{ModuleSignature, TypeDef};
    let record = |name: &str| SupportType {
        entity: Entity {
            module: "Probe".into(),
            namespace: Namespace::Type,
            name: name.into(),
        },
        definition: TypeDef {
            kind: Kind::Type,
            form: Form::NOMINAL,
            variance: Vec::new(),
        },
        owner_path: vec![],
    };
    let actual = Interface {
        module: "Probe".into(),
        support_types: vec![record("Required"), record("Unused")],
        ..Interface::default()
    };
    let signature = |requirements| ModuleSignature::Parameterized {
        group_size: 1,
        binder: "Argument".into(),
        parameter: Box::new(ModuleSignature::Structure {
            requirements: vec![],
        }),
        result: Box::new(ModuleSignature::Structure { requirements }),
    };
    let kept = signature(vec![ModuleRequirement::Value {
        name: "value".into(),
        ty: Type::con("Probe", "Required"),
    }])
    .project(&actual);
    assert_eq!(kept.support_types, vec![record("Required")]);
    let dropped = signature(vec![]).project(&actual);
    assert!(dropped.support_types.is_empty());
    assert!(kept.exports.is_empty());
    assert!(kept.modules.is_empty());
}

#[test]
fn private_service_support_matches_its_normalized_nested_snapshot() {
    let interface = provider(
        "module Probe exposing (Packed)\ntype alias Answer = Int\nservice Clock =\n  tick : Unit ->{} Answer\ntype module S =\n  service Tick = Clock\ntype alias Packed = exists K : S. Unit\n",
    );
    let restored = read(
        write(&interface).expect("publishable interface").as_bytes(),
        "Probe",
    )
    .expect("accepted provider round trips");
    assert_eq!(restored, interface);
    let names: Vec<_> = restored
        .support_types
        .iter()
        .map(|s| s.entity.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["Clock", "S"],
        "normalized operation no longer depends on Answer"
    );
    assert!(restored.find(Namespace::Type, "Clock").is_none());
    let diagnostics = consumer(
        &restored,
        "module Consumer exposing (Again)\nimport Probe\ntype alias Again = Probe.Packed\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn private_service_import_view_keeps_opaque_operation_results_nominal() {
    let interface = provider(
        "module Probe exposing (Packed)\nopaque type alias Answer = Int\nservice Clock =\n  tick : Unit ->{} Answer\ntype module S =\n  service Tick = Clock\ntype alias Packed = exists K : S. Unit\n",
    );
    let restored = read(
        write(&interface).expect("publishable interface").as_bytes(),
        "Probe",
    )
    .expect("opaque provider round trips");
    let answer = restored
        .support_types
        .iter()
        .find(|s| s.entity.name == "Answer")
        .expect("opaque dependency");
    assert_eq!(answer.definition.form, Form::NOMINAL);
    let clock = restored
        .support_types
        .iter()
        .find(|s| s.entity.name == "Clock")
        .expect("real service support");
    let Form::Service { operations, .. } = &clock.definition.form else {
        panic!("service definition");
    };
    let Type::Fun { result, .. } = &operations[0].ty else {
        panic!("operation arrow");
    };
    assert_eq!(**result, Type::con("Probe", "Answer"));
    assert!(restored.find(Namespace::Type, "Answer").is_none());
    let diagnostics = consumer(
        &restored,
        "module Consumer exposing (Again)\nimport Probe\ntype alias Again = Probe.Packed\n",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn private_contract_alias_coercions_survive_fresh_json_import_for_root_and_descendant() {
    for source in [
        "module Probe exposing (PackedA, PackedB)\ntype module S =\n  type Box a = List a\ntype alias PackedA = exists K : S. K.Box Int\ntype alias PackedB = exists J : S. J.Box Int\n",
        "module Probe exposing (PackedA, PackedB)\nmodule Hidden exposing (PackedA, PackedB) =\n  type module S =\n    type Box a = List a\n  type alias PackedA = exists K : S. K.Box Int\n  type alias PackedB = exists J : S. J.Box Int\ntype alias PackedA = Hidden.PackedA\ntype alias PackedB = Hidden.PackedB\n",
    ] {
        let published = provider(source);
        let restored = read(
            write(&published).expect("publishable interface").as_bytes(),
            "Probe",
        )
        .expect("fresh published interface");
        let diagnostics = consumer(
            &restored,
            "module Consumer exposing (coerce)\nimport Probe\ncoerce : Probe.PackedA -> Probe.PackedB\nlet coerce value = value\n",
        );
        assert!(
            diagnostics.is_empty(),
            "private contract coercion: {diagnostics:?}"
        );
        assert!(restored.find(Namespace::Type, "S").is_none());
        assert!(restored.modules.is_empty());
    }
}

#[test]
fn private_closed_existential_manifest_coercion_reenters_its_persisted_contract() {
    for source in [
        "module Probe exposing (PackedA, PackedB)\ntype module S =\n  type Box a = List a\ntype alias PackedA = exists K : S. K.Box Int\ntype alias PackedB = exists J : S. List Int\n",
        "module Probe exposing (PackedA, PackedB)\nmodule Hidden exposing (PackedA, PackedB) =\n  type module S =\n    type Box a = List a\n  type alias PackedA = exists K : S. K.Box Int\n  type alias PackedB = exists J : S. List Int\ntype alias PackedA = Hidden.PackedA\ntype alias PackedB = Hidden.PackedB\n",
    ] {
        let published = provider(source);
        let restored = read(
            write(&published).expect("publishable interface").as_bytes(),
            "Probe",
        )
        .expect("fresh private contract support");
        let diagnostics = consumer(
            &restored,
            "module Consumer exposing (coerce)\nimport Probe\ncoerce : Probe.PackedA -> Probe.PackedB\nlet coerce value = value\n",
        );
        assert!(
            diagnostics.is_empty(),
            "manifest aliases are definitionally List Int: {diagnostics:?}"
        );
        assert!(restored.modules.is_empty());
    }
}
