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

include!("checking_tests_hom_boundaries.rs");
