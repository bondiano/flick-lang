use std::collections::BTreeMap;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use super::{CompletionKind, Ide, Symbol, Target};
use crate::project::{self, Analysis, Host, analysis_fingerprint};
use crate::projection;
use crate::source::{FileId, SourceMap};
use crate::types::builtin_sources;

struct Memory(BTreeMap<PathBuf, String>);

impl Host for Memory {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.0
            .get(path)
            .map(|t| t.clone().into_bytes())
            .ok_or_else(|| ErrorKind::NotFound.into())
    }

    fn display(&self, path: &Path) -> String {
        path.strip_prefix("/p")
            .unwrap_or(path)
            .display()
            .to_string()
    }
}

const LIB: &str = "schema-version = 1\n[project]\nkind = \"lib\"\n";

const DOMAIN: &str = "module App.Domain exposing (User, Shape(..), Named, greet, area)

type alias User = { name : String; age : Number }

type Shape = Circle Number | Square Number
  deriving Show, Schema

class Named a =
  name : a -> String

instance Named User =
  let name u = u.name

greet : User -> String
let greet user = \"Hello, \" ++ name user

area : Shape -> Number
let area s =
  match s with
  | Circle r -> r * r
  | Square a -> a * a
";

const MAIN: &str = "module App.Main exposing (run, keep, describe)

import App.Domain exposing (User, Shape(..), greet, area)

service Store =
  put : String ->{; StoreError} Unit

type error StoreError = StoreError String

run : User ->{Store; StoreError} Number
let run user =
  let total = area (Circle 2) + user.age
  Store.put (greet user)
  List.length [total]

shapes : PureCodec Json Shape
let shapes = Codec.json (schema Shape)

describe : Show a => a -> Option String
let describe x = Some (show x)

keep : forall c r. (Unit -> { name : String | r })^c -> (Unit -> { name : String | r })^c
let keep f = f
";

struct Fixture {
    analysis: Analysis,
    sources: SourceMap,
}

fn fixture() -> Fixture {
    let host = Memory(
        [
            ("flick.toml", LIB),
            ("src/App/Domain.fl", DOMAIN),
            ("src/App/Main.fl", MAIN),
        ]
        .iter()
        .map(|(p, t)| (Path::new("/p").join(p), (*t).to_owned()))
        .collect(),
    );
    let mut sources = SourceMap::default();
    let project = project::discover(&host, &mut sources, Path::new("/p"), None).expect("config");
    let roots = [PathBuf::from("/p/src/App/Main.fl")];
    let analysis = project::analyze(&host, &mut sources, project.as_ref(), &roots);
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    Fixture { analysis, sources }
}

impl Fixture {
    fn ide(&self) -> Ide<'_> {
        Ide {
            analysis: &self.analysis,
            sources: &self.sources,
            package_dir: &|_| None,
        }
    }

    fn file(&self, module: &str) -> FileId {
        self.analysis.module(module).expect("module").file
    }

    /// The offset of the `nth` occurrence of `needle` in `module`.
    fn at(&self, module: &str, needle: &str, nth: usize) -> u32 {
        let text = &self.sources.get(self.file(module)).text;
        let found = text.match_indices(needle).nth(nth).expect("occurs").0;
        u32::try_from(found).expect("small")
    }

    fn text_at(&self, target: &Target, start: u32, end: u32) -> String {
        match target {
            Target::File(f) => self.sources.get(*f).text[start as usize..end as usize].to_owned(),
            Target::Builtin(m) => {
                let (_, text) = builtin_sources()
                    .iter()
                    .find(|(n, _)| n == m)
                    .expect("builtin");
                text[start as usize..end as usize].to_owned()
            }
            Target::Path(p) => p.display().to_string(),
        }
    }
}

#[test]
fn hover_shows_source_types_with_rows_and_captures() {
    let f = fixture();
    let ide = f.ide();
    let main = f.file("App.Main");
    let hover = |needle: &str, nth| {
        ide.hover(main, f.at("App.Main", needle, nth) + 1)
            .unwrap_or_else(|| panic!("hover {needle}"))
            .0
    };
    assert!(
        hover("greet", 1).contains("greet : User -> String"),
        "{}",
        hover("greet", 1)
    );
    assert!(hover("run", 1).contains("run : User ->{Store; StoreError} Number"));
    assert!(
        hover("keep", 1).contains("(Unit -> { name : String | r })^c"),
        "{}",
        hover("keep", 1)
    );
    assert!(hover("total", 1).contains("total : Number"));
    assert!(
        hover("put (", 0).contains("Store.put : String ->{; StoreError} Unit"),
        "{}",
        hover("put (", 0)
    );
    assert!(
        !hover("run", 1).contains("Effect<"),
        "no TypeScript helpers"
    );
}

#[test]
fn definition_crosses_modules_and_reaches_builtins_and_instances() {
    let f = fixture();
    let ide = f.ide();
    let main = f.file("App.Main");
    let go = |module: &str, needle: &str, nth: usize| {
        let file = f.file(module);
        let (symbol, _) = ide
            .symbol_at(file, f.at(module, needle, nth) + 1)
            .expect("a symbol");
        let loc = ide.definition(&symbol).expect("a definition");
        (
            loc.target.clone(),
            f.text_at(&loc.target, loc.start, loc.end),
        )
    };
    let (target, text) = go("App.Main", "greet user", 0);
    assert_eq!(
        (target, text.as_str()),
        (Target::File(f.file("App.Domain")), "greet")
    );
    assert_eq!(go("App.Main", "Circle 2", 0).1, "Circle");
    // a companion leads to the declaration that derives it
    assert_eq!(go("App.Main", "Shape)", 0).1, "Shape");
    assert_eq!(go("App.Main", "User ->", 0).1, "User");
    assert_eq!(go("App.Main", "total]", 0).1, "total");
    assert_eq!(go("App.Main", "put (", 0).1, "put");
    // built-in modules: a function (in the part a facade re-exports), a
    // type and a constructor
    let (target, text) = go("App.Main", "length", 0);
    assert_eq!(
        (target, text.as_str()),
        (Target::Builtin("List.Core".into()), "length")
    );
    assert_eq!(go("App.Main", "String ->", 0).1, "String");
    assert_eq!(go("App.Domain", "Named User", 0).1, "Named");
    for (needle, name) in [
        ("Some (", "Some"),
        ("Show a", "Show"),
        ("show x", "show"),
        ("Option String", "Option"),
    ] {
        let (target, text) = go("App.Main", needle, 0);
        assert_eq!(
            (target, text.as_str()),
            (Target::Builtin("Prelude".into()), name),
            "{needle}"
        );
    }
    // an instance's method leads to the instance
    let (target, text) = go("App.Domain", "name user", 0);
    assert_eq!(
        (target, text.as_str()),
        (Target::File(f.file("App.Domain")), "name")
    );
    let domain = &f.sources.get(f.file("App.Domain")).text;
    let (symbol, _) = ide
        .symbol_at(f.file("App.Domain"), f.at("App.Domain", "name user", 0))
        .expect("symbol");
    let loc = ide.definition(&symbol).expect("definition");
    assert!(
        domain[..loc.start as usize].ends_with("instance Named User =\n  let "),
        "goes to the instance's let"
    );
    let _ = main;
}

#[test]
fn rename_changes_every_use_and_refuses_what_has_no_exact_mapping() {
    let f = fixture();
    let ide = f.ide();
    let edits = ide
        .rename(
            f.file("App.Domain"),
            f.at("App.Domain", "greet", 1) + 1,
            "welcome",
        )
        .expect("renamed");
    let count = |m: &str| edits.iter().filter(|(file, _)| *file == f.file(m)).count();
    // exposing, signature, let in Domain; exposing and the use in Main
    assert_eq!(
        (count("App.Domain"), count("App.Main")),
        (3, 2),
        "{edits:?}"
    );
    assert!(
        ide.rename(
            f.file("App.Main"),
            f.at("App.Main", "length", 0) + 1,
            "size"
        )
        .is_err(),
        "built-in"
    );
    assert!(
        ide.rename(
            f.file("App.Main"),
            f.at("App.Main", "put (", 0) + 1,
            "store"
        )
        .is_err(),
        "an operation"
    );
    assert!(
        ide.rename(
            f.file("App.Main"),
            f.at("App.Main", "total", 0) + 1,
            "Total"
        )
        .is_err(),
        "a bad name"
    );
    let local = ide
        .rename(f.file("App.Main"), f.at("App.Main", "total", 1) + 1, "sum")
        .expect("a local");
    assert_eq!(local.len(), 2);
}

#[test]
fn completion_offers_fields_operations_and_constructors() {
    let f = fixture();
    let ide = f.ide();
    let main = f.file("App.Main");
    let labels = |cs: Vec<super::Completion>| {
        cs.into_iter()
            .map(|c| (c.label, c.kind))
            .collect::<Vec<_>>()
    };
    let fields = labels(ide.complete(main, f.at("App.Main", "user.age", 0) + 4, true));
    assert!(
        fields.contains(&("age".into(), CompletionKind::Field))
            && fields.contains(&("name".into(), CompletionKind::Field)),
        "{fields:?}"
    );
    let ops = labels(ide.complete(main, f.at("App.Main", "Store.put", 0) + 5, true));
    assert_eq!(ops, vec![("put".into(), CompletionKind::Operation)]);
    let names = labels(ide.complete(main, f.at("App.Main", "Store.put", 0), false));
    for wanted in [
        ("Circle", CompletionKind::Constructor),
        ("greet", CompletionKind::Value),
        ("total", CompletionKind::Local),
        ("Some", CompletionKind::Constructor),
    ] {
        assert!(
            names.contains(&(wanted.0.into(), wanted.1)),
            "{wanted:?} in {names:?}"
        );
    }
    let module = labels(ide.complete(main, f.at("App.Main", "List.length", 0) + 4, true));
    assert!(module.contains(&("map".into(), CompletionKind::Value)));
    let _ = Symbol::Local {
        file: main,
        binder: (0, 0),
        name: String::new(),
    };
}

#[test]
fn the_projection_maps_names_exactly_and_comes_from_the_analysis() {
    let f = fixture();
    let ide = f.ide();
    let module = f.analysis.module("App.Main").expect("module");
    let text = &f.sources.get(module.file).text;
    let fingerprint = analysis_fingerprint(&f.analysis.compilation);
    let lookup = |m: &str| ide.interface(m);
    let projected = projection::project(module, text, &fingerprint, &lookup, &|_| None);
    assert!(
        projected
            .text
            .starts_with(&format!("// flick-analysis {fingerprint}\n"))
    );
    assert!(
        projected.text.contains("export declare const run:"),
        "{}",
        projected.text
    );
    let exact: Vec<&projection::Mapping> = projected.mappings.iter().filter(|m| m.exact).collect();
    assert!(exact.len() >= 3, "{:?}", projected.mappings);
    for m in exact {
        assert_eq!(
            &projected.text[m.ts[0] as usize..m.ts[1] as usize],
            &text[m.fl[0] as usize..m.fl[1] as usize]
        );
    }
}
