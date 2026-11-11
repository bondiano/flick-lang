#[cfg(feature = "ide")]
#[test]
fn ported_review_inferred_nominals_retain_their_exported_methods() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Policy, Factory)\ntype module Input =\n  seed : Int\ntype module Policy =\n  class Equality a =\n    equal : a -> a -> Bool\ntype module Factory = (K : Input) -> Policy\n",
            ),
            (
                "Make",
                "private module App.Make(K : App.Contracts.Input) exposing (Equality(..))\nclass Equality a =\n  equal : a -> a -> Bool\n",
            ),
            (
                "Root",
                "module App.Root exposing (Selected)\nmodule Bind(F : App.Contracts.Factory) : App.Contracts.Factory = F\nmodule Selected = Bind(App.Make)\n",
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
fn ported_review_nested_parameter_contracts_supply_only_declared_dictionaries() {
    for required in [true, false] {
        let dictionary = if required {
            "  instance Eq Item from equality\n"
        } else {
            ""
        };
        let contracts = format!(
            "module App.Contracts exposing (Policy, Container, Output)\ntype module Policy =\n  type Item\n  item : Item\n  equality : Eq.Dict Item\n{dictionary}type module Container =\n  module Policy : Policy\ntype module Output =\n  answer : Bool\n"
        );
        let (_, analysis) = hom_boundary_analysis(
            &[
                ("Contracts", &contracts),
                (
                    "Root",
                    "module App.Root(N : App.Contracts.Container) : App.Contracts.Output\nanswer : Bool\nlet answer = eq N.Policy.item N.Policy.item\n",
                ),
            ],
            "Root",
        );
        if required {
            assert!(
                analysis.diagnostics.is_empty(),
                "{:?}",
                analysis.diagnostics
            );
        } else {
            assert!(
                analysis
                    .diagnostics
                    .iter()
                    .any(|d| d.message.contains("Eq")),
                "{:?}",
                analysis.diagnostics
            );
        }
    }
}

#[cfg(feature = "ide")]
#[test]
fn ported_review_parenthesized_signature_inputs_keep_import_references() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Output)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\n",
            ),
            (
                "Root",
                "module App.Root exposing (Factory)\nimport App.Contracts as C\ntype module Factory = (C.Input) -> C.Output\n",
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
fn ported_review_refinement_paths_are_not_expression_dependencies() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Cell, Namespace)\ntype module Cell =\n  type Key\ntype module Namespace =\n  module Child : Cell\n",
            ),
            (
                "Child",
                "module App.Child exposing (Key)\ntype alias Key = Int\n",
            ),
            (
                "Library",
                "module App.Library exposing (Child)\nmodule Child = App.Child\n",
            ),
            (
                "Root",
                "module App.Root exposing (Narrow)\nmodule Narrow = (App.Library : App.Contracts.Namespace with type Child.Key = Int)\n",
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
fn ported_review_module_signature_cycles_are_rejected() {
    for source in [
        "module App.Root exposing (A, B)\ntype module A = B\ntype module B = A\n",
        "module App.Root exposing (A, B)\ntype module A =\n  module Child : B\ntype module B =\n  module Child : A\n",
    ] {
        let (_, analysis) = hom_boundary_analysis(&[("Root", source)], "Root");
        assert!(
            analysis.diagnostics.iter().any(|d| d.code == codes::R007),
            "{:?}",
            analysis.diagnostics
        );
    }
}

#[cfg(feature = "ide")]
#[test]
fn ported_review_nested_alias_ancestors_are_checked_before_member_applications() {
    for member in ["answer", "extra"] {
        let root = format!(
            "module App.Root exposing (answer)\nmodule Chosen = App.Parent.Inner.Make(App.Argument)\nanswer : Int\nlet answer = Chosen.{member}\n"
        );
        let (_, analysis) = hom_boundary_analysis(
            &[
                (
                    "Contracts",
                    "module App.Contracts exposing (Input, Output, Rich, Factory, Space, Outer)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Rich =\n  answer : Int\n  extra : Int\ntype module Factory = (K : Input) -> Output\ntype module Space =\n  module Make : Factory\ntype module Outer =\n  module Inner : Space\n",
                ),
                (
                    "Argument",
                    "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
                ),
                (
                    "Implementation",
                    "module App.Implementation exposing (Make)\nmodule Make(K : App.Contracts.Input) : App.Contracts.Rich =\n  answer : Int\n  let answer = K.seed + 1\n  extra : Int\n  let extra = 99\n",
                ),
                (
                    "Parent",
                    "module App.Parent : App.Contracts.Outer exposing (Inner)\nmodule Inner = App.Implementation\n",
                ),
                ("Root", &root),
            ],
            "Root",
        );
        if member == "answer" {
            assert!(
                analysis.diagnostics.is_empty(),
                "{:?}",
                analysis.diagnostics
            );
        } else {
            assert!(
                analysis
                    .diagnostics
                    .iter()
                    .any(|d| matches!(d.code, codes::R004 | codes::T001)
                        && d.message.contains("extra")),
                "{:?}",
                analysis.diagnostics
            );
        }
    }
}
#[cfg(feature = "ide")]
#[test]
fn caller_parameter_names_do_not_shadow_factory_imports() {
    for member in ["answer", "extra"] {
        let root = format!(
            "module App.Root exposing (answer)\nmodule Applied = App.Apply(App.Make)\nmodule Built = Applied(App.Argument)\nanswer : Int\nlet answer = Built.{member}\n"
        );
        let (_, analysis) = hom_boundary_analysis(
            &[
                (
                    "Contracts",
                    "module App.Contracts exposing (Input, Output, Rich, Factory)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Rich =\n  answer : Int\n  extra : Int\ntype module Factory = (K : Input) -> Output\n",
                ),
                (
                    "Helpers",
                    "module App.Helpers exposing (offset)\noffset : Int\nlet offset = 1\n",
                ),
                (
                    "Argument",
                    "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
                ),
                (
                    "Make",
                    "module App.Make(K : App.Contracts.Input) : App.Contracts.Rich exposing (answer, extra)\nimport App.Helpers as F\nanswer : Int\nlet answer = K.seed + F.offset\nextra : Int\nlet extra = 99\n",
                ),
                (
                    "Apply",
                    "module App.Apply(F : App.Contracts.Factory)(K : App.Contracts.Input) : App.Contracts.Output = F(K)\n",
                ),
                ("Root", &root),
            ],
            "Root",
        );
        if member == "answer" {
            assert!(
                analysis.diagnostics.is_empty(),
                "{:?}",
                analysis.diagnostics
            );
        } else {
            assert!(
                analysis.diagnostics.iter().any(|d| {
                    matches!(d.code, codes::R004 | codes::T001) && d.message.contains("extra")
                }),
                "{:?}",
                analysis.diagnostics
            );
        }
    }
}
