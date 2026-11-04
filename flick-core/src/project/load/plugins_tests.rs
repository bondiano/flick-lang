use std::collections::BTreeMap;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use flick_test_pairs::Pairs;

use super::{check, compile};
use crate::diagnostics::{Diagnostic, Severity};
use crate::ir;
use crate::plugin::{Expander, Native};
use crate::project::config::{Build, Config, Kind};
use crate::project::{Host, Project};
use crate::source::SourceMap;
use crate::types::Thresholds;

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

    fn plugin(&self, package: &str) -> Result<Arc<dyn Expander>, String> {
        match package {
            "pairs" => Ok(Arc::new(Native::new("pairs", Pairs))),
            _ => Err("no such plugin".into()),
        }
    }
}

const HEAD: &str =
    "module App.Main exposing (point)\n\nimport plugin \"pairs\" exposing (pairs, bad)\n\n";

fn library(sources: &mut SourceMap) -> Project {
    Project {
        root: PathBuf::from("/p"),
        config_file: sources.add("flick.toml", ""),
        config: Config {
            kind: Kind::Lib,
            source_dir: "src".into(),
            test_dir: "test".into(),
            entries: Vec::new(),
            dependencies: Vec::new(),
            package: None,
            build: Build::default(),
            sql_dialect: Default::default(),
            allow: Vec::new(),
            no_let_assert: false,
            thresholds: Thresholds::default(),
        },
    }
}

fn host(main: &str) -> Memory {
    Memory(BTreeMap::from([(
        PathBuf::from("/p/src/App/Main.fl"),
        main.to_owned(),
    )]))
}

fn roots() -> Vec<PathBuf> {
    vec![PathBuf::from("/p/src/App/Main.fl")]
}

/// Each diagnostic of checking `main` as `code text-at-span: message`.
fn report(main: &str) -> Vec<String> {
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let found = check(&host(main), &mut sources, Some(&project), &roots());
    found.iter().map(|d| show(&sources, d)).collect()
}

fn show(sources: &SourceMap, d: &Diagnostic) -> String {
    let at = d.span.map_or(String::new(), |s| {
        format!(" `{}`", &sources.get(s.file).text[s.range()])
    });
    format!("{}{at}: {}", d.code, d.message)
}

#[test]
fn a_block_stands_for_the_code_its_plugin_gives() {
    let main = format!(
        "{HEAD}point : {{ x : Float; label : String; blocks : Float }}\nlet point = pairs {{ x = 1.0 + 2.0; label : String = \"a\" }}\n"
    );
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let (diagnostics, compilation) = compile(&host(&main), &mut sources, Some(&project), &roots());
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let module = &compilation.modules[0];
    let printed = ir::print(&module.ir);
    assert!(printed.contains("blocks"), "{printed}");
    // every span of the IR is in the module's own text, where source maps
    // and runtime errors point
    let len = u32::try_from(main.len()).expect("short");
    let mut ir = module.ir.clone();
    ir.each_span_mut(&mut |s| assert!(s.end <= len, "{s:?} past {len}"));
}

#[test]
fn errors_land_on_the_users_code_or_on_the_block() {
    // in the user's code inside a hole: at that code
    let found = report(&format!(
        "{HEAD}point : {{ x : Float; blocks : Float }}\nlet point = pairs {{ x : Float = \"no\" }}\n"
    ));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("`\"no\"`"), "{found:?}");
    // in the generated code: on the block, with a note
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let main = format!("{HEAD}point : String\nlet point = bad {{}}\n");
    let found = check(&host(&main), &mut sources, Some(&project), &roots());
    let errors: Vec<&Diagnostic> = found
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "{found:?}");
    let span = errors[0].span.expect("a span");
    assert_eq!(&main[span.range()], "bad {}");
    assert!(
        errors[0]
            .related
            .iter()
            .any(|r| r.message.contains("plugin `pairs`")),
        "{:?}",
        errors[0].related
    );
    // what the plugin finds wrong: at the token it names
    let found = report(&format!(
        "{HEAD}point : Float\nlet point = pairs {{ 1 = 2 }}\n"
    ));
    assert!(
        found
            .iter()
            .any(|f| f.starts_with("FLICK-P036 `1`: plugin `pairs`: expected a name")),
        "{found:?}"
    );
}

#[test]
fn a_plugin_that_does_not_load_is_reported_once_on_its_import() {
    let main = "module App.Main exposing (x)\n\nimport plugin \"other\" exposing (other)\n\nlet x = other { a = 1 }\n";
    assert_eq!(
        report(main),
        ["FLICK-P035 `\"other\"`: plugin \"other\" cannot be loaded: no such plugin"]
    );
}

#[cfg(feature = "ide")]
#[test]
fn editors_see_the_users_code_inside_a_block() {
    use super::analyze;
    use crate::ide::Ide;

    let main = format!(
        "{HEAD}base : Float\nlet base = 2.0\n\npoint : {{ x : Float; blocks : Float }}\nlet point = pairs {{ x = base }}\n"
    );
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let analysis = analyze(&host(&main), &mut sources, Some(&project), &roots());
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let ide = Ide {
        analysis: &analysis,
        sources: &sources,
    };
    let file = analysis.modules[0].file;
    let inside = u32::try_from(main.rfind("base").expect("used")).expect("short");
    let (shown, _) = ide
        .hover(file, inside + 1)
        .expect("a hover inside the block");
    assert!(shown.contains("Float"), "{shown}");
    let renamed = ide.rename(file, inside + 1, "origin").expect("renames");
    let places: Vec<&str> = renamed
        .iter()
        .map(|(_, (s, e))| &main[*s as usize..*e as usize])
        .collect();
    // the declaration, its signature and the use inside the block
    assert_eq!(places.len(), 3, "{places:?}");
}
