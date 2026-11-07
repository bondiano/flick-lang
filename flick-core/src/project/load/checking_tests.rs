#[cfg(feature = "ide")]
use std::collections::BTreeMap;
#[cfg(feature = "ide")]
use std::io::{self, ErrorKind};
#[cfg(feature = "ide")]
use std::path::{Path, PathBuf};
#[cfg(feature = "ide")]
use std::sync::Arc;

use super::checking::levels;
#[cfg(feature = "ide")]
use super::{ModuleCache, analyze, analyze_with};
#[cfg(feature = "ide")]
use crate::project::config::{Build, Config, Kind};
#[cfg(feature = "ide")]
use crate::project::{Host, Project};
#[cfg(feature = "ide")]
use crate::source::SourceMap;
#[cfg(feature = "ide")]
use crate::types::Thresholds;
#[cfg(feature = "ide")]
use crate::types::Typing;

#[cfg(feature = "ide")]
struct Memory(BTreeMap<PathBuf, String>);

#[cfg(feature = "ide")]
impl Host for Memory {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.0
            .get(path)
            .map(|text| text.clone().into_bytes())
            .ok_or_else(|| ErrorKind::NotFound.into())
    }

    fn display(&self, path: &Path) -> String {
        path.display().to_string()
    }
}

#[cfg(feature = "ide")]
const USE: &str =
    "module A.Use exposing (y)\nimport A.Base exposing (x)\ny : Float\nlet y = x + 1\n";

#[cfg(feature = "ide")]
fn host(base: &str) -> Memory {
    Memory(BTreeMap::from([
        (PathBuf::from("/p/src/A/Base.fl"), base.to_owned()),
        (PathBuf::from("/p/src/A/Use.fl"), USE.to_owned()),
    ]))
}

#[cfg(feature = "ide")]
fn library(sources: &mut SourceMap) -> Project {
    Project {
        root: PathBuf::from("/p"),
        config_file: sources.add("flick.toml", ""),
        workspace: None,
        config: Config {
            kind: Kind::Lib,
            source_dir: "src".into(),
            test_dir: "test".into(),
            entries: Vec::new(),
            dependencies: Vec::new(),
            js_dependencies: Vec::new(),
            package: None,
            build: Build::default(),
            sql_dialect: Default::default(),
            allow: Vec::new(),
            no_let_assert: false,
            thresholds: Thresholds::default(),
            plugins: BTreeMap::new(),
        },
    }
}

#[cfg(feature = "ide")]
/// Each module's typing and the diagnostics, as `code message`.
fn analyzed(
    host: &Memory,
    cache: Option<&mut ModuleCache>,
) -> (BTreeMap<String, Arc<Typing>>, Vec<String>) {
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let roots = [PathBuf::from("/p/src/A/Use.fl")];
    let analysis = match cache {
        Some(cache) => analyze_with(host, &mut sources, Some(&project), &roots, cache),
        None => analyze(host, &mut sources, Some(&project), &roots),
    };
    let typings = analysis
        .modules
        .iter()
        .map(|m| (m.name.clone(), Arc::clone(&m.typing)))
        .collect();
    let diagnostics = analysis
        .diagnostics
        .iter()
        .map(|d| format!("{} {}", d.code, d.message))
        .collect();
    (typings, diagnostics)
}

#[test]
fn levels_follow_imports_and_reject_cycles() {
    // 0 imports 1 and 2, 2 imports 1
    assert_eq!(
        levels(&[vec![1, 2], vec![], vec![1]]),
        Some(vec![vec![1], vec![2], vec![0]])
    );
    assert_eq!(levels(&[vec![1], vec![0]]), None);
}

#[cfg(feature = "ide")]
#[test]
fn an_edit_that_keeps_the_interface_rechecks_only_its_module() {
    let mut cache = ModuleCache::default();
    let (first, _) = analyzed(
        &host("module A.Base exposing (x)\nx : Float\nlet x = 1\n"),
        Some(&mut cache),
    );
    let (again, _) = analyzed(
        &host("module A.Base exposing (x)\nx : Float\nlet x = 1\n"),
        Some(&mut cache),
    );
    assert!(Arc::ptr_eq(&first["A.Base"], &again["A.Base"]));
    assert!(Arc::ptr_eq(&first["A.Use"], &again["A.Use"]));

    // a longer body moves nothing importers check
    let edited = host("module A.Base exposing (x)\n\nx : Float\nlet x = 1 + 2\n");
    let (warm, diagnostics) = analyzed(&edited, Some(&mut cache));
    assert!(!Arc::ptr_eq(&again["A.Base"], &warm["A.Base"]));
    assert!(Arc::ptr_eq(&again["A.Use"], &warm["A.Use"]));
    assert_eq!(diagnostics, analyzed(&edited, None).1);
}

#[cfg(feature = "ide")]
#[test]
fn a_changed_interface_rechecks_its_importers() {
    let mut cache = ModuleCache::default();
    let (first, _) = analyzed(
        &host("module A.Base exposing (x)\nx : Float\nlet x = 1\n"),
        Some(&mut cache),
    );
    let edited = host("module A.Base exposing (x)\nx : String\nlet x = \"one\"\n");
    let (warm, diagnostics) = analyzed(&edited, Some(&mut cache));
    assert!(!Arc::ptr_eq(&first["A.Use"], &warm["A.Use"]));
    assert!(!diagnostics.is_empty(), "`x + 1` no longer checks");
    assert_eq!(diagnostics, analyzed(&edited, None).1);
}

#[cfg(feature = "ide")]
#[test]
fn changing_programmatic_config_invalidates_cached_checking() {
    use crate::diagnostics::{Severity, codes};
    use crate::project::config::Backend;

    let host = host(
        "module A.Base exposing (x)\nx : Unit -> Unit\nlet x () =\n  let _ = task { 1 }\n  ()\n",
    );
    let roots = [PathBuf::from("/p/src/A/Base.fl")];
    let mut cache = ModuleCache::default();
    let mut sources = SourceMap::default();
    let mut project = library(&mut sources);
    let first = analyze_with(&host, &mut sources, Some(&project), &roots, &mut cache);
    assert!(
        first
            .diagnostics
            .iter()
            .all(|d| d.severity != Severity::Error),
        "{:?}",
        first.diagnostics
    );

    let mut sources = SourceMap::default();
    assert_eq!(sources.add("flick.toml", ""), project.config_file);
    project.config.build.effect = Backend::None;
    let changed = analyze_with(&host, &mut sources, Some(&project), &roots, &mut cache);
    assert!(
        changed.diagnostics.iter().any(|d| d.code == codes::E008),
        "{:?}",
        changed.diagnostics
    );
    assert!(!Arc::ptr_eq(
        &first.modules[0].typing,
        &changed.modules[0].typing
    ));
}
