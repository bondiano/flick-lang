#[cfg(feature = "ide")]
#[test]
fn parameterized_module_argument_must_satisfy_the_contract_at_the_application() {
    use crate::diagnostics::codes;

    let host = Memory(BTreeMap::from([
        (
            PathBuf::from("/p/src/App/Contracts.fl"),
            "module App.Contracts exposing (Source)\ntype module Source =\n  required : Int\n".into()),
        (
            PathBuf::from("/p/src/App/Argument.fl"),
            "module App.Argument exposing (other)\nother : Int\nlet other = 41\n".into()),
        (
            PathBuf::from("/p/src/App/Make.fl"),
            "module App.Make(K : App.Contracts.Source) exposing (answer)\nanswer : Int\nlet answer = 42\n".into()),
        (
            PathBuf::from("/p/src/App/Collections.fl"),
            "module App.Collections exposing (Names)\nmodule Names = App.Make(App.Argument)\n".into()),
    ]));
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Collections.fl")],
    );
    assert!(
        analysis.diagnostics.iter().any(|d| d.code == codes::T060
            && d.span
                .is_some_and(|span| sources.get(span.file).path.contains("Collections.fl"))),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn parameterized_module_rejects_same_shape_with_different_class_and_service_identities() {
    use crate::diagnostics::codes;

    let host = Memory(BTreeMap::from([
        (PathBuf::from("/p/src/App/Core.fl"), "module App.Core exposing (Equality(..), Clock)\nclass Equality a =\n  equal : a -> a -> Bool\nservice Clock =\n  now : Unit ->{} Int\n".into()),
        (PathBuf::from("/p/src/App/Other.fl"), "module App.Other exposing (Equality(..), Clock)\nclass Equality a =\n  equal : a -> a -> Bool\nservice Clock =\n  now : Unit ->{} Int\n".into()),
        (PathBuf::from("/p/src/App/Contracts.fl"), "module App.Contracts exposing (Source)\nimport App.Core as Core\ntype module Source =\n  class Equality = Core.Equality\n  service Clock = Core.Clock\n".into()),
        (PathBuf::from("/p/src/App/Argument.fl"), "module App.Argument exposing (Equality(..), Clock)\nimport App.Other exposing (Equality(..), Clock)\n".into()),
        (PathBuf::from("/p/src/App/Make.fl"), "module App.Make(K : App.Contracts.Source) exposing (answer)\nanswer : Int\nlet answer = 1\n".into()),
        (PathBuf::from("/p/src/App/Parent.fl"), "module App.Parent exposing (Chosen)\nmodule Chosen = App.Make(App.Argument)\n".into()),
    ]));
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Parent.fl")],
    );
    let wrong_identities: Vec<_> = analysis
        .diagnostics
        .iter()
        .filter(|d| d.code == codes::T062)
        .collect();
    assert_eq!(wrong_identities.len(), 2, "{:?}", analysis.diagnostics);
    assert!(wrong_identities.iter().all(|d| {
        d.span
            .is_some_and(|span| sources.get(span.file).path.contains("Parent.fl"))
    }));
}

#[cfg(feature = "ide")]
#[test]
fn abstract_class_requirement_checks_functional_dependencies() {
    use crate::diagnostics::codes;

    let host = Memory(BTreeMap::from([
        (PathBuf::from("/p/src/App/Contracts.fl"), "module App.Contracts exposing (Policy)\ntype module Policy =\n  class Convert a b | a -> b =\n    convert : a -> b\n".into()),
        (PathBuf::from("/p/src/App/Argument.fl"), "module App.Argument exposing (Convert(..))\nclass Convert a b | b -> a =\n  convert : a -> b\n".into()),
        (PathBuf::from("/p/src/App/Make.fl"), "module App.Make(K : App.Contracts.Policy) exposing (answer)\nanswer : Int\nlet answer = 1\n".into()),
        (PathBuf::from("/p/src/App/Parent.fl"), "module App.Parent exposing (Chosen)\nmodule Chosen = App.Make(App.Argument)\n".into()),
    ]));
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Parent.fl")],
    );
    assert!(
        analysis.diagnostics.iter().any(|d| d.code == codes::T061),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn abstract_class_requirement_can_name_its_dictionary_type() {
    let host = Memory(BTreeMap::from([
        (PathBuf::from("/p/src/App/Contracts.fl"), "module App.Contracts exposing (Policy)\ntype module Policy =\n  class Equality a =\n    equal : a -> a -> Bool\n  equality : Equality.Dict String\n  instance Equality String from equality\n".into()),
        (PathBuf::from("/p/src/App/Argument.fl"), "module App.Argument exposing (Equality(..), equality)\nclass Equality a =\n  equal : a -> a -> Bool\nequality : Equality.Dict String\nlet equality = Equality.dictionary { equal = fun _ _ -> false }\n".into()),
        (PathBuf::from("/p/src/App/Make.fl"), "module App.Make(K : App.Contracts.Policy) exposing (answer)\nanswer : Bool\nlet answer = K.equal \"A\" \"A\"\n".into()),
        (PathBuf::from("/p/src/App/Parent.fl"), "module App.Parent exposing (Chosen)\nmodule Chosen = App.Make(App.Argument)\n".into()),
    ]));
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Parent.fl")],
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn two_local_dictionaries_for_the_same_constraint_are_rejected() {
    use crate::diagnostics::codes;

    let host = Memory(BTreeMap::from([
        (PathBuf::from("/p/src/App/Contracts.fl"), "module App.Contracts exposing (Policy)\ntype module Policy =\n  type Key = String\n  equality : Eq.Dict Key\n  instance Eq Key from equality\n".into()),
        (PathBuf::from("/p/src/App/A.fl"), "module App.A exposing (Key, equality)\ntype alias Key = String\nequality : Eq.Dict Key\nlet equality = Eq.dictionary { eq = fun _ _ -> false }\n".into()),
        (PathBuf::from("/p/src/App/B.fl"), "module App.B exposing (Key, equality)\ntype alias Key = String\nequality : Eq.Dict Key\nlet equality = Eq.dictionary { eq = fun _ _ -> true }\n".into()),
        (PathBuf::from("/p/src/App/Make.fl"), "module App.Make(A : App.Contracts.Policy, B : App.Contracts.Policy) exposing (answer)\nanswer : Bool\nlet answer = eq \"x\" \"x\"\n".into()),
        (PathBuf::from("/p/src/App/Parent.fl"), "module App.Parent exposing (Chosen)\nmodule Chosen = App.Make(App.A, App.B)\n".into()),
    ]));
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Parent.fl")],
    );
    let diagnostic = analysis.diagnostics.iter().find(|d| d.code == codes::T066)
        .expect("ambiguous module dictionaries are rejected");
    assert!(diagnostic.message.contains("parameterized module"), "{}", diagnostic.message);
}

#[cfg(feature = "ide")]
#[test]
fn unpacked_abstract_result_matches_an_explicit_existential_signature() {
    let host = Memory(BTreeMap::from([
        (PathBuf::from("/p/src/App/Contracts.fl"), "module App.Contracts exposing (Box)\ntype module Box =\n  type Key\n  value : Key\n".into()),
        (PathBuf::from("/p/src/App/Argument.fl"), "module App.Argument exposing (Key, value)\ntype alias Key = String\nvalue : Key\nlet value = \"x\"\n".into()),
        (PathBuf::from("/p/src/App/Main.fl"), "module App.Main exposing (result)\nimport App.Contracts exposing (Box)\nimport App.Argument as Arg\nresult : exists K : Box. K.Key\nlet result = unpack (pack Arg as Box) as K in K.value\n".into()),
        (PathBuf::from("/p/src/App/Use.fl"), "module App.Use exposing (answer)\nimport App.Main exposing (result)\nanswer : Int\nlet answer = unpack result as K, item in (fun _ -> 1) item\n".into()),
    ]));
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Use.fl")],
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let main = analysis
        .modules
        .iter()
        .find(|m| m.name == "App.Main")
        .expect("main module");
    let result = main
        .interface
        .exports
        .iter()
        .find(|e| e.entity.name == "result")
        .expect("result export");
    assert!(
        matches!(result.scheme, Some(types::Type::Exists { .. })),
        "{result:?}"
    );
}

#[cfg(feature = "ide")]
#[test]
fn independent_existential_openings_keep_distinct_type_witnesses() {
    use crate::diagnostics::codes;
    let host = Memory(BTreeMap::from([
        (PathBuf::from("/p/src/App/Contracts.fl"), "module App.Contracts exposing (Box)\ntype module Box =\n  type Key\n  value : Key\n".into()),
        (PathBuf::from("/p/src/App/Argument.fl"), "module App.Argument exposing (Key, value)\ntype alias Key = String\nvalue : Key\nlet value = \"x\"\n".into()),
        (PathBuf::from("/p/src/App/Packed.fl"), "module App.Packed exposing (result)\nimport App.Contracts exposing (Box)\nimport App.Argument as Arg\nresult : exists K : Box. K.Key\nlet result = unpack (pack Arg as Box) as K in K.value\n".into()),
        (PathBuf::from("/p/src/App/Use.fl"), "module App.Use exposing (bad)\nimport App.Packed exposing (result)\nsame : forall a. a -> a -> Int\nlet same _ _ = 1\nbad : Int\nlet bad = unpack result as A, a in unpack result as B, b in same a b\n".into()),
    ]));
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Use.fl")],
    );
    assert!(
        analysis.diagnostics.iter().any(|d| d.code == codes::T003),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn public_existential_result_requires_an_explicit_signature() {
    use crate::diagnostics::codes;
    let host = Memory(BTreeMap::from([
        (PathBuf::from("/p/src/App/Contracts.fl"), "module App.Contracts exposing (Box)\ntype module Box =\n  type Key\n  value : Key\n".into()),
        (PathBuf::from("/p/src/App/Argument.fl"), "module App.Argument exposing (Key, value)\ntype alias Key = String\nvalue : Key\nlet value = \"x\"\n".into()),
        (PathBuf::from("/p/src/App/Main.fl"), "module App.Main exposing (result)\nimport App.Contracts exposing (Box)\nimport App.Argument as Arg\nlet result = unpack (pack Arg as Box) as K in K.value\n".into()),
    ]));
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Main.fl")],
    );
    assert!(
        analysis.diagnostics.iter().any(|d| d.code == codes::T064),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn parameterized_module_body_sees_only_the_contract_projection() {
    let host = Memory(BTreeMap::from([
        (PathBuf::from("/p/src/App/Contracts.fl"), "module App.Contracts exposing (Source)\ntype module Source =\n  value : Int\n".into()),
        (PathBuf::from("/p/src/App/Argument.fl"), "module App.Argument exposing (value, extra)\nvalue : Int\nlet value = 41\nextra : Int\nlet extra = 1\n".into()),
        (PathBuf::from("/p/src/App/Make.fl"), "module App.Make(K : App.Contracts.Source) exposing (answer)\nanswer : Int\nlet answer = K.extra\n".into()),
        (PathBuf::from("/p/src/App/Collections.fl"), "module App.Collections exposing (Names)\nmodule Names = App.Make(App.Argument)\n".into()),
    ]));
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Collections.fl")],
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.message.contains("extra")),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn two_named_parameterized_module_applications_have_distinct_abstract_result_types() {
    use crate::diagnostics::codes;

    let host = Memory(BTreeMap::from([
        (PathBuf::from("/p/src/App/Contracts.fl"), "module App.Contracts exposing (Source)\ntype module Source =\n  type Key\n".into()),
        (PathBuf::from("/p/src/App/Argument.fl"), "module App.Argument exposing (Key)\ntype alias Key = String\n".into()),
        (PathBuf::from("/p/src/App/Make.fl"), "module App.Make(K : App.Contracts.Source) exposing (Token)\ntype Token\n".into()),
        (PathBuf::from("/p/src/App/Parent.fl"), "module App.Parent exposing (A, B)\nmodule A = App.Make(App.Argument)\nmodule B = App.Make(App.Argument)\n".into()),
        (PathBuf::from("/p/src/App/Use.fl"), "module App.Use exposing (mix)\nimport App.Parent.A as A\nimport App.Parent.B as B\nmix : A.Token -> B.Token\nlet mix x = x\n".into()),
    ]));
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Use.fl")],
    );
    let mismatch = analysis
        .diagnostics
        .iter()
        .find(|d| d.code == codes::T003)
        .expect("distinct results must not unify");
    assert!(
        mismatch.message.contains("App.Parent.A.Token")
            && mismatch.message.contains("App.Parent.B.Token"),
        "{mismatch:?}"
    );
    assert!(
        mismatch
            .fixes
            .iter()
            .any(|fix| fix.message.contains("distinct nominal types")),
        "{mismatch:?}"
    );
}

#[cfg(feature = "ide")]
#[test]
fn abstract_module_parameter_hides_the_arguments_alias() {
    use crate::diagnostics::codes;

    let host = Memory(BTreeMap::from([
        (PathBuf::from("/p/src/App/Contracts.fl"), "module App.Contracts exposing (Source)\ntype module Source =\n  type Key\n".into()),
        (PathBuf::from("/p/src/App/Argument.fl"), "module App.Argument exposing (Key)\ntype alias Key = String\n".into()),
        (PathBuf::from("/p/src/App/Make.fl"), "module App.Make(K : App.Contracts.Source) exposing (answer)\nanswer : K.Key\nlet answer = \"string\"\n".into()),
        (PathBuf::from("/p/src/App/Parent.fl"), "module App.Parent exposing (A)\nmodule A = App.Make(App.Argument)\n".into()),
    ]));
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Parent.fl")],
    );
    assert!(
        analysis.diagnostics.iter().any(|d| d.code == codes::T003),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn virtual_parameterized_module_path_conflicts_with_a_file_module() {
    use crate::diagnostics::codes;

    let host = Memory(BTreeMap::from([
        (PathBuf::from("/p/src/App/Contracts.fl"), "module App.Contracts exposing (Source)\ntype module Source =\n  value : Int\n".into()),
        (PathBuf::from("/p/src/App/Argument.fl"), "module App.Argument exposing (value)\nvalue : Int\nlet value = 1\n".into()),
        (PathBuf::from("/p/src/App/Make.fl"), "module App.Make(K : App.Contracts.Source) exposing (answer)\nanswer : Int\nlet answer = K.value\n".into()),
        (PathBuf::from("/p/src/App/Parent.fl"), "module App.Parent exposing (A)\nmodule A = App.Make(App.Argument)\n".into()),
        (PathBuf::from("/p/src/App/Parent/A.fl"), "module App.Parent.A exposing (answer)\nanswer : Int\nlet answer = 2\n".into()),
        (PathBuf::from("/p/src/App/Use.fl"), "module App.Use exposing (answer)\nimport App.Parent\nimport App.Parent.A exposing (answer)\n".into()),
    ]));
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Use.fl")],
    );
    assert!(
        analysis.diagnostics.iter().any(|d| d.code == codes::R002),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn changing_a_parameterized_module_contract_invalidates_the_virtual_module_cache() {
    use crate::diagnostics::codes;

    let host = |contract: &str| {
        Memory(BTreeMap::from([
        (PathBuf::from("/p/src/App/Contracts.fl"), "module App.Contracts exposing (A, B)\ntype module A =\n  value : Int\ntype module B =\n  value : String\n".into()),
        (PathBuf::from("/p/src/App/Argument.fl"), "module App.Argument exposing (value)\nvalue : Int\nlet value = 1\n".into()),
        (PathBuf::from("/p/src/App/Make.fl"), format!("module App.Make(K : App.Contracts.{contract}) exposing (answer)\nanswer : Int\nlet answer = K.value\n")),
        (PathBuf::from("/p/src/App/Parent.fl"), "module App.Parent exposing (Result)\nmodule Result = App.Make(App.Argument)\n".into()),
    ]))
    };
    let roots = [PathBuf::from("/p/src/App/Parent.fl")];
    let mut cache = ModuleCache::default();
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let first = analyze_with(&host("A"), &mut sources, Some(&project), &roots, &mut cache);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let changed = analyze_with(&host("B"), &mut sources, Some(&project), &roots, &mut cache);
    assert!(
        changed.diagnostics.iter().any(|d| d.code == codes::T061),
        "{:?}",
        changed.diagnostics
    );
}
