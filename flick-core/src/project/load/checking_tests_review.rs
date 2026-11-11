#[cfg(feature = "ide")]
use crate::diagnostics::codes;

#[cfg(feature = "ide")]
#[test]
fn review_manifest_type_parameters_are_applied_before_expansion() {
    let (_, analysis) = hom_boundary_analysis(
        &[(
            "Root",
            "module App.Root exposing (Factory, Selected)\ntype module Input =\n  type Items a = List a\ntype module Output =\n  type Value\ntype module Factory = (K : Input) -> Output with type Value = K.Items Int\nmodule Make(K : Input) : Output with type Value = List Int =\n  type alias Value = List Int\nmodule Bind(F : Factory) : Factory = F\nmodule Selected = Bind(Make)\n",
        )],
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
fn review_alias_materialization_keeps_canonical_paths() {
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
                "Other",
                "module App.Other exposing (seed)\nseed : Int\nlet seed = 0\n",
            ),
            (
                "Factory",
                "module App.Factory(K : App.Contracts.Input) : App.Contracts.Output\nanswer : Int\nlet answer = K.seed + 1\n",
            ),
            (
                "Root",
                "module App.Root exposing (answer)\nimport App.Factory as Factory\nimport App.Argument as Actual\nimport App.Other as App\nmodule Alias = Actual\nmodule Chosen = Factory(Alias)\nanswer : Int\nlet answer = Chosen.answer + App.seed\n",
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
fn review_structure_aliases_and_returned_namespaces_expose_factory_members() {
    for selected in ["App.Parent", "App.Return(App.Argument)"] {
        let root = format!(
            "module App.Root exposing (answer)\nmodule Alias = {selected}\nmodule Chosen = Alias.Make(App.Argument)\nanswer : Int\nlet answer = Chosen.answer\n"
        );
        let (_, analysis) = hom_boundary_analysis(
            &[
                (
                    "Contracts",
                    "module App.Contracts exposing (Input, Output, Factory, Namespace)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Factory = (K : Input) -> Output\ntype module Namespace =\n  module Make : Factory\n",
                ),
                (
                    "Argument",
                    "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
                ),
                (
                    "Parent",
                    "module App.Parent exposing (Make)\nmodule Make(K : App.Contracts.Input) : App.Contracts.Output =\n  answer : Int\n  let answer = K.seed + 1\n",
                ),
                (
                    "Return",
                    "module App.Return(K : App.Contracts.Input) : App.Contracts.Namespace = App.Parent\n",
                ),
                ("Root", &root),
            ],
            "Root",
        );
        assert!(
            analysis.diagnostics.is_empty(),
            "{selected}: {:?}",
            analysis.diagnostics
        );
    }
}

#[cfg(feature = "ide")]
#[test]
fn review_structure_alias_members_keep_the_parents_result_boundary() {
    for member in ["answer", "extra"] {
        let root = format!(
            "module App.Root exposing (answer)\nmodule Alias = App.Parent\nmodule Chosen = Alias.Make(App.Argument)\nanswer : Int\nlet answer = Chosen.{member}\n"
        );
        let (_, analysis) = hom_boundary_analysis(
            &[
                (
                    "Contracts",
                    "module App.Contracts exposing (Input, Output, Rich, Factory, Namespace)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Rich =\n  answer : Int\n  extra : Int\ntype module Factory = (K : Input) -> Output\ntype module Namespace =\n  module Make : Factory\n",
                ),
                (
                    "Argument",
                    "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n",
                ),
                (
                    "Parent",
                    "module App.Parent : App.Contracts.Namespace exposing (Make)\nmodule Make(K : App.Contracts.Input) : App.Contracts.Rich =\n  answer : Int\n  let answer = K.seed + 1\n  extra : Int\n  let extra = 99\n",
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
fn review_parameter_applications_keep_the_callers_result_boundary() {
    for member in ["answer", "extra"] {
        let root = format!(
            "module App.Root exposing (answer)\nmodule Chosen = App.Apply(App.Rich)(App.Argument)\nanswer : Int\nlet answer = Chosen.{member}\n"
        );
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
                    "Rich",
                    "private module App.Rich(K : App.Contracts.Input) exposing (answer, extra)\nanswer : Int\nlet answer = K.seed + 1\nextra : Int\nlet extra = 99\n",
                ),
                (
                    "Apply",
                    "private module App.Apply(F : App.Contracts.Factory)(K : App.Contracts.Input) = F(K)\n",
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
                    .any(|d| matches!(d.code, codes::R004 | codes::T001)),
                "the parameter must hide extra: {:?}",
                analysis.diagnostics
            );
        }
    }
}

#[cfg(feature = "ide")]
#[test]
fn review_inline_prefixes_retain_private_declarations_before_ascription() {
    let (_, analysis) = hom_boundary_analysis(
        &[(
            "Root",
            "module App.Root : Output exposing (answer)\ntype module Output =\n  answer : Int\noffset : Int\nlet offset = 41\nmodule Child exposing (answer) =\n  answer : Int\n  let answer = offset + 1\nanswer : Int\nlet answer = Child.answer\n",
        )],
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
fn review_signature_binders_shadow_import_aliases() {
    let (_, analysis) = hom_boundary_analysis(
        &[(
            "Root",
            "module App.Root exposing (Factory)\nimport Int as K\ntype module Input =\n  type Key\ntype module Output =\n  type Value\ntype module Factory = (K : Input) -> Output with type Value = K.Key\n",
        )],
        "Root",
    );
    assert!(
        analysis.diagnostics.iter().all(|d| d.code == codes::R012),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn review_header_signatures_resolve_nested_import_aliases() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Library",
                "module App.Library exposing (Types)\nmodule Types = App.Types\n",
            ),
            (
                "Types",
                "module App.Types exposing (Input)\ntype module Input =\n  seed : Int\n",
            ),
            (
                "Root",
                "module App.Root(K : N.Types.Input) : Output\nimport App.Library as N\ntype module Output =\n  answer : Int\nanswer : Int\nlet answer = K.seed\n",
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
fn review_inferred_results_preserve_external_type_identity() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Output, Factory)\ntype module Input =\n  seed : Int\ntype module Output =\n  type T\n  value : T\ntype module Factory = (K : Input) -> Output\n",
            ),
            (
                "External",
                "module App.External exposing (T, value)\ntype T = T\nvalue : T\nlet value = T\n",
            ),
            (
                "Factory",
                "private module App.Factory(K : App.Contracts.Input) exposing (T, value)\nimport App.External exposing (T, value)\n",
            ),
            (
                "Bind",
                "module App.Bind(F : App.Contracts.Factory) : App.Contracts.Factory = F\n",
            ),
            (
                "Root",
                "module App.Root exposing (Selected)\nmodule Selected = App.Bind(App.Factory)\n",
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
fn review_inferred_results_preserve_finite_kind_declarations() {
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
                "private module App.Factory(K : App.Contracts.Input) exposing (Mode(..))\nkind Mode = Fast | Slow\n",
            ),
            (
                "Root",
                "module App.Root exposing (Chosen)\nmodule Chosen = App.Factory(App.Argument)\n",
            ),
        ],
        "Root",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let chosen = analysis.module("App.Root.Chosen").expect("chosen");
    for name in ["Mode", "Fast", "Slow"] {
        assert!(
            chosen.interface.find(Namespace::Type, name).is_some(),
            "{name}"
        );
    }
}

#[cfg(feature = "ide")]
#[test]
fn review_local_nominal_requirements_keep_finished_definitions() {
    let (_, analysis) = hom_boundary_analysis(
        &[(
            "Root",
            r#"module App.Root exposing (Make)
class Equality a =
  equal : a -> a -> Bool
service Clock =
  now : Unit ->{} Int
type module Policy =
  class Equality = Equality
  service Clock = Clock
  equality : Equality.Dict String
  instance Equality String from equality
type module Output =
  answer : Bool
  read : Unit ->{Clock} Int
module Make(K : Policy) : Output =
  answer : Bool
  let answer = K.equal "x" "x"
  read : Unit ->{Clock} Int
  let read () = K.Clock.now ()
"#,
        )],
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
fn review_nested_runtime_module_fields_can_be_packed_and_opened() {
    let (_, analysis) = hom_boundary_analysis(
        &[
            (
                "Contracts",
                "module App.Contracts exposing (Input, Outer)\ntype module Input =\n  value : Int\ntype module Outer =\n  module Child : Input\n",
            ),
            (
                "Child",
                "module App.Child exposing (value)\nvalue : Int\nlet value = 42\n",
            ),
            (
                "Namespace",
                "module App.Namespace exposing (Child)\nmodule Child = App.Child\n",
            ),
            (
                "Root",
                "module App.Root exposing (answer)\nimport App.Contracts as C\nimport App.Namespace\npacked : Module C.Outer\nlet packed = pack App.Namespace as C.Outer\nanswer : Int\nlet answer = unpack packed as K in K.Child.value\n",
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
fn review_inferred_kind_parameters_keep_their_local_kind_identity() {
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
                "private module App.Factory(K : App.Contracts.Input) exposing (Mode(..), Box, box, identity)\nkind Mode = Fast | Slow\ntype Box (mode : Mode) = Box\nbox : Box Fast\nlet box = Box\nidentity : forall mode. Box mode -> Box mode\nlet identity value = value\n",
            ),
            (
                "Root",
                "module App.Root exposing (Chosen)\nmodule Chosen = App.Factory(App.Argument)\n",
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
fn review_changing_an_ancestor_signature_invalidates_alias_member_applications() {
    let host = |result: &str| {
        Memory(BTreeMap::from([
        (PathBuf::from("/p/src/App/Contracts.fl"), "module App.Contracts exposing (Input, Output, Rich, Factory, RichFactory, Namespace, RichNamespace)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Rich =\n  answer : Int\n  extra : Int\ntype module Factory = (K : Input) -> Output\ntype module RichFactory = (K : Input) -> Rich\ntype module Namespace =\n  module Make : Factory\ntype module RichNamespace =\n  module Make : RichFactory\n".into()),
        (PathBuf::from("/p/src/App/Argument.fl"), "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n".into()),
        (PathBuf::from("/p/src/App/Make.fl"), "module App.Make(K : App.Contracts.Input) : App.Contracts.Rich\nanswer : Int\nlet answer = K.seed + 1\nextra : Int\nlet extra = 99\n".into()),
        (PathBuf::from("/p/src/App/Parent.fl"), format!("module App.Parent : App.Contracts.{result} exposing (Make)\nmodule Make = App.Make\n")),
        (PathBuf::from("/p/src/App/Root.fl"), "module App.Root exposing (answer)\nmodule Alias = App.Parent\nmodule Chosen = Alias.Make(App.Argument)\nanswer : Int\nlet answer = Chosen.extra\n".into()),
    ]))
    };
    let roots = [PathBuf::from("/p/src/App/Root.fl")];
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let mut cache = ModuleCache::default();
    let first = analyze_with(
        &host("RichNamespace"),
        &mut sources,
        Some(&project),
        &roots,
        &mut cache,
    );
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let changed = analyze_with(
        &host("Namespace"),
        &mut sources,
        Some(&project),
        &roots,
        &mut cache,
    );
    assert!(
        changed
            .diagnostics
            .iter()
            .any(|d| matches!(d.code, codes::R004 | codes::T001) && d.message.contains("extra")),
        "{:?}",
        changed.diagnostics
    );
}
