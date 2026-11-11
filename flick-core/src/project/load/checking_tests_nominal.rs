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
fn hom_file_parameterized_accepts_an_indented_structure_body() {
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
fn hom_nested_parameterized_modules_capture_imports_without_widening_exposing() {
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
