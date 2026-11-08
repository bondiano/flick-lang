//! A file of another language as a module: `pairs` loads `.pairs` files.

use std::path::PathBuf;

use super::{host, library, show};
#[cfg(feature = "ide")]
use crate::ide::{Ide, Target};
#[cfg(feature = "ide")]
use crate::project::load::analyze;
use crate::project::load::check;
use crate::source::SourceMap;

/// `pairs` as a library whose plugin loads `.pairs` files.
const LOADING: &str = "schema-version = 1\n\n[package]\nname = \"pairs\"\nversion = \"0.1.0\"\nplugin = \"pairs.wasm\"\nload = [\"pairs\"]\n\n[project]\nkind = \"lib\"\n";

fn main_of(greeting: &str) -> String {
    format!(
        "module App.Main exposing (greeting)\n\nimport App.Words\n\ngreeting : {greeting}\nlet greeting = App.Words.hello\n"
    )
}

/// Each diagnostic of checking `main` beside `App/Words.pairs` of `words`.
fn report(main: &str, words: &str) -> Vec<String> {
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let host = host(main);
    host.write("/pairs/flick.toml", LOADING);
    host.write("/p/src/App/Words.pairs", words);
    let found = check(
        &host,
        &mut sources,
        Some(&project),
        &[PathBuf::from("/p/src/App/Main.fl")],
    );
    found.iter().map(|d| show(&sources, d)).collect()
}

#[test]
fn a_file_a_plugin_loads_is_a_module_the_project_imports_and_checks() {
    assert_eq!(
        report(&main_of("String"), "hello = \"hi\"\n"),
        Vec::<String>::new()
    );
    let found = report(&main_of("Float"), "hello = \"hi\"\n");
    assert!(
        found.iter().any(|f| f.starts_with("FLICK-T003")),
        "{found:?}"
    );
    // what the plugin finds wrong is at the file's own line; the module has
    // no code then, so what imports it misses its names too
    let found = report(&main_of("String"), "hello = \"hi\"\noops\n");
    assert!(
        found.contains(&"pairs/X004 `oops`: expected `name = \"text\"`".to_owned()),
        "{found:?}"
    );
}

#[cfg(feature = "ide")]
#[test]
fn an_editor_goes_from_a_use_to_the_line_of_the_file() {
    let main = main_of("String");
    let words = "\nhello = \"hi\"\n";
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let host = host(&main);
    host.write("/pairs/flick.toml", LOADING);
    host.write("/p/src/App/Words.pairs", words);
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
    let ide = Ide {
        analysis: &analysis,
        sources: &sources,
    };
    let file = analysis.module("App.Main").expect("the module").file;
    let used = u32::try_from(main.rfind("hello").expect("used")).expect("short");
    let (symbol, _) = ide.symbol_at(file, used + 1).expect("a symbol");
    let at = ide.definition(&symbol).expect("its definition");
    let loaded = analysis
        .module("App.Words")
        .expect("the loaded module")
        .file;
    assert_eq!(at.target, Target::File(loaded));
    assert_eq!(&words[at.start as usize..at.end as usize], "hello = \"hi\"");
}
