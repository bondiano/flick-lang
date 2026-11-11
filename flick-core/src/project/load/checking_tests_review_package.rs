#[test]
fn review_cached_alias_templates_preserve_canonical_materialized_targets() {
    use crate::project::{CachedModule, PackageCache, compile_with};
    use crate::source::Span;

    let mut host = Memory(BTreeMap::from([
        (PathBuf::from("/lib/flick.toml"), "schema-version = 1\n[project]\nkind = \"lib\"\n".into()),
        (PathBuf::from("/lib/src/Lib/Contracts.fl"), "module Lib.Contracts exposing (Input, Output, Factory)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Factory = (K : Input) -> Output\n".into()),
        (PathBuf::from("/lib/src/Lib/Make.fl"), "module Lib.Make(K : Lib.Contracts.Input) : Lib.Contracts.Output =\n  answer : Int\n  let answer = K.seed + 1\n".into()),
        (PathBuf::from("/lib/src/Lib/Return.fl"), "module Lib.Return(F : Lib.Contracts.Factory) : Lib.Contracts.Factory = F\n".into()),
        (PathBuf::from("/p/src/App/Argument.fl"), "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n".into()),
        (PathBuf::from("/p/src/App/Root.fl"), "module App.Root exposing (Chosen)\nmodule Selected = Lib.Return(Lib.Aliases.Selected)\nmodule Chosen = Selected(App.Argument)\n".into()),
        (PathBuf::from("/lib/src/Lib/Aliases.fl"), "module Lib.Aliases exposing (Selected, value)\nimport Lib.Make as Actual\nimport Lib.Other as Lib\nmodule Selected = Actual\nvalue : Int\nlet value = Lib.seed\n".into()),
        (PathBuf::from("/lib/src/Lib/Other.fl"), "module Lib.Other exposing (seed)\nseed : Int\nlet seed = 0\n".into()),
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
