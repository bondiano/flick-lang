#[cfg(feature = "ide")]
use crate::diagnostics::Severity;

#[cfg(feature = "ide")]
#[test]
fn changing_an_ancestor_namespace_invalidates_cached_callee_views() {
    let files = [
        (
            "Contracts",
            "module App.Contracts exposing (..)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Rich =\n  answer : Int\n  secret : Int\ntype module FactoriesWide =\n  module Make : (K : Input) -> Rich\ntype module FactoriesNarrow =\n  module Make : (K : Input) -> Output\ntype module Wide =\n  module Alias : FactoriesWide\ntype module Narrow =\n  module Alias : FactoriesNarrow\n",
        ),
        (
            "Argument",
            "module App.Argument exposing (seed)\nseed : Int\nlet seed = 1\n",
        ),
        (
            "Make",
            "module App.Make(K : App.Contracts.Input) : App.Contracts.Rich\nanswer : Int\nlet answer = K.seed\nsecret : Int\nlet secret = 42\n",
        ),
        (
            "Factories",
            "module App.Factories exposing (Make)\nmodule Make = App.Make\n",
        ),
        (
            "Parent",
            "module App.Parent : App.Contracts.Wide\nmodule Alias = App.Factories\n",
        ),
        (
            "Root",
            "module App.Root exposing (secret)\nmodule Chosen = App.Parent.Alias.Make(App.Argument)\nsecret : Int\nlet secret = Chosen.secret\n",
        ),
    ];
    let mut host = Memory(
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
    let mut cache = ModuleCache::default();
    let roots = [PathBuf::from("/p/src/App/Root.fl")];
    let first = analyze_with(&host, &mut sources, Some(&project), &roots, &mut cache);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    host.0.insert(
        PathBuf::from("/p/src/App/Parent.fl"),
        files[4].1.replace("Contracts.Wide", "Contracts.Narrow"),
    );
    let warm = analyze_with(&host, &mut sources, Some(&project), &roots, &mut cache);
    let cold = analyze(&host, &mut sources, Some(&project), &roots);
    let messages = |analysis: &super::Analysis| {
        analysis
            .diagnostics
            .iter()
            .map(|d| (d.code, d.message.clone()))
            .collect::<Vec<_>>()
    };
    assert!(
        cold.diagnostics
            .iter()
            .any(|d| d.code == codes::R004 && d.message.contains("secret")),
        "{:?}",
        cold.diagnostics
    );
    assert_eq!(messages(&warm), messages(&cold));
}

#[cfg(feature = "ide")]
#[test]
fn changing_a_namespace_ascription_invalidates_cached_argument_views() {
    let files = [
        (
            "Contracts",
            "module App.Contracts exposing (Small, Rich, Output, Wide, Narrow)\ntype module Small =\n  seed : Int\ntype module Rich =\n  seed : Int\n  secret : Int\ntype module Output =\n  answer : Int\ntype module Wide =\n  module Visible : Rich\ntype module Narrow =\n  module Visible : Small\n",
        ),
        (
            "Argument",
            "module App.Argument exposing (seed, secret)\nseed : Int\nlet seed = 1\nsecret : Int\nlet secret = 42\n",
        ),
        (
            "Make",
            "module App.Make(K : App.Contracts.Rich) : App.Contracts.Output\nanswer : Int\nlet answer = K.secret\n",
        ),
        (
            "Parent",
            "module App.Parent : App.Contracts.Wide\nmodule Visible = App.Argument\n",
        ),
        (
            "Root",
            "module App.Root exposing (answer)\nmodule Chosen = App.Make(App.Parent.Visible)\nanswer : Int\nlet answer = Chosen.answer\n",
        ),
    ];
    let mut host = Memory(
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
    let mut cache = ModuleCache::default();
    let roots = [PathBuf::from("/p/src/App/Root.fl")];
    let first = analyze_with(&host, &mut sources, Some(&project), &roots, &mut cache);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    host.0.insert(
        PathBuf::from("/p/src/App/Parent.fl"),
        files[3].1.replace("Contracts.Wide", "Contracts.Narrow"),
    );
    let warm = analyze_with(&host, &mut sources, Some(&project), &roots, &mut cache);
    let cold = analyze(&host, &mut sources, Some(&project), &roots);
    let codes = |analysis: &super::Analysis| {
        analysis
            .diagnostics
            .iter()
            .map(|d| (d.code, d.message.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(codes(&warm), codes(&cold));
    assert!(
        warm.diagnostics
            .iter()
            .any(|d| d.code == codes::T060 && d.message.contains("secret")),
        "{:?}",
        warm.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn namespace_parent_cycles_report_the_original_nested_import_span() {
    let (sources, analysis) = hom_boundary_analysis(
        &[
            (
                "A",
                "module App.A exposing (value)\nimport App.B.Child exposing (identity)\nvalue : Int\nlet value = identity 1\n",
            ),
            (
                "B",
                "module App.B exposing (Child)\nimport App.A exposing (value)\nmodule Child = Prelude\n",
            ),
        ],
        "A",
    );
    let diagnostic = analysis
        .diagnostics
        .iter()
        .find(|d| d.code == codes::R006)
        .expect("cycle diagnostic");
    let span = diagnostic.span.expect("import span");
    assert!(
        sources.get(span.file).text[span.start as usize..span.end as usize].contains("App.B.Child")
    );
}

#[cfg(feature = "ide")]
#[test]
fn inline_helpers_do_not_inherit_the_enclosing_factory_result_contract() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Output)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 42\n",
            ),
            (
                "Make",
                "module App.Make(K : App.Contracts.Input) : App.Contracts.Output\nmodule Helper exposing (value) =\n  value : Int\n  let value = K.seed\nanswer : Int\nlet answer = Helper.value\n",
            ),
            (
                "Root",
                "module App.Root exposing (answer)\nmodule Chosen = App.Make(App.Argument)\nanswer : Int\nlet answer = Chosen.answer\n",
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
fn module_arguments_keep_the_callers_view_even_inside_the_argument_owner() {
    for callee in ["App.Make", "App.Parent.Make"] {
        let root = format!(
            "module App.Root exposing (answer)\nmodule Chosen = {callee}(App.Parent.Visible)\nanswer : Int\nlet answer = Chosen.answer\n"
        );
        let (_, analysis) = hom_boundary_analysis(
            &[
                (
                    "Contracts",
                    "module App.Contracts exposing (Small, Rich, Output, Namespace)\ntype module Small =\n  seed : Int\ntype module Rich =\n  seed : Int\n  secret : Int\ntype module Output =\n  answer : Int\ntype module Namespace =\n  module Visible : Small\n  module Make : (K : Rich) -> Output\n",
                ),
                (
                    "Argument",
                    "module App.Argument exposing (seed, secret)\nseed : Int\nlet seed = 1\nsecret : Int\nlet secret = 42\n",
                ),
                (
                    "Make",
                    "module App.Make(K : App.Contracts.Rich) : App.Contracts.Output\nanswer : Int\nlet answer = K.secret\n",
                ),
                (
                    "Parent",
                    "module App.Parent : App.Contracts.Namespace\nmodule Visible = App.Argument\nmodule Make(K : App.Contracts.Rich) : App.Contracts.Output =\n  answer : Int\n  let answer = K.secret\n",
                ),
                ("Root", &root),
            ],
            "Root",
        );
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|d| d.code == codes::T060 && d.message.contains("secret")),
            "{callee}: {:?}",
            analysis.diagnostics
        );
    }
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Rich, Output)\ntype module Rich =\n  secret : Int\ntype module Output =\n  answer : Int\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (secret)\nsecret : Int\nlet secret = 42\n",
            ),
            (
                "Make",
                "module App.Make(K : App.Contracts.Rich) : App.Contracts.Output\nanswer : Int\nlet answer = K.secret\n",
            ),
            (
                "Parent",
                "module App.Parent : App.Contracts.Output\nmodule Private = App.Argument\nmodule Chosen = App.Make(Private)\nanswer : Int\nlet answer = Chosen.answer\n",
            ),
        ],
        "Parent",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "caller may pass its own private member: {:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn parameterized_manifest_aliases_match_through_higher_order_dependent_results() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Output, Factory)\ntype module Input =\n  type Items a = List a\ntype module Output =\n  type Value\n  value : Value\ntype module Factory = (K : Input) -> Output with type Value = K.Items Int\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (Items)\ntype alias Items a = List a\n",
            ),
            (
                "Make",
                "module App.Make(K : App.Contracts.Input) : App.Contracts.Output with type Value = K.Items Int\ntype alias Value = K.Items Int\nvalue : Value\nlet value = []\n",
            ),
            (
                "Apply",
                "module App.Apply(F : App.Contracts.Factory, K : App.Contracts.Input) : App.Contracts.Output with type Value = K.Items Int = F(K)\n",
            ),
            (
                "Root",
                "module App.Root exposing (value)\nmodule Chosen = App.Apply(App.Make, App.Argument)\nvalue : List Int\nlet value = Chosen.value\n",
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
fn structure_aliases_materialize_their_advertised_factory_members() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Output)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 42\n",
            ),
            (
                "Parent",
                "module App.Parent exposing (Make)\nmodule Make(K : App.Contracts.Input) : App.Contracts.Output =\n  answer : Int\n  let answer = K.seed\n",
            ),
            (
                "Root",
                "module App.Root exposing (answer)\nmodule Alias = App.Parent\nmodule Chosen = Alias.Make(App.Argument)\nanswer : Int\nlet answer = Chosen.answer\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Output, Namespace)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Namespace =\n  module Make : (K : Input) -> Output\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 42\n",
            ),
            (
                "Parent",
                "module App.Parent exposing (Make, Hidden)\nmodule Make(K : App.Contracts.Input) : App.Contracts.Output =\n  answer : Int\n  let answer = K.seed\nmodule Hidden = Make\n",
            ),
            (
                "Root",
                "module App.Root exposing (answer)\nmodule Alias : App.Contracts.Namespace = App.Parent\nmodule Chosen = Alias.Hidden(App.Argument)\nanswer : Int\nlet answer = Chosen.answer\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == codes::R018 && d.message.contains("Hidden")),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn nominal_contract_targets_inside_factories_are_not_module_dependencies() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Output)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\n",
            ),
            (
                "Argument",
                "module App.Argument exposing (seed)\nseed : Int\nlet seed = 42\n",
            ),
            (
                "Parent",
                "module App.Parent exposing (Make)\nmodule Make(K : App.Contracts.Input) : App.Contracts.Output =\n  type module Local =\n    class Equality = Prelude.Eq\n  answer : Int\n  let answer = K.seed\n",
            ),
            (
                "Root",
                "module App.Root exposing (answer)\nmodule Chosen = App.Parent.Make(App.Argument)\nanswer : Int\nlet answer = Chosen.answer\n",
            ),
        ],
        "Root",
    );
    assert!(
        !analysis
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn factory_local_imports_shadow_enclosing_imports_during_resolution() {
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
            (
                "Outer",
                "module App.Outer exposing (value)\nvalue : Int\nlet value = 1\n",
            ),
            (
                "Inner",
                "module App.Inner exposing (answer)\nanswer : Int\nlet answer = 41\n",
            ),
            (
                "Parent",
                "module App.Parent exposing (Make, outer)\nimport App.Outer as N\nouter : Int\nlet outer = N.value\nmodule Make(K : App.Contracts.Input) : App.Contracts.Output =\n  import App.Inner as N\n  answer : Int\n  let answer = N.answer + K.seed\n",
            ),
            (
                "Root",
                "module App.Root exposing (answer)\nmodule Chosen = App.Parent.Make(App.Argument)\nanswer : Int\nlet answer = Chosen.answer\n",
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
fn dependent_signature_binders_shadow_imported_module_qualifiers() {
    let (_, analysis) = hom_boundary_analysis(
        &[(
            "Contracts",
            "module App.Contracts exposing (..)\nimport String as K\nmarker : String\nlet marker = K.trim \"\"\ntype module Input =\n  type Key\ntype module Output =\n  type Value\ntype module Factory = (K : Input) -> Output with type Value = K.Key\n",
        )],
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
fn packing_nested_static_namespaces_reports_the_unsupported_runtime_boundary() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Inner, Outer)\ntype module Inner =\n  value : Int\ntype module Outer =\n  module Child : Inner\n",
            ),
            (
                "Child",
                "module App.Child exposing (value)\nvalue : Int\nlet value = 42\n",
            ),
            (
                "Parent",
                "module App.Parent exposing (Child)\nmodule Child = App.Child\n",
            ),
            (
                "Root",
                "module App.Root exposing (packed)\nimport App.Parent as Parent\nimport App.Contracts as C\npacked : Module C.Outer\nlet packed = pack Parent as C.Outer\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis.diagnostics.iter().any(|d| d.code == codes::T068),
        "{:?}",
        analysis.diagnostics
    );
}
