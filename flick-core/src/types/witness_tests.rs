//! Manifest members stay real, kind-correct aliases under a module witness.
use crate::diagnostics::Severity;
use crate::iface::{Interface, Namespace, support, write};
use crate::ir::tests::check_text;
use crate::source::SourceMap;
use crate::types::ForeignBackendSpec;
use crate::types::{Form, Kind, ModuleRequirement, Type};
use std::slice;

#[test]
fn existential_manifest_application_keeps_a_named_head_without_free_alias_formals() {
    let source = "module Probe exposing (S, Packed)\ntype module S =\n  type Box a = List a\ntype alias Packed = exists K : S. K.Box Int\n";
    let (_, interface) = check_text("Probe", source).expect("accepted existential source");
    let packed = interface
        .find(Namespace::Type, "Packed")
        .and_then(|e| e.definition.as_ref())
        .expect("packed alias");
    let Form::Alias {
        body: Type::Exists { body, .. },
        ..
    } = &packed.form
    else {
        panic!("existential alias");
    };
    assert_eq!(
        **body,
        Type::app(Type::con("$exists:K", "Box"), Type::con("Prelude", "Int"))
    );
    let mut free_formal = false;
    body.visit(&mut |ty| free_formal |= matches!(ty, Type::Var(name) if name == "a"));
    assert!(
        !free_formal,
        "Box's declaration formal must not escape: {body:?}"
    );
}

#[test]
fn unpacked_manifest_value_reduces_box_int_to_list_int() {
    let source = "module Probe exposing (S, inspect)\ntype module S =\n  type Box a = List a\n  take : Box Int -> Int\ninspect : Module S -> Int\nlet inspect packed = unpack packed as K in K.take [1]\n";
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", source);
    let errors: Vec<_> = crate::check(&sources, file)
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "List Int must satisfy the manifest Box Int: {errors:?}"
    );
}

#[test]
fn partial_manifest_constructor_keeps_its_kind_and_works_in_later_value_application() {
    let source = "module Probe exposing (S, Wrapper(..), Packed, inspect)\ntype module S =\n  type Box a = (a, a)\n  take : Box Int -> Int\ntype Wrapper (f : Type -> Type) = Wrap (f Int)\ntype alias Packed = exists K : S. Wrapper K.Box\nunwrap : Wrapper f -> f Int\nlet unwrap wrapped = match wrapped with\n  | Wrap value -> value\ninspect : Packed -> Int\nlet inspect packed = unpack packed as K, wrapped in K.take (unwrap wrapped)\n";
    let (_, interface) =
        check_text("Probe", source).expect("partial witness alias and later application");
    let packed = interface
        .find(Namespace::Type, "Packed")
        .and_then(|e| e.definition.as_ref())
        .expect("packed alias");
    let Form::Alias {
        body: Type::Exists { body, .. },
        ..
    } = &packed.form
    else {
        panic!("existential alias");
    };
    assert_eq!(
        **body,
        Type::app(Type::con("Probe", "Wrapper"), Type::con("$exists:K", "Box"))
    );
    let contract = interface
        .find(Namespace::Type, "S")
        .and_then(|e| e.definition.as_ref())
        .expect("contract");
    let Form::ModuleContract { requirements } = &contract.form else {
        panic!("module contract");
    };
    let ModuleRequirement::Type { params, .. } = &requirements[0] else {
        panic!("manifest member");
    };
    assert_eq!(
        Kind::arrows(params.iter().map(|p| p.kind.clone()), Kind::Type),
        Kind::arrow(Kind::Type, Kind::Type)
    );
}

#[test]
fn closed_existential_comparison_cannot_infer_an_outside_meta_as_its_hidden_type() {
    use crate::diagnostics::codes;
    let source = "module Probe exposing (S, Packed, discard)\ntype module S =\n  type Token\ntype alias Packed = exists K : S. K.Token\nget : (exists J : S. a) -> a\nforeign pure get from \"fixture\" \"get\"\ndiscard : Packed -> Unit\nlet discard packed =\n  let _ignored = get packed\n  ()\n";
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", source);
    let diagnostics = crate::check(&sources, file);
    assert!(
        diagnostics.iter().any(|d| d.code == codes::T003),
        "a closed hidden Token cannot instantiate an outside type parameter: {diagnostics:?}"
    );
}

#[test]
fn unpack_cannot_capture_an_outer_inferred_callback_type_even_when_result_is_unit() {
    use crate::diagnostics::codes;
    let source = "module Probe exposing (S, leak)\ntype module S =\n  type Token\n  value : Token\nmake : Unit ->{} (a -> Unit)\nforeign make from \"fixture\" \"make\"\nleak : Module S ->{} Unit\nlet leak packed =\n  let consume = make ()\n  unpack packed as K in\n    consume K.value\n";
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", source);
    let diagnostics = crate::check(&sources, file);
    assert!(
        diagnostics.iter().any(|d| d.code == codes::T003),
        "the outside callback cannot capture K.Token: {diagnostics:?}"
    );
}

#[test]
fn witness_meta_scopes_allow_closed_manifest_solutions_and_inner_inference() {
    for source in [
        "module Probe exposing (S, Packed, read)\ntype module S =\n  type Box a = List a\ntype alias Packed = exists K : S. K.Box Int\nget : (exists J : S. a) -> a\nforeign pure get from \"fixture\" \"get\"\nread : Packed -> List Int\nlet read packed = get packed\n",
        "module Probe exposing (S, okay)\ntype module S =\n  type Token\n  value : Token\nokay : Module S -> Unit\nlet okay packed = unpack packed as K in\n  let consume _value = ()\n  let _ignored = consume K.value\n  ()\n",
        "module Probe exposing (S, okay)\ntype module S =\n  type Token\nmake : Unit ->{} (a -> Unit)\nforeign make from \"fixture\" \"make\"\nokay : Module S ->{} Unit\nlet okay packed =\n  let consume = make ()\n  unpack packed as K in consume 1\n",
    ] {
        let mut sources = SourceMap::default();
        let file = sources.add("Probe.fl", source);
        let diagnostics: Vec<_> = crate::check(&sources, file)
            .into_iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(
            diagnostics.is_empty(),
            "legitimate scope-local/closed inference: {diagnostics:?}\n{source}"
        );
    }
}

fn checked_interface(name: &str, text: &str, dependencies: &[Interface]) -> Interface {
    use crate::{resolve, syntax, types};
    let mut sources = SourceMap::default();
    let file = sources.add(format!("{name}.fl"), text);
    let parsed = syntax::parse(text, file);
    let syntax = resolve::module_syntax(&parsed.tree, text, file);
    let module = resolve::Module {
        name,
        file,
        text,
        tree: &parsed.tree,
        syntax: &syntax,
        generated: resolve::NO_GENERATED,
    };
    let imported = |name: &str| dependencies.iter().find(|i| i.module == name).cloned();
    let (mut interface, mut diagnostics) = resolve::resolve(&module, &imported);
    diagnostics.extend(parsed.diagnostics);
    diagnostics.extend(types::check(
        &module,
        &imported,
        types::builtins(),
        types::Options {
            local: &|_| true,
            limits: types::Limits::default(),
            no_let_assert: false,
            thresholds: types::Thresholds::default(),
            watched: types::Watched::none(),
            stages_by_class: false,
            foreign_backend: ForeignBackendSpec::default_js(),
            validate_foreign_metadata: true,
            native_overrides: None,
        },
        &mut interface,
    ));
    assert!(
        diagnostics.iter().all(|d| d.severity != Severity::Error),
        "{diagnostics:?}"
    );
    interface
}

#[test]
fn inferred_witness_kind_is_closed_before_json_and_reopens_in_a_fresh_consumer() {
    use crate::iface::{Decl, Entity, Export, Interface, read, write};
    use crate::types::{ModuleSignature, TypeDef};
    let modes = checked_interface(
        "Modes",
        "module Modes exposing (Mode(..), At)\nkind Mode = Fast | Slow\ntype At (mode : Mode)\n",
        &[],
    );
    let ModuleSignature::Structure { requirements } = ModuleSignature::from_interface(&modes)
    else {
        panic!("real inferred structure");
    };
    let provider = Interface {
        module: "Provider".into(),
        exports: vec![Export {
            entity: Entity {
                module: "Provider".into(),
                namespace: Namespace::Type,
                name: "S".into(),
            },
            decl: Decl::ModuleType,
            parent: None,
            scheme: None,
            definition: Some(TypeDef {
                kind: Kind::Module,
                form: Form::ModuleContract { requirements },
                variance: Vec::new(),
            }),
            blocked: None,
            at: None,
            via: Vec::new(),
        }],
        ..Interface::default()
    };
    let provider = read(
        write(&provider).expect("publishable interface").as_bytes(),
        "Provider",
    )
    .expect("real signature persistence");
    let consumer = checked_interface(
        "Consumer",
        "module Consumer exposing (Packed)\nimport Provider\ntype alias Packed = exists K : Provider.S. forall a. K.At a\n",
        slice::from_ref(&provider),
    );
    let definition = consumer
        .find(Namespace::Type, "Packed")
        .and_then(|e| e.definition.as_ref())
        .expect("packed");
    let Form::Alias {
        body: Type::Exists { body, .. },
        ..
    } = &definition.form
    else {
        panic!("exists alias");
    };
    let Type::Forall { vars, .. } = body.as_ref() else {
        panic!("forall");
    };
    assert_eq!(
        vars[0].kind,
        Kind::Named {
            module: "$exists:K".into(),
            name: "Mode".into()
        }
    );
    let dependencies = support::dependencies(&consumer);
    assert!(
        dependencies
            .iter()
            .all(|entity| entity.module != "$exists:K"),
        "bound kinds are not external globals"
    );
    let encoded = write(&consumer).expect("publishable interface");
    assert!(
        !encoded.contains("$witness"),
        "no checker-fresh kind identity in published JSON"
    );
    let consumer = read(encoded.as_bytes(), "Consumer").expect("closed kind persistence");
    checked_interface(
        "User",
        "module User exposing (Other, coerce)\nimport Consumer\nimport Provider\ntype alias Other = exists J : Provider.S. forall a. J.At a\ncoerce : Consumer.Packed -> Other\nlet coerce value = value\n",
        &[consumer, provider],
    );
}

#[test]
fn an_existential_annotation_hole_can_be_inferred_when_its_value_is_discarded() {
    checked_interface(
        "Probe",
        "module Probe exposing (S, Packed, discard)\ntype module S =\n  type Token\ntype alias Packed = exists K : S. K.Token\ndiscard : Packed -> Unit\nlet discard value =\n  let _x : exists J : S. _ = value\n  ()\n",
        &[],
    );
}

#[test]
fn an_existential_annotation_hole_can_be_closed_and_reopened_for_roundtrip() {
    let interface = checked_interface(
        "Probe",
        "module Probe exposing (S, Packed, roundtrip)\ntype module S =\n  type Token\ntype alias Packed = exists K : S. K.Token\nroundtrip : Packed -> Packed\nlet roundtrip value =\n  let x : exists J : S. _ = value\n  x\n",
        &[],
    );
    assert!(
        !write(&interface)
            .expect("publishable interface")
            .contains("$witness")
    );
}

#[test]
fn nested_member_dictionary_and_values_use_the_same_materialized_view() {
    let source = "module Probe exposing (ChildSig, S, Packed, same)\nclass Equal a =\n  equal : a -> a -> Bool\ntype module ChildSig =\n  type Key\ntype module S =\n  module Child : ChildSig\n  value : Child.Key\n  other : Child.Key\n  equality : Equal.Dict Child.Key\n  instance Equal Child.Key from equality\ntype alias Packed = exists J : S. Unit\nsame : Packed -> Bool\nlet same packed = unpack packed as K, _payload in equal K.value K.other\n";
    checked_interface("Probe", source, &[]);
}

#[test]
fn a_named_annotation_variable_is_outside_an_existential_even_when_first_used_inside_it() {
    use crate::diagnostics::codes;
    let source = "module Probe exposing (S, Packed, discard)\ntype module S =\n  type Token\ntype alias Packed = exists K : S. K.Token\ndiscard : Packed -> Unit\nlet discard value =\n  let x : exists J : S. a = value\n  ()\n";
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", source);
    let diagnostics = crate::check(&sources, file);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == codes::T003),
        "annotation-wide a cannot learn a hidden Token: {diagnostics:?}"
    );
}

#[test]
fn a_named_annotation_variable_can_infer_a_closed_manifest_solution() {
    checked_interface(
        "Probe",
        "module Probe exposing (S, Packed, discard)\ntype module S =\n  type Box a = List a\ntype alias Packed = exists K : S. K.Box Int\ndiscard : Packed -> Unit\nlet discard value =\n  let x : exists J : S. a = value\n  ()\n",
        &[],
    );
}

#[test]
fn unpack_cannot_capture_a_hidden_type_in_an_outer_record_row() {
    use crate::diagnostics::codes;
    let source = "module Probe exposing (S, leak)\ntype module S =\n  type Token\n  value : Token\nmake : Unit ->{} ({ known : Unit | r } -> Unit)\nforeign make from \"fixture\" \"make\"\nleak : Module S ->{} Unit\nlet leak packed =\n  let consume = make ()\n  unpack packed as K in consume { known = (); field = K.value }\n";
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", source);
    let diagnostics = crate::check(&sources, file);
    assert!(
        diagnostics.iter().any(|d| d.code == codes::T003),
        "a row solution must not capture a hidden Token: {diagnostics:?}"
    );
}

#[test]
fn unpack_keeps_local_record_row_inference_polymorphic() {
    let source = "module Probe exposing (S, okay)\ntype module S =\n  type Token\n  value : Token\nokay : Module S -> Unit\nlet okay packed = unpack packed as K in\n  let get : forall a r. { field : a | r } -> a = fun x -> x.field\n  let _token = get { field = K.value; extra = 1 }\n  let _number = get { field = 1 }\n  ()\n";
    checked_interface("Probe", source, &[]);
}

#[test]
fn unpack_keeps_error_projection_inference() {
    checked_interface(
        "Probe",
        "module Probe exposing (S, okay)\ntype module S =\n  type Token\ntype error Oops = Oops\ndiscard : Errors e -> Unit\nlet discard _error = ()\nokay : Module S -> Unit\nlet okay packed = unpack packed as K in discard Oops\n",
        &[],
    );
}

#[test]
fn unpack_cannot_capture_a_hidden_type_through_deferred_error_row_growth() {
    use crate::diagnostics::codes;
    let source = "module Probe exposing (S, leak)\ntype module S =\n  type Token\n  value : Token\ntype error Wrapped a = Wrapped a\nmake : Unit ->{} ((Unit ->{; e} Unit) ->{} Unit)\nforeign make from \"fixture\" \"make\"\nleak : Module S ->{} Unit\nlet leak packed =\n  let consume = make ()\n  unpack packed as K in consume (fun () -> from_result (Err (Wrapped K.value)))\n";
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", source);
    let diagnostics = crate::check(&sources, file);
    assert!(
        diagnostics.iter().any(|d| d.code == codes::E002),
        "deferred row solving must retain the outside root's witness permissions: {diagnostics:?}"
    );
}

#[test]
fn unpack_keeps_capture_inference_for_local_closures() {
    checked_interface(
        "Probe",
        "module Probe exposing (S, okay)\ntype module S =\n  type Token\ntype File (s : Region)\nopen : Unit ->{Scope s} File s\nforeign effect open from \"fixture\" \"open\"\ndiscard : a -> Unit\nlet discard _value = ()\nokay : Module S ->{} Unit\nlet okay packed = unpack packed as K in scope s {\n  let file = open ()\n  discard (fun () -> file)\n}\n",
        &[],
    );
}

#[test]
fn an_existential_annotation_hole_reopens_with_its_payload_witness() {
    for body in [
        "unpack x as K, item in K.take item",
        "unpack packed as Outer in unpack x as K, item in K.take item",
    ] {
        let source = format!(
            "module Probe exposing (S, Packed, inspect)\ntype module S =\n  type Token\n  take : Token -> Int\ntype alias Packed = exists K : S. K.Token\ninspect : Module S -> Packed -> Int\nlet inspect packed value =\n  let x : exists J : S. _ = value\n  {body}\n"
        );
        checked_interface("Probe", &source, &[]);
    }
}
