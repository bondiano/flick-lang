#[cfg(feature = "ide")]
#[test]
fn cyclic_contract_equalities_are_rejected_without_expanding_their_bodies() {
    let (_, analysis) = hom_boundary_analysis(
        &[(
            "Contracts",
            "module App.Contracts exposing (Input)\ntype module Input =\n  type T = (T, T)\n  type A\n  type B\n  type C\n  type D\n",
        )],
        "Contracts",
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == crate::diagnostics::codes::R007),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn finite_higher_order_callbacks_can_reenter_a_template_with_different_arguments() {
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
                "Identity",
                "module App.Identity(K : App.Contracts.Input) : App.Contracts.Output\nanswer : Int\nlet answer = K.seed\n",
            ),
            (
                "Apply",
                "module App.Apply(F : App.Contracts.Factory, K : App.Contracts.Input) : App.Contracts.Output = F(K)\n",
            ),
            (
                "Bounce",
                "module App.Bounce(K : App.Contracts.Input) : App.Contracts.Output = App.Apply(App.Identity, K)\n",
            ),
            (
                "Root",
                "module App.Root exposing (Chosen)\nmodule Chosen = App.Apply(App.Bounce, App.Argument)\n",
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
fn recursive_static_expansions_with_changing_arguments_are_bounded() {
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
                "Loop",
                "module App.Loop(K : App.Contracts.Input) : App.Contracts.Output = App.Loop((functor (X : App.Contracts.Input) ->\n  seed : Int\n  let seed = X.seed\n)(K))\n",
            ),
            (
                "Root",
                "module App.Root exposing (Chosen)\nmodule Chosen = App.Loop(App.Argument)\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == crate::diagnostics::codes::R006),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn static_arguments_resolve_aliases_once_before_using_canonical_module_identities() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Output)\ntype module Input =\n  identity : a -> a\ntype module Output =\n  answer : Int\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nidentity : a -> a\nlet seed = 41\n",
            ),
            (
                "Other",
                "module App.Other exposing (unrelated)\nunrelated : String\nlet unrelated = \"other\"\n",
            ),
            (
                "Make",
                "module App.Make(K : App.Contracts.Input) : App.Contracts.Output\nanswer : Int\nlet answer = K.identity 41\n",
            ),
            (
                "Root",
                "module App.Root exposing (Chosen, other)\nimport Prelude as Actual\nimport App.Other as Prelude\nmodule Chosen = App.Make(Actual)\nother : String\nlet other = Prelude.unrelated\n",
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
