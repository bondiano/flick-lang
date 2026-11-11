#[cfg(feature = "ide")]
#[test]
fn hom_builtin_modules_are_static_arguments_and_aliases() {
    let host = Memory(BTreeMap::from([(
        PathBuf::from("/p/src/App/Main.fl"),
        "module App.Main exposing (answer)\ntype module Input =\n  identity : forall a. a -> a\ntype module Output =\n  answer : Int\noffset : Int\nlet offset = 1\nmodule Make(K : Input) : Output =\n  answer : Int\n  let answer = K.identity (41 + offset)\nmodule Alias = Prelude\nmodule Chosen = Make(Alias)\nanswer : Int\nlet answer = Chosen.answer\n".into(),
    )]));
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
}

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
            "module App.Make(K : App.Contracts.Source) : PublicResult exposing (answer)\nanswer : Int\nlet answer = 42\n\ntype module PublicResult =\n  answer : Int\n".into()),
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
        (PathBuf::from("/p/src/App/Make.fl"), "module App.Make(K : App.Contracts.Source) : PublicResult exposing (answer)\nanswer : Int\nlet answer = 1\n\ntype module PublicResult =\n  answer : Int\n".into()),
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
        (PathBuf::from("/p/src/App/Make.fl"), "module App.Make(K : App.Contracts.Policy) : PublicResult exposing (answer)\nanswer : Int\nlet answer = 1\n\ntype module PublicResult =\n  answer : Int\n".into()),
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
        (PathBuf::from("/p/src/App/Make.fl"), "module App.Make(K : App.Contracts.Policy) : PublicResult exposing (answer)\nanswer : Bool\nlet answer = K.equal \"A\" \"A\"\n\ntype module PublicResult =\n  answer : Bool\n".into()),
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
        (PathBuf::from("/p/src/App/Make.fl"), "module App.Make(A : App.Contracts.Policy, B : App.Contracts.Policy) : PublicResult exposing (answer)\nanswer : Bool\nlet answer = eq \"x\" \"x\"\n\ntype module PublicResult =\n  answer : Bool\n".into()),
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
        (PathBuf::from("/p/src/App/Make.fl"), "module App.Make(K : App.Contracts.Source) : PublicResult exposing (answer)\nanswer : Int\nlet answer = K.extra\n\ntype module PublicResult =\n  answer : Int\n".into()),
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
        (PathBuf::from("/p/src/App/Make.fl"), "module App.Make(K : App.Contracts.Source) : PublicResult exposing (Token)\ntype Token\n\ntype module PublicResult =\n  type Token\n".into()),
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
        (PathBuf::from("/p/src/App/Make.fl"), "module App.Make(K : App.Contracts.Source) : PublicResult exposing (answer)\nanswer : K.Key\nlet answer = \"string\"\n\ntype module PublicResult =\n  answer : K.Key\n".into()),
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
        (PathBuf::from("/p/src/App/Make.fl"), "module App.Make(K : App.Contracts.Source) : PublicResult exposing (answer)\nanswer : Int\nlet answer = K.value\n\ntype module PublicResult =\n  answer : Int\n".into()),
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
        (PathBuf::from("/p/src/App/Make.fl"), format!("module App.Make(K : App.Contracts.{contract}) : PublicResult exposing (answer)\nanswer : Int\nlet answer = K.value\n\ntype module PublicResult =\n  answer : Int\n")),
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
    let (sources, analysis) = hom_boundary_analysis(
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
    let mismatch = analysis
        .diagnostics
        .iter()
        .find(|d| d.code == codes::T003)
        .unwrap_or_else(|| panic!("{:?}", analysis.diagnostics));
    let span = mismatch.span.expect("type mismatch span");
    assert_eq!(
        &sources.get(span.file).text[span.start as usize..span.end as usize],
        "Saved.value"
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_boundary_atomic_group_cannot_satisfy_a_curried_signature() {
    use crate::diagnostics::codes;
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Output, Factory)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Factory = (A : Input) -> (B : Input) -> Output\n",
            ),
            (
                "Make",
                "module App.Make(A : App.Contracts.Input, B : App.Contracts.Input) : App.Contracts.Output\nanswer : Int\nlet answer = A.seed + B.seed\n",
            ),
            (
                "Higher",
                "module App.Higher(F : App.Contracts.Factory) : App.Contracts.Output\nanswer : Int\nlet answer = 42\n",
            ),
            (
                "Root",
                "module App.Root exposing (Chosen)\nmodule Chosen = App.Higher(App.Make)\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis.diagnostics.iter().any(|d| d.code == codes::T061),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_grouped_signature_accepts_and_applies_a_matching_factory() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Output, Factory)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Factory = (A : Input, B : Input) -> Output\n",
            ),
            (
                "Make",
                "module App.Make(A : App.Contracts.Input, B : App.Contracts.Input) : App.Contracts.Output\nanswer : Int\nlet answer = A.seed + B.seed\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 21\n",
            ),
            (
                "Higher",
                "module App.Higher(F : App.Contracts.Factory, K : App.Contracts.Input) : App.Contracts.Output = F(K, K)\n",
            ),
            (
                "Root",
                "module App.Root exposing (Chosen)\nmodule Chosen = App.Higher(App.Make, App.Argument)\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_boundary_parameter_views_stay_separate_for_the_same_argument() {
    use crate::diagnostics::codes;
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Small, Large, Output)\ntype module Small =\n  seed : Int\ntype module Large =\n  seed : Int\n  secret : Int\ntype module Output =\n  answer : Int\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed, secret)\nseed : Int\nlet seed = 1\nsecret : Int\nlet secret = 42\n",
            ),
            (
                "Make",
                "module App.Make(A : App.Contracts.Small, B : App.Contracts.Large) : App.Contracts.Output\nanswer : Int\nlet answer = A.secret\n",
            ),
            (
                "Root",
                "module App.Root exposing (Chosen)\nmodule Chosen = App.Make(App.Argument, App.Argument)\n",
            ),
        ],
        "Root",
    );
    let errors: Vec<_> = analysis
        .diagnostics
        .iter()
        .filter(|d| {
            (d.code == codes::T001 || d.code == codes::R004) && d.message.contains("secret")
        })
        .collect();
    // Both the symbolic template and its concrete application must reject it.
    assert_eq!(errors.len(), 2, "{:?}", analysis.diagnostics);
}

#[cfg(feature = "ide")]
#[test]
fn hom_dependent_results_preserve_types_through_argument_aliases() {
    use crate::diagnostics::codes;
    for (declaration, rejected) in [("type alias", false), ("opaque type alias", true)] {
        let argument = format!(
            "module App.Argument exposing (Key, value)\n{declaration} Key = String\nvalue : Key\nlet value = \"x\"\n"
        );
        let (sources, analysis) = hom_boundary_analysis(
            &[
                (
                    "Contracts",
                    "module App.Contracts exposing (Source, Output)\ntype module Source =\n  type Key\n  value : Key\ntype module Output =\n  type Value\n  value : Value\n",
                ),
                ("Argument", &argument),
                (
                    "Make",
                    "module App.Make(K : App.Contracts.Source) : App.Contracts.Output with type Value = K.Key\ntype alias Value = K.Key\nvalue : Value\nlet value = K.value\n",
                ),
                (
                    "Root",
                    "module App.Root exposing (Chosen)\nmodule Alias = App.Argument\nmodule Chosen = App.Make(Alias)\n",
                ),
                (
                    "Use",
                    "module App.Use exposing (value)\nimport App.Root.Chosen as Chosen\nvalue : String\nlet value = Chosen.value\n",
                ),
            ],
            "Use",
        );
        if rejected {
            assert!(
                analysis.diagnostics.iter().any(|d| d.code == codes::T003
                    && d.span
                        .is_some_and(|s| sources.get(s.file).path == "/p/src/App/Use.fl")),
                "{:?}",
                analysis.diagnostics
            );
        } else {
            assert!(
                analysis.diagnostics.is_empty(),
                "{:?}",
                analysis.diagnostics
            );
        }
    }
}

#[cfg(feature = "ide")]
#[test]
fn hom_boundary_a_structure_alias_cannot_widen_a_parameter() {
    use crate::diagnostics::codes;
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Output)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed, secret)\nseed : Int\nlet seed = 1\nsecret : Int\nlet secret = 42\n",
            ),
            (
                "Make",
                "module App.Make(K : App.Contracts.Input) : App.Contracts.Output\nmodule Saved = K\nanswer : Int\nlet answer = Saved.secret\n",
            ),
            (
                "Root",
                "module App.Root exposing (Chosen)\nmodule Chosen = App.Make(App.Argument)\n",
            ),
        ],
        "Root",
    );
    let errors: Vec<_> = analysis
        .diagnostics
        .iter()
        .filter(|d| {
            (d.code == codes::T001 || d.code == codes::R004) && d.message.contains("secret")
        })
        .collect();
    assert_eq!(errors.len(), 2, "{:?}", analysis.diagnostics);
}

#[cfg(feature = "ide")]
#[test]
fn hom_curried_parameter_dictionaries_have_nested_scopes() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Policy, Output)\ntype module Policy =\n  type Key = String\n  equality : Eq.Dict Key\n  instance Eq Key from equality\ntype module Output =\n  answer : Bool\n",
            ),
            (
                "A",
                "module App.A exposing (Key, equality)\ntype alias Key = String\nequality : Eq.Dict Key\nlet equality = Eq.dictionary { eq = fun _ _ -> true }\n",
            ),
            (
                "B",
                "module App.B exposing (Key, equality)\ntype alias Key = String\nequality : Eq.Dict Key\nlet equality = Eq.dictionary { eq = fun _ _ -> false }\n",
            ),
            (
                "Make",
                "module App.Make(A : App.Contracts.Policy)(B : App.Contracts.Policy) : App.Contracts.Output\nanswer : Bool\nlet answer = eq \"x\" \"x\"\n",
            ),
            (
                "Root",
                "module App.Root exposing (Chosen)\nmodule Chosen = App.Make(App.A)(App.B)\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_shadowed_anonymous_parameter_keeps_its_own_argument() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Output, Factory)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Factory = (K : Input) -> Output\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
            ),
            (
                "Outer",
                "module App.Outer(K : App.Contracts.Input) : App.Contracts.Factory = functor (K : App.Contracts.Input) ->\n  answer : Int\n  let answer = K.seed + 1\n",
            ),
            (
                "Root",
                "module App.Root exposing (Chosen)\nmodule Selected = App.Outer(App.Argument)\nmodule Chosen = Selected(App.Argument)\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_recursive_static_expansions_report_cycles() {
    use crate::diagnostics::codes;
    for other in [false, true] {
        let make = if other {
            "module App.Make(K : App.Contracts.Input) : App.Contracts.Output = App.Other(K)\n"
        } else {
            "module App.Make(K : App.Contracts.Input) : App.Contracts.Output = App.Make(K)\n"
        };
        let (_, analysis) = hom_boundary_analysis(
            &[
                (
                    "Contracts",
                    "module App.Contracts exposing (Input, Output)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\n",
                ),
                (
                    "Argument",
                    "module App.Argument exposing (seed)\nseed : Int\nlet seed = 1\n",
                ),
                ("Make", make),
                (
                    "Other",
                    "module App.Other(K : App.Contracts.Input) : App.Contracts.Output = App.Make(K)\n",
                ),
                (
                    "Root",
                    "module App.Root exposing (Chosen)\nmodule Chosen = App.Make(App.Argument)\n",
                ),
            ],
            "Root",
        );
        assert!(
            analysis.diagnostics.iter().any(|d| d.code == codes::R006),
            "{:?}",
            analysis.diagnostics
        );
    }
}

#[cfg(feature = "ide")]
#[test]
fn hom_finite_nested_applications_can_reuse_a_functor() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input)\ntype module Input =\n  seed : Int\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 42\n",
            ),
            (
                "Identity",
                "module App.Identity(K : App.Contracts.Input) : App.Contracts.Input = K\n",
            ),
            (
                "Root",
                "module App.Root exposing (Chosen)\nmodule Chosen = App.Identity(App.Identity(App.Argument))\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_nested_expression_functor_does_not_capture_a_path_suffix() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Output)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\n",
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
                "Nest",
                "module App.Nest exposing (Make)\nmodule Make(K : App.Contracts.Input) : App.Contracts.Output = App.Make(K)\n",
            ),
            (
                "Root",
                "module App.Root exposing (Chosen)\nmodule Chosen = App.Nest.Make(App.Argument)\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_manifest_parameter_equalities_are_used_without_opening_abstract_types() {
    use crate::diagnostics::{Severity, codes};
    for manifest in [true, false] {
        let contracts = format!(
            "module App.Contracts exposing (Input, Output, Factory)\ntype module Input =\n  type Item{}\ntype module Output =\n  type Value\n  value : Value\ntype module Factory = (K : Input) -> Output with type Value = K.Item\n",
            if manifest { " = String" } else { "" }
        );
        let (_, analysis) = hom_boundary_analysis(
            &[
                ("Contracts", &contracts),
                (
                    "Argument",
                    "module App.Argument exposing (Item)\ntype alias Item = String\n",
                ),
                (
                    "Make",
                    "module App.Make(K : App.Contracts.Input) : App.Contracts.Output with type Value = K.Item\ntype alias Value = String\nvalue : Value\nlet value = \"x\"\n",
                ),
                (
                    "Return",
                    "module App.Return(F : App.Contracts.Factory) : App.Contracts.Factory = F\n",
                ),
                (
                    "Root",
                    "module App.Root exposing (Chosen)\nmodule Selected = App.Return(App.Make)\nmodule Chosen = Selected(App.Argument)\n",
                ),
            ],
            "Root",
        );
        let errors: Vec<_> = analysis
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        if manifest {
            assert!(errors.is_empty(), "{errors:?}");
        } else {
            assert!(errors.iter().any(|d| d.code == codes::T061), "{errors:?}");
        }
    }
}

#[cfg(feature = "ide")]
#[test]
fn hom_public_factories_require_an_advertised_result() {
    use crate::diagnostics::{Severity, codes};
    for (definition, root, rejected) in [
        (
            "module App.Make(K : App.Contracts.Input) exposing (answer)\nanswer : Int\nlet answer = K.seed\n",
            "module App.Root exposing (Chosen)\nmodule Chosen = App.Make\n",
            true,
        ),
        (
            "private module App.Make(K : App.Contracts.Input) exposing (answer)\nanswer : Int\nlet answer = K.seed\n",
            "module App.Root exposing (answer)\nmodule Local = App.Make\nanswer : Int\nlet answer = 42\n",
            false,
        ),
        (
            "private module App.Make(K : App.Contracts.Input) exposing (answer)\nanswer : Int\nlet answer = K.seed\n",
            "module App.Root exposing (Chosen)\nmodule Chosen = App.Make\n",
            true,
        ),
        (
            "module App.Make(K : App.Contracts.Input) : App.Contracts.Output\nanswer : Int\nlet answer = K.seed\n",
            "module App.Root exposing (Chosen)\nmodule Chosen = App.Make\n",
            false,
        ),
        (
            "private module App.Make(K : App.Contracts.Input) exposing (answer)\nanswer : Int\nlet answer = K.seed\n",
            "module App.Root exposing (Chosen)\nmodule Chosen : App.Contracts.Factory = App.Make\n",
            false,
        ),
        (
            "module App.Make(K : App.Contracts.Input) : App.Contracts.Output\nanswer : Int\nlet answer = K.seed\n",
            "module App.Root exposing (Chosen)\nmodule Chosen(K : App.Contracts.Input) = App.Make(K)\n",
            true,
        ),
    ] {
        let (_, analysis) = hom_boundary_analysis(
            &[
                (
                    "Contracts",
                    "module App.Contracts exposing (Input, Output, Factory)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Factory = (K : Input) -> Output\n",
                ),
                ("Make", definition),
                ("Root", root),
            ],
            "Root",
        );
        let errors: Vec<_> = analysis
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        if rejected {
            assert!(
                errors.iter().any(|d| d.code == codes::T067),
                "{definition}\n{root}\n{errors:?}"
            );
        } else {
            assert!(errors.is_empty(), "{definition}\n{root}\n{errors:?}");
        }
    }
}

#[cfg(feature = "ide")]
#[test]
fn hom_explicit_exposing_agrees_with_the_result_signature() {
    use crate::diagnostics::{Severity, codes};
    for (exposing, rejected) in [
        ("", false),
        (" exposing (answer)", false),
        (" exposing (answer, extra)", true),
        (" exposing (..)", true),
    ] {
        let make = format!(
            "module App.Make(K : App.Contracts.Input) : App.Contracts.Output{exposing}\nanswer : Int\nlet answer = K.seed\nextra : Int\nlet extra = 1\n"
        );
        let (_, analysis) = hom_boundary_analysis(
            &[
                (
                    "Contracts",
                    "module App.Contracts exposing (Input, Output)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\n",
                ),
                ("Make", &make),
            ],
            "Make",
        );
        let errors: Vec<_> = analysis
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        if rejected {
            assert!(
                errors
                    .iter()
                    .any(|d| d.code == codes::T061 && d.message.contains("exposing")),
                "{make}\n{errors:?}"
            );
        } else {
            assert!(errors.is_empty(), "{make}\n{errors:?}");
        }
    }
}

#[cfg(feature = "ide")]
#[test]
fn hom_namespace_exports_are_checked_even_for_materialized_members() {
    use crate::diagnostics::{Severity, codes};
    for (header, imported, rejected) in [
        ("module App.Parent exposing (Visible)", "Hidden", true),
        (
            "module App.Parent : App.Contracts.Namespace",
            "Hidden",
            true,
        ),
        (
            "module App.Parent : App.Contracts.Namespace",
            "Visible",
            false,
        ),
    ] {
        let parent = format!(
            "{header}\nmodule Visible = App.Make(App.Argument)\nmodule Hidden = App.Make(App.Argument)\n"
        );
        let root = format!(
            "module App.Root exposing (answer)\nimport App.Parent\nimport App.Parent.{imported} exposing (answer)\n"
        );
        let (_, analysis) = hom_boundary_analysis(
            &[
                (
                    "Contracts",
                    "module App.Contracts exposing (Input, Output, Namespace)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Namespace =\n  module Visible : Output\n",
                ),
                (
                    "Argument",
                    "module App.Argument exposing (seed)\nseed : Int\nlet seed = 42\n",
                ),
                (
                    "Make",
                    "module App.Make(K : App.Contracts.Input) : App.Contracts.Output\nanswer : Int\nlet answer = K.seed\n",
                ),
                ("Parent", &parent),
                ("Root", &root),
            ],
            "Root",
        );
        let errors: Vec<_> = analysis
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        if rejected {
            assert!(
                errors
                    .iter()
                    .any(|d| d.code == codes::R018 && d.message.contains("not exported")),
                "{parent}\n{root}\n{errors:?}"
            );
        } else {
            assert!(errors.is_empty(), "{parent}\n{root}\n{errors:?}");
        }
    }
}

#[cfg(feature = "ide")]
#[test]
fn hom_exported_factory_member_uses_the_parents_sealed_contract() {
    use crate::diagnostics::codes;
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Output, Rich, Namespace)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Rich =\n  answer : Int\n  extra : Int\ntype module Namespace =\n  module Make : (K : Input) -> Output\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 42\n",
            ),
            (
                "Make",
                "module App.Make(K : App.Contracts.Input) : App.Contracts.Rich\nanswer : Int\nlet answer = K.seed\nextra : Int\nlet extra = 1\n",
            ),
            (
                "Parent",
                "module App.Parent : App.Contracts.Namespace\nmodule Make = App.Make\n",
            ),
            (
                "Root",
                "module App.Root exposing (answer)\nmodule Result = App.Parent.Make(App.Argument)\nanswer : Int\nlet answer = Result.extra\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == codes::R004 && d.message.contains("extra")),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_arrow_binders_retain_custom_nominal_kinds_and_definitions() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Core",
                "module App.Core exposing (Clock, Equality(..))\nservice Clock =\n  now : Unit ->{} Int\nclass Equality a =\n  equal : a -> a -> Bool\n",
            ),
            (
                "Contracts",
                "module App.Contracts exposing (Input, Output, ServiceFactory, ClassFactory)\nimport App.Core as Core\ntype module Input =\n  service Clock = Core.Clock\n  class Equality = Core.Equality\ntype module Output =\n  type Value\ntype module ServiceFactory = (K : Input) -> Output with type Value = (Unit ->{K.Clock} Int)\ntype module ClassFactory = (K : Input) -> Output with type Value = (K.Equality String => String -> Bool)\n",
            ),
        ],
        "Contracts",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_nominal_service_rows_follow_static_witness_substitution() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Repository)\ntype module Input =\n  seed : Int\ntype module Repository =\n  service Repo =\n    get : String ->{} String\n  get : String ->{Repo} String\n  layer : Layer none none {Repo}\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 42\n",
            ),
            (
                "Make",
                "module App.Make(K : App.Contracts.Input) : App.Contracts.Repository\nservice Repo =\n  get : String ->{} String\nget : String ->{Repo} String\nlet get key = Repo.get key\nlayer : Layer none none {Repo}\nlet layer = Layer.succeed Repo { get = fun key -> show K.seed ++ key }\n",
            ),
            (
                "Logged",
                "module App.Logged(Base : App.Contracts.Repository) : Result\ntype module Result =\n  get : String ->{Base.Repo, Log.Log} String\n  layer : Layer none none {Base.Repo}\nget : String ->{Base.Repo, Log.Log} String\nlet get key =\n  Log.info key\n  Base.get key\nlayer : Layer none none {Base.Repo}\nlet layer = Base.layer\n",
            ),
            (
                "Root",
                "module App.Root exposing (Base, Logged)\nmodule Base = App.Make(App.Argument)\nmodule Logged = App.Logged(Base)\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_structure_members_bind_dependent_type_paths() {
    use crate::diagnostics::{Severity, codes};
    for (item, rejected) in [("String", false), ("Int", true)] {
        let child = format!("module App.Child exposing (Item)\ntype alias Item = {item}\n");
        let (_, analysis) = hom_boundary_analysis(
            &[
                (
                    "Contracts",
                    "module App.Contracts exposing (Child, Container, Output)\ntype module Child =\n  type Item\ntype module Container =\n  module M : Child\n  value : M.Item\ntype module Output =\n  type Value\n  value : Value\n",
                ),
                ("Child", &child),
                (
                    "Parent",
                    "module App.Parent exposing (M, value)\nmodule M = App.Child\nvalue : String\nlet value = \"forty-two\"\n",
                ),
                (
                    "Make",
                    "module App.Make(K : App.Contracts.Container) : App.Contracts.Output with type Value = K.M.Item\ntype alias Value = K.M.Item\nvalue : Value\nlet value = K.value\n",
                ),
                (
                    "Root",
                    "module App.Root exposing (Chosen)\nmodule Chosen = App.Make(App.Parent)\n",
                ),
            ],
            "Root",
        );
        let errors: Vec<_> = analysis
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        if rejected {
            assert!(
                errors
                    .iter()
                    .any(|d| d.code == codes::T061 && d.message.contains("value")),
                "{errors:?}"
            );
        } else {
            assert!(errors.is_empty(), "{errors:?}");
            let chosen = &analysis
                .modules
                .iter()
                .find(|m| m.name == "App.Root")
                .expect("root")
                .interface
                .modules["Chosen"];
            assert!(chosen.find(Namespace::Value, "value").is_some());
        }
    }
}

#[cfg(feature = "ide")]
#[test]
fn hom_file_functor_accepts_an_indented_structure_body() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Output)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
            ),
            (
                "Make",
                "module App.Make(K : App.Contracts.Input) : App.Contracts.Output =\n  answer : Int\n  let answer = K.seed + 1\n",
            ),
            (
                "Root",
                "module App.Root exposing (Chosen)\nmodule Chosen = App.Make(App.Argument)\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_factory_members_retain_dependencies_on_sibling_modules() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Cell, Output, Namespace)\ntype module Input =\n  seed : Int\ntype module Cell =\n  type Item\ntype module Output =\n  type Value\n  value : Value\ntype module Namespace =\n  module M : Cell\n  module Build : (K : Input) -> Output with type Value = M.Item\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
            ),
            (
                "Child",
                "module App.Child exposing (Item, value)\ntype alias Item = String\nvalue : Item\nlet value = \"forty-two\"\n",
            ),
            (
                "Build",
                "module App.Build(K : App.Contracts.Input) : App.Contracts.Output with type Value = App.Child.Item\nimport App.Child\ntype alias Value = App.Child.Item\nvalue : Value\nlet value = App.Child.value\n",
            ),
            (
                "Parent",
                "module App.Parent exposing (M, Build)\nmodule M = App.Child\nmodule Build = App.Build\n",
            ),
            (
                "Adapter",
                "module App.Adapter(N : App.Contracts.Namespace)(K : App.Contracts.Input) : App.Contracts.Output with type Value = N.M.Item = N.Build(K)\n",
            ),
            (
                "Root",
                "module App.Root exposing (Chosen)\nmodule Chosen = App.Adapter(App.Parent)(App.Argument)\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_concrete_result_equalities_do_not_open_a_parameters_abstract_members() {
    use crate::diagnostics::codes;
    let (sources, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Child, Container, Output)\ntype module Child =\n  type Item\ntype module Container =\n  module M : Child\n  value : M.Item\ntype module Output =\n  value : String\n",
            ),
            (
                "Child",
                "module App.Child exposing (Item)\ntype alias Item = String\n",
            ),
            (
                "Parent",
                "module App.Parent exposing (M, value)\nmodule M = App.Child\nvalue : String\nlet value = \"forty-two\"\n",
            ),
            (
                "Make",
                "module App.Make(K : App.Contracts.Container) : App.Contracts.Output\nvalue : String\nlet value = K.value\n",
            ),
            (
                "Root",
                "module App.Root exposing (Chosen)\nmodule Chosen = App.Make(App.Parent)\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis.diagnostics.iter().any(|d| d.code == codes::T003
            && d.span
                .is_some_and(|s| sources.get(s.file).path == "/p/src/App/Make.fl")),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_factory_members_capture_the_structures_abstract_type() {
    use crate::diagnostics::{Severity, codes};
    for (item, rejected) in [("String", false), ("Int", true)] {
        let parent = format!(
            "module App.Parent exposing (Item, Build)\ntype alias Item = {item}\nmodule Build = App.Build\n"
        );
        let (_, analysis) = hom_boundary_analysis(
            &[
                (
                    "Contracts",
                    "module App.Contracts exposing (Input, Output, Space)\ntype module Input =\n  seed : Int\ntype module Output =\n  type Value\n  value : Value\ntype module Space =\n  type Item\n  module Build : (K : Input) -> Output with type Value = Item\n",
                ),
                (
                    "Argument",
                    "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
                ),
                (
                    "Build",
                    "module App.Build(K : App.Contracts.Input) : App.Contracts.Output with type Value = String\ntype alias Value = String\nvalue : Value\nlet value = show K.seed\n",
                ),
                ("Parent", &parent),
                (
                    "Adapter",
                    "module App.Adapter(N : App.Contracts.Space)(K : App.Contracts.Input) : App.Contracts.Output with type Value = N.Item = N.Build(K)\n",
                ),
                (
                    "Root",
                    "module App.Root exposing (Chosen)\nmodule Chosen = App.Adapter(App.Parent)(App.Argument)\n",
                ),
            ],
            "Root",
        );
        let errors: Vec<_> = analysis
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        if rejected {
            assert!(
                errors
                    .iter()
                    .any(|d| d.code == codes::T061 && d.message.contains("Build")),
                "{errors:?}"
            );
        } else {
            assert!(errors.is_empty(), "{errors:?}");
        }
    }
}

#[cfg(feature = "ide")]
#[test]
fn hom_nested_functors_capture_imports_without_widening_exposing() {
    use crate::diagnostics::{Severity, codes};
    for (import, input, output, expression, rejected) in [
        (
            "import App.Contracts as Contracts",
            "Contracts.Input",
            "Contracts.Output",
            "K.seed + 1",
            false,
        ),
        (
            "import App.Helpers exposing (offset)",
            "App.Contracts.Input",
            "App.Contracts.Output",
            "K.seed + offset",
            false,
        ),
        (
            "import App.Helpers exposing (..)",
            "App.Contracts.Input",
            "App.Contracts.Output",
            "K.seed + offset",
            false,
        ),
        (
            "import App.Helpers exposing (offset)",
            "App.Contracts.Input",
            "App.Contracts.Output",
            "K.seed + offset + hidden",
            true,
        ),
    ] {
        let root = format!(
            "module App.Root exposing (answer)\n{import}\nmodule Make(K : {input}) : {output} =\n  answer : Int\n  let answer = {expression}\nmodule Chosen = Make(App.Argument)\nanswer : Int\nlet answer = Chosen.answer\n"
        );
        let (_, analysis) = hom_boundary_analysis(
            &[
                (
                    "Contracts",
                    "module App.Contracts exposing (Input, Output)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\n",
                ),
                (
                    "Argument",
                    "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
                ),
                (
                    "Helpers",
                    "module App.Helpers exposing (offset, hidden)\noffset : Int\nlet offset = 1\nhidden : Int\nlet hidden = 99\n",
                ),
                ("Root", &root),
            ],
            "Root",
        );
        if rejected {
            assert!(
                analysis
                    .diagnostics
                    .iter()
                    .any(|d| d.code == codes::T001 && d.message.contains("hidden")),
                "{import}: {:?}",
                analysis.diagnostics
            );
        } else {
            assert!(
                analysis
                    .diagnostics
                    .iter()
                    .all(|d| d.severity != Severity::Error),
                "{import}: {:?}",
                analysis.diagnostics
            );
            assert!(
                analysis.diagnostics.is_empty(),
                "{import}: {:?}",
                analysis.diagnostics
            );
        }
    }
}

#[cfg(feature = "ide")]
#[test]
fn hom_inherited_import_warnings_belong_to_the_lexical_source() {
    use crate::diagnostics::codes;
    let (sources, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Output)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
            ),
            (
                "Helpers",
                "module App.Helpers exposing (offset, unused)\noffset : Int\nlet offset = 1\nunused : Int\nlet unused = 99\n",
            ),
            (
                "Root",
                "module App.Root exposing (answer)\nimport App.Helpers exposing (offset, unused)\nmodule Make(K : App.Contracts.Input) : App.Contracts.Output =\n  answer : Int\n  let answer = K.seed + offset\nmodule Chosen = Make(App.Argument)\nanswer : Int\nlet answer = Chosen.answer\n",
            ),
        ],
        "Root",
    );
    assert_eq!(analysis.diagnostics.len(), 1, "{:?}", analysis.diagnostics);
    let warning = &analysis.diagnostics[0];
    assert_eq!(warning.code, codes::R012);
    let span = warning.span.expect("source import span");
    assert!(sources.get(span.file).path.ends_with("/App/Root.fl"));
    assert_eq!(
        &sources.get(span.file).text[span.start as usize..span.end as usize],
        "unused"
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_parameter_alias_ascriptions_narrow_both_symbolic_and_concrete_views() {
    use crate::diagnostics::{Severity, codes};
    for (parameter, annotation, body, rejected) in [
        ("RichFactory", "NarrowFactory", "Built.answer", false),
        (
            "RichFactory",
            "NarrowFactory",
            "Built.answer + Original.extra",
            false,
        ),
        ("RichFactory", "NarrowFactory", "Built.extra", true),
        ("NarrowFactory", "RichFactory", "Built.extra", true),
    ] {
        let adapter = format!(
            "module App.Adapter(F : App.Contracts.{parameter})(K : App.Contracts.Input) : App.Contracts.Narrow\nmodule Narrow : App.Contracts.{annotation} = F\nmodule Built = Narrow(K)\nmodule Original = F(K)\nanswer : Int\nlet answer = {body}\n"
        );
        for root in ["Adapter", "Root"] {
            let (_, analysis) = hom_boundary_analysis(
                &[
                    (
                        "Contracts",
                        "module App.Contracts exposing (Input, Rich, Narrow, RichFactory, NarrowFactory)\ntype module Input =\n  seed : Int\ntype module Rich =\n  answer : Int\n  extra : Int\ntype module Narrow =\n  answer : Int\ntype module RichFactory = (K : Input) -> Rich\ntype module NarrowFactory = (K : Input) -> Narrow\n",
                    ),
                    (
                        "Argument",
                        "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
                    ),
                    (
                        "Make",
                        "module App.Make(K : App.Contracts.Input) : App.Contracts.Rich\nanswer : Int\nlet answer = K.seed + 1\nextra : Int\nlet extra = 99\n",
                    ),
                    ("Adapter", &adapter),
                    (
                        "Root",
                        "module App.Root exposing (answer)\nmodule Chosen = App.Adapter(App.Make)(App.Argument)\nanswer : Int\nlet answer = Chosen.answer\n",
                    ),
                ],
                root,
            );
            let errors: Vec<_> = analysis
                .diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .collect();
            if rejected {
                assert!(
                    errors.iter().any(|d| if parameter == "NarrowFactory" {
                        d.code == codes::T061 && d.message.contains("annotation")
                    } else {
                        matches!(d.code, codes::R004 | codes::T001) && d.message.contains("extra")
                    }),
                    "{parameter} as {annotation} ({body}): {errors:?}"
                );
            } else {
                assert!(
                    analysis.diagnostics.is_empty(),
                    "{parameter} as {annotation}: {:?}",
                    analysis.diagnostics
                );
            }
        }
    }
}

#[cfg(feature = "ide")]
#[test]
fn hom_applied_module_ascription_hides_undeclared_result_members() {
    use crate::diagnostics::codes;
    for (member, rejected) in [("answer", false), ("extra", true)] {
        let root = format!(
            "module App.Root exposing (answer)\nmodule Chosen : App.Contracts.Output = App.Make(App.Argument)\nanswer : Int\nlet answer = Chosen.{member}\n"
        );
        let (_, analysis) = hom_boundary_analysis(
            &[
                (
                    "Contracts",
                    "module App.Contracts exposing (Input, Output, Rich)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Rich =\n  answer : Int\n  extra : Int\n",
                ),
                (
                    "Argument",
                    "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
                ),
                (
                    "Make",
                    "module App.Make(K : App.Contracts.Input) : App.Contracts.Rich\nanswer : Int\nlet answer = K.seed + 1\nextra : Int\nlet extra = 99\n",
                ),
                ("Root", &root),
            ],
            "Root",
        );
        if rejected {
            assert!(
                analysis
                    .diagnostics
                    .iter()
                    .any(|d| d.code == codes::R004 && d.message.contains("extra")),
                "{:?}",
                analysis.diagnostics
            );
        } else {
            assert!(
                analysis.diagnostics.is_empty(),
                "{:?}",
                analysis.diagnostics
            );
        }
    }
}

#[cfg(feature = "ide")]
#[test]
fn hom_private_factories_infer_returned_functor_signatures() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input)\ntype module Input =\n  seed : Int\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
            ),
            (
                "Return",
                "private module App.Return(K : App.Contracts.Input) = functor (J : App.Contracts.Input) ->\n  answer : Int\n  let answer = K.seed + J.seed\n",
            ),
            (
                "Root",
                "module App.Root exposing (answer)\nmodule Selected = App.Return(App.Argument)\nmodule Chosen = Selected(App.Argument)\nanswer : Int\nlet answer = Chosen.answer\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_private_inference_retains_kinds_and_generative_nominal_members() {
    use crate::diagnostics::codes;
    for (member, declarations, use_source, incompatible) in [
        (
            "Box",
            "type Box a = Box a\nbox : Box Int\nlet box = Box K.seed",
            "box : Chosen.Box Int\nlet box = Chosen.box",
            false,
        ),
        (
            "Box",
            "type Box a = Box a\nbox : Box Int\nlet box = Box K.seed",
            "box : Other.Box Int\nlet box = Chosen.box",
            true,
        ),
        (
            "Equality(..), Clock",
            "class Equality a =\n  equal : a -> a -> Bool\nequality : Equality.Dict String\nlet equality = Equality.dictionary { equal = fun a b -> a == b }\nservice Clock =\n  now : Unit ->{} Int\nread : Unit ->{Clock} Int\nlet read () = Clock.now ()",
            "equality : Chosen.Equality.Dict String\nlet equality = Chosen.equality\nread : Unit ->{Chosen.Clock} Int\nlet read = Chosen.read",
            false,
        ),
        (
            "Equality(..), Clock",
            "class Equality a =\n  equal : a -> a -> Bool\nequality : Equality.Dict String\nlet equality = Equality.dictionary { equal = fun a b -> a == b }\nservice Clock =\n  now : Unit ->{} Int\nread : Unit ->{Clock} Int\nlet read () = Clock.now ()",
            "equality : Other.Equality.Dict String\nlet equality = Chosen.equality",
            true,
        ),
    ] {
        let exports = if member == "Box" {
            "Box, box, answer"
        } else {
            "Equality(..), Clock, equality, read, answer"
        };
        let factory = format!(
            "private module App.Factory(K : App.Contracts.Input) exposing ({exports})\n{declarations}\nanswer : Int\nlet answer = K.seed + 1\n"
        );
        let root = format!(
            "module App.Root exposing (..)\nmodule Chosen = App.Factory(App.Argument)\nmodule Other = App.Factory(App.Argument)\n{use_source}\n"
        );
        let (_, analysis) = hom_boundary_analysis(
            &[
                (
                    "Contracts",
                    "module App.Contracts exposing (Input)\ntype module Input =\n  seed : Int\n",
                ),
                (
                    "Argument",
                    "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
                ),
                ("Factory", &factory),
                ("Root", &root),
            ],
            "Root",
        );
        if incompatible {
            assert!(
                analysis.diagnostics.iter().any(|d| d.code == codes::T003),
                "{member}: {:?}",
                analysis.diagnostics
            );
        } else {
            assert!(
                analysis.diagnostics.is_empty(),
                "{member}: {:?}",
                analysis.diagnostics
            );
        }
    }
}

#[cfg(feature = "ide")]
#[test]
fn hom_private_inference_preserves_parameterized_manifest_equalities() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input)\ntype module Input =\n  seed : Int\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
            ),
            (
                "Factory",
                "private module App.Factory(K : App.Contracts.Input) exposing (Box, box)\ntype alias Box a = List a\nbox : Box Int\nlet box = [K.seed]\n",
            ),
            (
                "Root",
                "module App.Root exposing (box)\nmodule First = App.Factory(App.Argument)\nmodule Second = App.Factory(App.Argument)\nbox : Second.Box Int\nlet box = First.box\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_parameterized_manifest_members_compare_bodies_under_their_binders() {
    use crate::diagnostics::codes;
    for (body, value, rejected) in [("List x", "[]", false), ("Option x", "None", true)] {
        let make = format!(
            "module App.Make(K : App.Contracts.Input) : App.Contracts.Output\ntype alias Items x = {body}\nempty : forall x. Items x\nlet empty = {value}\n"
        );
        let (_, analysis) = hom_boundary_analysis(
            &[
                (
                    "Contracts",
                    "module App.Contracts exposing (Input, Output)\ntype module Input =\n  seed : Int\ntype module Output =\n  type Items a = List a\n  empty : forall a. Items a\n",
                ),
                (
                    "Argument",
                    "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
                ),
                ("Make", &make),
                (
                    "Root",
                    "module App.Root exposing (empty)\nmodule Chosen = App.Make(App.Argument)\nempty : List Int\nlet empty = Chosen.empty\n",
                ),
            ],
            "Root",
        );
        if rejected {
            assert!(
                analysis.diagnostics.iter().any(|d| d.code == codes::T061),
                "{:?}",
                analysis.diagnostics
            );
        } else {
            assert!(
                analysis.diagnostics.is_empty(),
                "{:?}",
                analysis.diagnostics
            );
        }
    }
}

#[cfg(feature = "ide")]
#[test]
fn hom_nested_functors_capture_earlier_local_signatures_types_and_values() {
    use crate::diagnostics::codes;
    for late in [false, true] {
        let offset = "offset : Int\nlet offset = 1\n";
        let root = format!(
            "module App.Root exposing (Chosen, Token)\nopaque type alias Token = Int\nwrap : Int -> Token\nlet wrap value = value\n{}type module Input =\n  seed : Int\ntype module Output =\n  type Value = Token\n  value : Value\nmodule Make(K : Input) : Output =\n  type alias Value = Token\n  value : Value\n  let value = wrap (K.seed + offset)\n{}module Chosen = Make(App.Argument)\n",
            if late { "" } else { offset },
            if late { offset } else { "" }
        );
        let (_, analysis) = hom_boundary_analysis(
            &[
                (
                    "Argument",
                    "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
                ),
                ("Root", &root),
            ],
            "Root",
        );
        if late {
            assert!(
                analysis
                    .diagnostics
                    .iter()
                    .any(|d| d.code == codes::T001 && d.message.contains("offset")),
                "{:?}",
                analysis.diagnostics
            );
        } else {
            assert!(
                analysis.diagnostics.is_empty(),
                "{:?}",
                analysis.diagnostics
            );
        }
    }
}

#[cfg(feature = "ide")]
#[test]
fn hom_nested_functors_capture_values_of_an_enclosing_generic_functor() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Output, Namespace)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Namespace =\n  module Make : Input -> Output\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
            ),
            (
                "Outer",
                "module App.Outer(K : App.Contracts.Input) : App.Contracts.Namespace\noffset : Int\nlet offset = K.seed\nmodule Make(J : App.Contracts.Input) : App.Contracts.Output =\n  answer : Int\n  let answer = offset + J.seed\n",
            ),
            (
                "Root",
                "module App.Root exposing (answer)\nmodule Namespace = App.Outer(App.Argument)\nmodule Chosen = Namespace.Make(App.Argument)\nanswer : Int\nlet answer = Chosen.answer\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn hom_nested_factories_capture_a_parameter_dictionary_in_preceding_values() {
    let (sources, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                r"module App.Contracts exposing (Policy, Output, Factory, Namespace)
type module Policy =
  type Key = String
  equality : Eq.Dict Key
  instance Eq Key from equality
type module Output =
  answer : Bool
type module Factory = (K : Policy) -> Output
type module Namespace =
  module Make : Factory
",
            ),
            (
                "Always",
                r"module App.Always exposing (Key, equality)
type alias Key = String
equality : Eq.Dict Key
let equality = Eq.dictionary { eq = fun _ _ -> true }
",
            ),
            (
                "Never",
                r"module App.Never exposing (Key, equality)
type alias Key = String
equality : Eq.Dict Key
let equality = Eq.dictionary { eq = fun _ _ -> false }
",
            ),
            (
                "Make",
                r#"module App.Make(A : App.Contracts.Policy)(B : App.Contracts.Policy) : App.Contracts.Output
answer : Bool
let answer = eq "x" "x"
"#,
            ),
            (
                "Outer",
                r#"module App.Outer(A : App.Contracts.Policy) : App.Contracts.Factory = functor (B : App.Contracts.Policy) ->
  answer : Bool
  let answer = eq "x" "x"
"#,
            ),
            (
                "Capture",
                r#"module App.Capture(A : App.Contracts.Policy) : App.Contracts.Namespace
prior : Bool
let prior = eq "x" "x"
module Make(B : App.Contracts.Policy) : App.Contracts.Output =
  answer : Bool
  let answer = if prior then eq "x" "x" else true
"#,
            ),
            (
                "Instances",
                r"module App.Instances exposing (Curried, Nested, Captured, CapturedOther)
module Curried = App.Make(App.Always)(App.Never)
module Nested = App.Outer(App.Always)(App.Never)
module First = App.Capture(App.Always)
module Second = App.Capture(App.Never)
module Captured = First.Make(App.Never)
module CapturedOther = Second.Make(App.Always)
",
            ),
            (
                "Main",
                r#"module App.Main exposing (main)
import App.Instances.Curried as Curried
import App.Instances.Nested as Nested
import App.Instances.Captured as Captured
import App.Instances.CapturedOther as CapturedOther
main : Unit ->{} Unit
let main () = Platform.stdout_line (show Curried.answer ++ " " ++ show Nested.answer ++ " " ++ show Captured.answer ++ " " ++ show CapturedOther.answer ++ " " ++ show (eq "x" "x"))
"#,
            ),
        ],
        "Main",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}\n{}",
        analysis.diagnostics,
        sources
            .files()
            .filter(|s| s.path.contains("First") || s.path.contains("Second"))
            .map(|s| format!("{}\n{}", s.path, s.text))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
