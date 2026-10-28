use std::collections::BTreeMap;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::checking::levels;
use super::{ModuleCache, analyze, analyze_with};
use crate::project::{Host, discover};
use crate::source::SourceMap;
use crate::types::Typing;

struct Memory(BTreeMap<PathBuf, String>);

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

const LIB: &str = "schema-version = 1\n[project]\nkind = \"lib\"\n";
const USE: &str =
    "module A.Use exposing (y)\nimport A.Base exposing (x)\ny : Float\nlet y = x + 1\n";

fn host(base: &str) -> Memory {
    Memory(BTreeMap::from([
        (PathBuf::from("/p/flick.toml"), LIB.to_owned()),
        (PathBuf::from("/p/src/A/Base.fl"), base.to_owned()),
        (PathBuf::from("/p/src/A/Use.fl"), USE.to_owned()),
    ]))
}

/// Each module's typing and the diagnostics, as `code message`.
fn analyzed(
    host: &Memory,
    cache: Option<&mut ModuleCache>,
) -> (BTreeMap<String, Arc<Typing>>, Vec<String>) {
    let mut sources = SourceMap::default();
    let project = discover(host, &mut sources, Path::new("/p"), None)
        .expect("valid config")
        .expect("a project");
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
