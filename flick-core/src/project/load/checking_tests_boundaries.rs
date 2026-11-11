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
