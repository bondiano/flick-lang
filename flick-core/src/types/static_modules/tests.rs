use crate::diagnostics::{Severity, codes};
use crate::iface::Namespace;
use crate::source::SourceMap;
use crate::types::Type;

#[test]
fn applying_a_known_structure_reports_an_application_error_in_standalone_checking() {
    let mut sources = SourceMap::default();
    let file = sources.add(
        "Probe.fl",
        "module Probe exposing (Bad)\nmodule Bad = String(Prelude)\n",
    );
    let diagnostics = crate::check(&sources, file);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == codes::T002 && d.message.contains("not a parameterized module")),
        "{diagnostics:?}"
    );
}

#[test]
fn dependent_signatures_keep_manifest_type_constructor_parameters_bound() {
    use crate::ir::tests::check_text;
    use crate::types::{Form, ModuleRequirement, ModuleSignature};
    let source = "module Contracts exposing (..)\ntype module Input =\n  type Items a = List a\ntype module Output =\n  type Value\ntype module Factory = (K : Input) -> Output with type Value = K.Items Int\n";
    let (_, interface) = check_text("Contracts", source).expect("signature checks");
    let definition = interface
        .find(Namespace::Type, "Factory")
        .and_then(|e| e.definition.as_ref())
        .expect("factory");
    let Form::ModuleSignature { signature } = &definition.form else {
        panic!("arrow signature")
    };
    let ModuleSignature::Parameterized { result, .. } = signature.as_ref() else {
        panic!("factory")
    };
    let ModuleSignature::Structure { requirements } = result.as_ref() else {
        panic!("result")
    };
    let value = requirements
        .iter()
        .find_map(|r| match r {
            ModuleRequirement::Type { name, manifest, .. } if name == "Value" => manifest.as_ref(),
            _ => None,
        })
        .expect("manifest value");
    let mut variables = Vec::new();
    value.visit(&mut |ty| {
        if let Type::Var(name) = ty {
            variables.push(name.clone());
        }
    });
    assert!(variables.is_empty(), "unbound alias parameter: {value:?}");
}

#[test]
fn single_file_hom_captures_only_earlier_declarations() {
    let source = "module App.Factory exposing (Make)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\noffset : Int\nlet offset = 1\nmodule Make(K : Input) : Output =\n  answer : Int\n  let answer = K.seed + offset\n";
    let mut sources = SourceMap::default();
    let file = sources.add("App/Factory.fl", source);
    let diagnostics = crate::check(&sources, file);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");

    let late = source.replace("offset : Int\nlet offset = 1\n", "");
    let file = sources.add(
        "App/Late.fl",
        format!("{late}offset : Int\nlet offset = 1\n"),
    );
    let diagnostics = crate::check(&sources, file);
    assert!(
        diagnostics.iter().any(|d| d.severity == Severity::Error
            && d.code == codes::T001
            && d.message.contains("offset")),
        "{diagnostics:?}"
    );
}
