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

include!("checking_tests_hom_inference.rs");
