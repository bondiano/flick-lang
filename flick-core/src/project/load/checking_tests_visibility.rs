fn host(base: &str) -> Memory {
    Memory(BTreeMap::from([
        (PathBuf::from("/p/src/A/Base.fl"), base.to_owned()),
        (PathBuf::from("/p/src/A/Use.fl"), USE.to_owned()),
    ]))
}

#[cfg(feature = "ide")]
#[test]
fn private_file_module_is_available_to_its_own_package() {
    let (modules, diagnostics) = analyzed(
        &host("private module A.Base exposing (x)\nx : Float\nlet x = 1\n"),
        None,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(modules.contains_key("A.Use"));
}

#[cfg(feature = "ide")]
#[test]
fn another_package_cannot_import_a_private_file_module() {
    use crate::diagnostics::codes;
    use crate::source::{FileId, Span};

    let host = Memory(BTreeMap::from([
        (
            PathBuf::from("/p/src/App/Main.fl"),
            "module App.Main exposing (x)\nimport Lib.Hidden exposing (x)\n".into(),
        ),
        (
            PathBuf::from("/lib/flick.toml"),
            "schema-version = 1\n[project]\nkind = \"lib\"\n".into(),
        ),
        (
            PathBuf::from("/lib/src/Lib/Hidden.fl"),
            "private module Lib.Hidden exposing (x)\nx : Int\nlet x = 1\n".into(),
        ),
    ]));
    let mut sources = SourceMap::default();
    let mut project = library(&mut sources);
    project.config.dependencies.push(Dependency {
        name: "lib".into(),
        source: Source::Path("../lib".into()),
        span: Span::new(FileId::default(), 0, 0),
    });
    let analysis = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Main.fl")],
    );
    assert!(
        analysis.diagnostics.iter().any(|d| d.code == codes::R017),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn a_public_facade_reexports_from_a_private_module() {
    use crate::source::{FileId, Span};

    let host = Memory(BTreeMap::from([
        (
            PathBuf::from("/p/src/App/Main.fl"),
            "module App.Main exposing (x)\nimport Lib.Facade exposing (x)\n".into(),
        ),
        (
            PathBuf::from("/lib/flick.toml"),
            "schema-version = 1\n[project]\nkind = \"lib\"\n".into(),
        ),
        (
            PathBuf::from("/lib/src/Lib/Facade.fl"),
            "module Lib.Facade exposing (x)\nimport Lib.Hidden exposing (x)\n".into(),
        ),
        (
            PathBuf::from("/lib/src/Lib/Hidden.fl"),
            "private module Lib.Hidden exposing (x)\nx : Int\nlet x = 1\n".into(),
        ),
    ]));
    let mut sources = SourceMap::default();
    let mut project = library(&mut sources);
    project.config.dependencies.push(Dependency {
        name: "lib".into(),
        source: Source::Path("../lib".into()),
        span: Span::new(FileId::default(), 0, 0),
    });
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
fn named_functor_application_creates_an_importable_virtual_module() {
    let host = Memory(BTreeMap::from([
        (
            PathBuf::from("/p/src/App/Contracts.fl"),
            "module App.Contracts exposing (Source)\ntype module Source =\n  value : Int\n".into(),
        ),
        (
            PathBuf::from("/p/src/App/Argument.fl"),
            "module App.Argument exposing (value)\nvalue : Int\nlet value = 41\n".into(),
        ),
        (
            PathBuf::from("/p/src/App/Make.fl"),
            "module App.Make(K : App.Contracts.Source) : PublicResult exposing (answer)\nanswer : Int\nlet answer = K.value + 1\n\ntype module PublicResult =\n  answer : Int\n".into(),
        ),
        (
            PathBuf::from("/p/src/App/Collections.fl"),
            "module App.Collections exposing (Names)\nmodule Names = App.Make(App.Argument)\n".into(),
        ),
        (
            PathBuf::from("/p/src/App/Use.fl"),
            "module App.Use exposing (result)\nimport App.Collections.Names exposing (answer)\nresult : Int\nlet result = answer\n".into(),
        ),
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
    assert!(
        analysis
            .modules
            .iter()
            .any(|m| m.name == "App.Collections.Names")
    );
    assert!(analysis.compilation.modules.iter().any(|m| m.name == "App.Collections.Names" && m.ir.items.iter().any(|item| matches!(&item.kind, ir::ItemKind::Value { name, .. } if name == "answer"))));
}

#[cfg(feature = "ide")]
#[test]
fn inline_child_stays_in_its_source_file_and_is_not_importable() {
    use crate::diagnostics::codes;

    let host = Memory(BTreeMap::from([
        (
            PathBuf::from("/p/src/App/Parent.fl"),
            "module App.Parent exposing (answer)\nlet seed = 41\nmodule Child exposing (value) =\n  let value = seed + 1\nlet answer = Child.value\n".into(),
        ),
        (
            PathBuf::from("/p/src/App/Use.fl"),
            "module App.Use exposing (answer)\nimport App.Parent.Child exposing (value)\nlet answer = value\n".into(),
        ),
    ]));
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let parent = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Parent.fl")],
    );
    assert!(parent.diagnostics.is_empty(), "{:?}", parent.diagnostics);
    assert_eq!(parent.compilation.modules.len(), 1);
    assert!(parent.module("App.Parent.Child").is_some());
    let imported = analyze(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Use.fl")],
    );
    assert!(
        imported.diagnostics.iter().any(|d| d.code == codes::R018),
        "{:?}",
        imported.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn deeply_nested_inline_path_reports_the_inline_boundary() {
    use crate::diagnostics::codes;

    let host = Memory(BTreeMap::from([
        (
            PathBuf::from("/p/src/App/Parent.fl"),
            "module App.Parent exposing (answer)\nmodule Child exposing (value) =\n  module Inner exposing (value) =\n    let value = 42\n  let value = Inner.value\nlet answer = Child.value\n".into(),
        ),
        (
            PathBuf::from("/p/src/App/Use.fl"),
            "module App.Use exposing (answer)\nimport App.Parent.Child.Inner exposing (value)\nlet answer = value\n".into(),
        ),
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
        analysis.diagnostics.iter().any(|d| d.code == codes::R018),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn invalid_inline_application_reports_an_error_without_panicking() {
    use crate::diagnostics::codes;

    let host = Memory(BTreeMap::from([(
        PathBuf::from("/p/src/App/Main.fl"),
        "module App.Main exposing (answer)\nmodule Child exposing (answer) =\n  module Names = App.Make(App.Argument)\n  let answer = Names.answer\nlet answer = Child.answer\n".into(),
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
        analysis.diagnostics.iter().any(|d| d.code == codes::R019),
        "{:?}",
        analysis.diagnostics
    );
}

#[cfg(feature = "ide")]
#[test]
fn inline_import_is_loaded_for_the_child_without_entering_the_parent_scope() {
    let host = Memory(BTreeMap::from([
        (
            PathBuf::from("/p/src/App/Util.fl"),
            "module App.Util exposing (value)\nlet value = 42\n".into(),
        ),
        (
            PathBuf::from("/p/src/App/Parent.fl"),
            "module App.Parent exposing (answer)\nmodule Child exposing (value) =\n  import App.Util as U\n  let value = U.value\nlet answer = Child.value\n".into(),
        ),
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
    assert_eq!(
        analysis
            .compilation
            .modules
            .iter()
            .filter(|m| m.name == "App.Parent")
            .count(),
        1
    );
}

#[cfg(feature = "ide")]
#[test]
fn inline_module_can_define_a_module_contract() {
    let host = Memory(BTreeMap::from([(
        PathBuf::from("/p/src/App/Parent.fl"),
        "module App.Parent exposing (answer)\nmodule Child exposing (Sig, answer) =\n  type module Sig =\n    required : Int\n  let answer = 42\nanswer : Int\nlet answer = Child.answer\n".into(),
    )]));
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
    assert_eq!(analysis.compilation.modules.len(), 1);
}

#[cfg(feature = "ide")]
#[test]
fn inline_module_can_define_an_effect_and_a_foreign_value() {
    let host = Memory(BTreeMap::from([(
        PathBuf::from("/p/src/App/Parent.fl"),
        "module App.Parent exposing (answer)\nmodule Child exposing (Ask, probe, now) =\n  effect Ask =\n    ask : Unit ->{} Int\n  probe : Unit ->{Ask; none} Int\n  let probe () = Ask.ask ()\n  now : Unit ->{} Float\n  foreign now from global \"Date.now\"\n    arity 0\nanswer : Unit ->{Child.Ask; none} Int\nlet answer () = Child.probe ()\n".into(),
    )]));
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
    assert_eq!(analysis.compilation.modules.len(), 1);
}

#[cfg(feature = "ide")]
#[test]
fn inline_module_names_cannot_repeat_or_occupy_a_file_module_path() {
    use crate::diagnostics::codes;

    let duplicate = Memory(BTreeMap::from([(
        PathBuf::from("/p/src/App/Parent.fl"),
        "module App.Parent exposing (answer)\nmodule Child exposing (value) =\n  let value = 1\nmodule Child exposing (value) =\n  let value = 2\nlet answer = Child.value\n".into(),
    )]));
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &duplicate,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Parent.fl")],
    );
    assert!(
        analysis.diagnostics.iter().any(|d| d.code == codes::R002
            && d.span
                .is_some_and(|s| sources.get(s.file).line_col(s.start).0 == 3)),
        "{:?}",
        analysis.diagnostics
    );

    let conflict = Memory(BTreeMap::from([
        (PathBuf::from("/p/src/App/Parent.fl"),
         "module App.Parent exposing (answer)\nmodule Child exposing (value) =\n  let value = 1\nlet answer = Child.value\n".into()),
        (PathBuf::from("/p/src/App/Parent/Child.fl"),
         "module App.Parent.Child exposing (value)\nlet value = 2\n".into()),
    ]));
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(
        &conflict,
        &mut sources,
        Some(&project),
        &[
            PathBuf::from("/p/src/App/Parent.fl"),
            PathBuf::from("/p/src/App/Parent/Child.fl"),
        ],
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == codes::R002 && d.message.contains("inline module")),
        "{:?}",
        analysis.diagnostics
    );
}
