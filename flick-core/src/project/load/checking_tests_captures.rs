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
fn hom_private_factories_infer_returned_parameterized_signatures() {
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
                "private module App.Return(K : App.Contracts.Input) = module (J : App.Contracts.Input) ->\n  answer : Int\n  let answer = K.seed + J.seed\n",
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
fn hom_nested_parameterized_modules_capture_earlier_local_signatures_types_and_values() {
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
fn hom_nested_parameterized_modules_capture_values_of_an_enclosing_generic_parameterized() {
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
