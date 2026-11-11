use std::slice;

#[test]
fn hom_package_templates_survive_serialization_and_missing_sources() {
    use crate::project::{CachedModule, PackageCache, compile_with};
    use crate::source::Span;

    let mut host = Memory(BTreeMap::from([
        (PathBuf::from("/lib/flick.toml"), "schema-version = 1\n[project]\nkind = \"lib\"\n".into()),
        (PathBuf::from("/lib/src/Lib/Contracts.fl"), "module Lib.Contracts exposing (Input, Output, Factory)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Factory = (K : Input) -> Output\n".into()),
        (PathBuf::from("/lib/src/Lib/Make.fl"), "module Lib.Make(K : Lib.Contracts.Input) : Lib.Contracts.Output =\n  answer : Int\n  let answer = K.seed + 1\n".into()),
        (PathBuf::from("/lib/src/Lib/Return.fl"), "module Lib.Return(F : Lib.Contracts.Factory) : Lib.Contracts.Factory = F\n".into()),
        (PathBuf::from("/p/src/App/Argument.fl"), "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n".into()),
        (PathBuf::from("/p/src/App/Root.fl"), "module App.Root exposing (Chosen)\nmodule Selected = Lib.Return(Lib.Make)\nmodule Chosen = Selected(App.Argument)\n".into()),
    ]));
    let mut sources = SourceMap::default();
    let mut project = library(&mut sources);
    project.config.dependencies.push(Dependency {
        name: "lib".into(),
        source: Source::Path("../lib".into()),
        span: Span::new(project.config_file, 0, 0),
    });
    let identity = |_: &Path, _: Option<&str>| Some("immutable-package-v1".to_owned());
    let empty = PackageCache {
        salt: "hom",
        identity: &identity,
        saved: &|_| None,
    };
    let roots = [PathBuf::from("/p/src/App/Root.fl")];
    let (diagnostics, _, fresh) = compile_with(
        &host,
        &mut sources,
        Some(&project),
        &roots,
        Some(&empty),
        false,
        None,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let saved: BTreeMap<_, Vec<CachedModule>> = fresh
        .into_iter()
        .map(|package| {
            let json = serde_json::to_vec(&package.modules).expect("package serializes");
            (
                package.key,
                serde_json::from_slice(&json).expect("package round trips"),
            )
        })
        .collect();
    let modules: Vec<_> = saved.values().flatten().map(|m| m.name.as_str()).collect();
    assert!(
        modules.contains(&"Lib.Make") && modules.contains(&"Lib.Return"),
        "{modules:?}"
    );

    host.0.retain(|path, _| !path.starts_with("/lib/src"));
    let cache = PackageCache {
        salt: "hom",
        identity: &identity,
        saved: &|key| saved.get(key).cloned(),
    };
    let (diagnostics, compilation, _) = compile_with(
        &host,
        &mut sources,
        Some(&project),
        &roots,
        Some(&cache),
        false,
        None,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let chosen = compilation
        .modules
        .iter()
        .find(|m| m.name == "App.Root.Chosen")
        .expect("chosen module");
    assert_eq!(
        chosen
            .interface
            .find(Namespace::Value, "answer")
            .and_then(|e| e.scheme.clone()),
        Some(types::Type::con("Prelude", "Int"))
    );
}

fn hom_package_fixture(make: &str, root: &str) -> (Memory, SourceMap, Project) {
    use crate::source::Span;
    let host = Memory(BTreeMap::from([
        (PathBuf::from("/lib/flick.toml"), "schema-version = 1\n[project]\nkind = \"lib\"\n".into()),
        (PathBuf::from("/lib/src/Lib/Contracts.fl"), "module Lib.Contracts exposing (Input, Output, Factory)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Factory = (K : Input) -> Output\n".into()),
        (PathBuf::from("/lib/src/Lib/Make.fl"), make.into()),
        (PathBuf::from("/p/src/App/Argument.fl"), "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n".into()),
        (PathBuf::from("/p/src/App/Root.fl"), root.into()),
    ]));
    let mut sources = SourceMap::default();
    let mut project = library(&mut sources);
    project.config.dependencies.push(Dependency {
        name: "lib".into(),
        source: Source::Path("../lib".into()),
        span: Span::new(project.config_file, 0, 0),
    });
    (host, sources, project)
}

fn hom_save_package(
    host: &Memory,
    sources: &mut SourceMap,
    project: &Project,
) -> BTreeMap<String, Vec<super::CachedModule>> {
    let empty = super::PackageCache {
        salt: "hom",
        identity: &|_, _| Some("immutable-package-v1".to_owned()),
        saved: &|_| None,
    };
    let (diagnostics, _, fresh) = super::compile_with(
        host,
        sources,
        Some(project),
        &[PathBuf::from("/p/src/App/Root.fl")],
        Some(&empty),
        false,
        None,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    fresh
        .into_iter()
        .map(|package| {
            let json = serde_json::to_vec(&package.modules).expect("package serializes");
            (
                package.key,
                serde_json::from_slice(&json).expect("package round trips"),
            )
        })
        .collect()
}

#[test]
fn hom_package_private_implementation_access_stays_lexical() {
    use crate::diagnostics::codes;
    let (mut host, mut sources, project) = hom_package_fixture(
        "private module Lib.Make(K : Lib.Contracts.Input) : Lib.Contracts.Output\nanswer : Int\nlet answer = K.seed + 1\n",
        "module App.Root exposing (Chosen)\nmodule Chosen = Lib.Public(App.Argument)\n",
    );
    host.0.insert(
        PathBuf::from("/lib/src/Lib/Public.fl"),
        "module Lib.Public(K : Lib.Contracts.Input) : Lib.Contracts.Output = Lib.Make(K)\n".into(),
    );
    let saved = hom_save_package(&host, &mut sources, &project);
    let private = saved
        .values()
        .flatten()
        .find(|m| m.name == "Lib.Make")
        .expect("private template");
    assert!(private.interface.private);
    host.0.retain(|path, _| !path.starts_with("/lib/src"));
    let cache = super::PackageCache {
        salt: "hom",
        identity: &|_, _| Some("immutable-package-v1".to_owned()),
        saved: &|key| saved.get(key).cloned(),
    };
    let root = PathBuf::from("/p/src/App/Root.fl");
    let (diagnostics, _, _) = super::compile_with(
        &host,
        &mut sources,
        Some(&project),
        slice::from_ref(&root),
        Some(&cache),
        false,
        None,
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    host.0.insert(
        root.clone(),
        "module App.Root exposing (Chosen)\nmodule Chosen = Lib.Make(App.Argument)\n".into(),
    );
    let (diagnostics, _, _) = super::compile_with(
        &host,
        &mut sources,
        Some(&project),
        &[root],
        Some(&cache),
        false,
        None,
    );
    assert!(
        diagnostics.iter().any(|d| d.code == codes::R017),
        "{diagnostics:?}"
    );
}

#[test]
fn hom_package_templates_detect_modified_immutable_sources() {
    use crate::diagnostics::codes;
    let (mut host, mut sources, project) = hom_package_fixture(
        "module Lib.Make(K : Lib.Contracts.Input) : Lib.Contracts.Output\nanswer : Int\nlet answer = K.seed + 1\n",
        "module App.Root exposing (Chosen)\nmodule Chosen = Lib.Make(App.Argument)\n",
    );
    let saved = hom_save_package(&host, &mut sources, &project);
    host.0
        .get_mut(&PathBuf::from("/lib/src/Lib/Make.fl"))
        .expect("source")
        .push_str("// changed immutable source\n");
    let cache = super::PackageCache {
        salt: "hom",
        identity: &|_, _| Some("immutable-package-v1".to_owned()),
        saved: &|key| saved.get(key).cloned(),
    };
    let (diagnostics, _, _) = super::compile_with(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Root.fl")],
        Some(&cache),
        false,
        None,
    );
    assert!(
        diagnostics.iter().any(|d| d.code == codes::P034),
        "{diagnostics:?}"
    );
}

#[test]
fn hom_package_private_static_artifacts_stay_private() {
    let (mut host, mut sources, project) = hom_package_fixture(
        "module Lib.Make(K : Lib.Contracts.Input) : Lib.Contracts.Output\nanswer : Int\nlet answer = K.seed + 1\n",
        "module App.Root exposing (Chosen)\nmodule Chosen = Lib.Published.Selected(App.Argument)\n",
    );
    host.0.insert(PathBuf::from("/lib/src/Lib/Return.fl"), "module Lib.Return(F : Lib.Contracts.Factory) : Lib.Contracts.Factory = module (K : Lib.Contracts.Input) ->\n  module Built = F(K)\n  answer : Int\n  let answer = Built.answer\n".into());
    host.0.insert(
        PathBuf::from("/lib/src/Lib/Published.fl"),
        "module Lib.Published exposing (Selected)\nmodule Selected = Lib.Return(Lib.Make)\n".into(),
    );
    let saved = hom_save_package(&host, &mut sources, &project);
    let closure = saved
        .values()
        .flatten()
        .find(|m| {
            m.name.starts_with("Lib.Published.Selected.") && m.interface.parameterized.is_some()
        })
        .expect("stored anonymous closure");
    assert!(closure.interface.private, "{}", closure.name);
    let selected = saved
        .values()
        .flatten()
        .find(|m| m.name == "Lib.Published.Selected")
        .expect("public factory");
    assert!(!selected.interface.private);
}
