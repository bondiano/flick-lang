#[cfg(feature = "ide")]
#[test]
fn functor_argument_must_satisfy_the_contract_at_the_application() {
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
fn functor_rejects_same_shape_with_different_class_and_service_identities() {
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
    assert!(
        analysis.diagnostics.iter().any(|d| d.code == codes::T066),
        "{:?}",
        analysis.diagnostics
    );
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
fn functor_body_sees_only_the_contract_projection() {
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
fn two_named_functor_applications_have_distinct_abstract_result_types() {
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
    #[cfg(feature = "diagnostic-fixes")]
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
fn abstract_functor_parameter_hides_the_arguments_alias() {
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
fn virtual_functor_path_conflicts_with_a_file_module() {
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
fn changing_a_functor_contract_invalidates_the_virtual_module_cache() {
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
#[cfg(feature = "ide")]
#[test]
fn higher_order_factory_is_checked_and_applied_through_its_parameter() {
    let mut host = Memory(BTreeMap::from([
        (PathBuf::from("/p/src/App/Contracts.fl"), "module App.Contracts exposing (Input, Output, Factory)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Factory = (K : Input) -> Output\n".into()),
        (PathBuf::from("/p/src/App/Argument.fl"), "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n".into()),
        (PathBuf::from("/p/src/App/Make.fl"), "module App.Make(K : App.Contracts.Input) : App.Contracts.Output exposing (answer)\nanswer : Int\nlet answer = K.seed + 1\n".into()),
        (PathBuf::from("/p/src/App/Apply.fl"), "module App.Apply(F : App.Contracts.Factory, K : App.Contracts.Input) : App.Contracts.Output exposing (answer)\nmodule Built = F(K)\nanswer : Int\nlet answer = Built.answer\n".into()),
        (PathBuf::from("/p/src/App/Main.fl"), "module App.Main exposing (Chosen)\nmodule Chosen = App.Apply(App.Make, App.Argument)\n".into()),
    ]));
    for curried in [false, true] {
        if curried {
            let apply = host
                .0
                .get_mut(&PathBuf::from("/p/src/App/Apply.fl"))
                .expect("apply");
            *apply = apply.replace("Factory, K", "Factory)(K");
            let main = host
                .0
                .get_mut(&PathBuf::from("/p/src/App/Main.fl"))
                .expect("main");
            *main = "module App.Main exposing (Chosen)\nmodule Half = App.Apply(App.Make)\nmodule Chosen = App.Main.Half(App.Argument)\n".into();
        }
        let mut sources = SourceMap::default();
        let project = library(&mut sources);
        let analysis = analyze(
            &host,
            &mut sources,
            Some(&project),
            &[PathBuf::from("/p/src/App/Main.fl")],
        );
        assert!(
            analysis.diagnostics.is_empty(),
            "{:?}",
            analysis.diagnostics
        );
        let result = analysis
            .modules
            .iter()
            .find(|m| m.name == "App.Main.Chosen");
        assert!(result.is_some(), "named result must be materialized");
    }
}

#[cfg(feature = "ide")]
#[test]
fn anonymous_and_returned_factories_are_module_values() {
    for (factory, main) in [
        (
            "module App.Return(F : App.Contracts.Factory) : App.Contracts.Factory = F\n",
            "module App.Main exposing (Chosen)\nmodule First = App.Return(App.Make)\nmodule Second = App.Return(App.Main.First)\nmodule Chosen = App.Main.Second(App.Argument)\n",
        ),
        (
            "module App.Return exposing (Make)\nmodule Make(K : App.Contracts.Input) : App.Contracts.Output =\n  answer : Int\n  let answer = K.seed + 1\n",
            "module App.Main exposing (Chosen)\nmodule Chosen = App.Return.Make(App.Argument)\n",
        ),
        (
            "module App.Return(F : App.Contracts.Factory) : App.Contracts.Factory = functor (K : App.Contracts.Input) ->\n  module Built = F(K)\n  answer : Int\n  let answer = Built.answer\n",
            "module App.Main exposing (Chosen)\nmodule Selected = App.Return(App.Make)\nmodule Chosen = App.Main.Selected(App.Argument)\n",
        ),
        (
            "module App.Return(F : App.Contracts.Factory) : App.Contracts.Factory = F\n",
            "module App.Main exposing (Chosen)\nmodule Selected = App.Return(App.Make)\nmodule Chosen = App.Main.Selected(App.Argument)\n",
        ),
        (
            "module App.Return(F : App.Contracts.Factory)(K : App.Contracts.Input) : App.Contracts.Output\nmodule Built = F(K)\nanswer : Int\nlet answer = Built.answer\n",
            "module App.Main exposing (Chosen)\nmodule Chosen = App.Return((functor (K : App.Contracts.Input) ->\n  answer : Int\n  let answer = K.seed + 1\n))(App.Argument)\n",
        ),
    ] {
        let host = Memory(BTreeMap::from([
            (PathBuf::from("/p/src/App/Contracts.fl"), "module App.Contracts exposing (Input, Output, Factory)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Factory = (K : Input) -> Output\n".into()),
            (PathBuf::from("/p/src/App/Argument.fl"), "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n".into()),
            (PathBuf::from("/p/src/App/Make.fl"), "module App.Make(K : App.Contracts.Input) : App.Contracts.Output exposing (answer)\nanswer : Int\nlet answer = K.seed + 1\n".into()),
            (PathBuf::from("/p/src/App/Return.fl"), factory.into()),
            (PathBuf::from("/p/src/App/Main.fl"), main.into()),
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
            analysis.diagnostics.is_empty(),
            "{main}: {:?}",
            analysis.diagnostics
        );
        let chosen = analysis.module("App.Main.Chosen").expect("named result");
        let answer = chosen
            .interface
            .find(Namespace::Value, "answer")
            .expect("answer");
        assert_eq!(answer.scheme, Some(types::Type::con("Prelude", "Int")));
    }
}

#[cfg(feature = "ide")]
#[test]
fn cyclic_module_aliases_are_diagnosed_without_recursing() {
    use crate::diagnostics::codes;
    let source = "module App.Main exposing (A, B)\nmodule A = B\nmodule B = A\n";
    let host = Memory(BTreeMap::from([(
        PathBuf::from("/p/src/App/Main.fl"),
        source.to_owned(),
    )]));
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Main.fl")],
    );
    let cycle = analysis
        .diagnostics
        .iter()
        .find(|d| d.code == codes::R006)
        .expect("module cycle");
    let span = cycle.span.expect("cycle span");
    assert!(matches!(
        &source[span.start as usize..span.end as usize],
        "A" | "B"
    ));
}

#[cfg(feature = "ide")]
fn hom_boundary_analysis(files: &[(&str, &str)], root: &str) -> (SourceMap, super::Analysis) {
    let host = Memory(
        files
            .iter()
            .map(|(name, source)| {
                (
                    PathBuf::from(format!("/p/src/App/{name}.fl")),
                    (*source).to_owned(),
                )
            })
            .collect(),
    );
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from(format!("/p/src/App/{root}.fl"))],
    );
    (sources, analysis)
}

#[cfg(feature = "ide")]
#[test]
fn hom_boundary_returned_factory_hides_implementation_result_members() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Output, Rich, Factory)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Rich =\n  answer : Int\n  extra : Int\ntype module Factory = (K : Input) -> Output\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
            ),
            (
                "Make",
                "module App.Make(K : App.Contracts.Input) : App.Contracts.Rich\nanswer : Int\nlet answer = K.seed + 1\nextra : Int\nlet extra = 99\n",
            ),
            (
                "Return",
                "module App.Return(F : App.Contracts.Factory) : App.Contracts.Factory = F\n",
            ),
            (
                "Main",
                "module App.Main exposing (Chosen)\nmodule Selected = App.Return(App.Make)\nmodule Chosen = App.Main.Selected(App.Argument)\n",
            ),
        ],
        "Main",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let chosen = analysis.module("App.Main.Chosen").expect("chosen");
    assert!(chosen.interface.find(Namespace::Value, "answer").is_some());
    assert!(
        chosen.interface.find(Namespace::Value, "extra").is_none(),
        "result signature must hide implementation members"
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_boundary_returned_factory_keeps_its_advertised_input_requirement() {
    use crate::diagnostics::codes;
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Strict, Output, Factory)\ntype module Input =\n  seed : Int\ntype module Strict =\n  seed : Int\n  required : Int\ntype module Output =\n  answer : Int\ntype module Factory = (K : Strict) -> Output\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
            ),
            (
                "Make",
                "module App.Make(K : App.Contracts.Input) : App.Contracts.Output\nanswer : Int\nlet answer = K.seed + 1\n",
            ),
            (
                "Return",
                "module App.Return(F : App.Contracts.Factory) : App.Contracts.Factory = F\n",
            ),
            (
                "Main",
                "module App.Main exposing (Chosen)\nmodule Selected = App.Return(App.Make)\nmodule Chosen = App.Main.Selected(App.Argument)\n",
            ),
        ],
        "Main",
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == codes::T060 && d.message.contains("required")),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_boundary_shadowed_parameters_do_not_equate_outer_and_inner_types() {
    use crate::diagnostics::codes;
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Source, Output, Factory)\ntype module Source =\n  type Key\n  value : Key\ntype module Output =\n  type Value\n  value : Value\ntype module Factory = (K : Source) -> Output with type Value = K.Key\n",
            ),
            (
                "Outer",
                "module App.Outer(K : App.Contracts.Source) : App.Contracts.Factory = functor (K : App.Contracts.Source) ->\n  type alias Value = K.Key\n  value : Value\n  let value = Saved.value\nmodule Saved = K\n",
            ),
        ],
        "Outer",
    );
    assert!(
        analysis.diagnostics.iter().any(|d| d.code == codes::T003),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_boundary_atomic_group_cannot_satisfy_a_curried_signature() {
    use crate::diagnostics::codes;
    let (_, analysis) = hom_boundary_analysis(&[
        ("Contracts", "module App.Contracts exposing (Input, Output, Factory)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Factory = (A : Input) -> (B : Input) -> Output\n"),
        ("Make", "module App.Make(A : App.Contracts.Input, B : App.Contracts.Input) : App.Contracts.Output\nanswer : Int\nlet answer = A.seed + B.seed\n"),
        ("Higher", "module App.Higher(F : App.Contracts.Factory) : App.Contracts.Output\nanswer : Int\nlet answer = 42\n"),
        ("Root", "module App.Root exposing (Chosen)\nmodule Chosen = App.Higher(App.Make)\n"),
    ], "Root");
    assert!(analysis.diagnostics.iter().any(|d| d.code == codes::T061), "{:?}", analysis.diagnostics);
}
