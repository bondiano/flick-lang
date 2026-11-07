use std::collections::BTreeMap;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use super::{CompletionKind, Ide, Symbol, Target};
use crate::diagnostics::Diagnostic;
use crate::project::config::{Build, Config, Kind};
use crate::project::{self, Analysis, Host, analysis_fingerprint};
use crate::projection;
use crate::source::{FileId, SourceMap};
use crate::types::{Thresholds, builtin_sources};

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

const DOMAIN: &str = "module App.Domain exposing (User, Shape(..), Named, greet, area)

type alias User = { name : String; age : Float }

type Shape = Circle Float | Square Float
  deriving Show, Schema

class Named a =
  name : a -> String

instance Named User =
  let name u = u.name

greet : User -> String
let greet user = \"Hello, \" ++ name user

area : Shape -> Float
let area s =
  match s with
  | Circle r -> r * r
  | Square a -> a * a

type Pair a b = Pair a b
  deriving Eq, Ord

type Tagged (f : Type -> Type) a = Tagged a
  deriving Hash, Eq
";

const MAIN: &str = "module App.Main exposing (run, keep, describe)

import App.Domain exposing (User, Shape(..), greet, area)
import Sql exposing (plugin sql)

service Store =
  put : String ->{; StoreError} Unit

type error StoreError = StoreError String

run : User ->{Store; StoreError} Int
let run user =
  let total = area (Circle 2) + user.age
  Store.put (greet user)
  List.length [total]

shapes : PureCodec Json Shape
let shapes = Codec.json Shape.schema

describe : Show a => a -> Option String
let describe x = Some (show x)

keep : forall c r. (Unit -> { name : String | r })^c -> (Unit -> { name : String | r })^c
let keep f = f

saved : User -> Eff {Store} {StoreError} Float
let saved user =
  task {
    Store.put user.name
    1
  }

counted : User ->{Store; StoreError} Int
let counted user =
  ignore (saved user)
  ignore (run user)
  scope {
    run user
  }

picked : User ->{Store; StoreError} Float
let picked user =
  let m = await (saved user)
  if m > 0 then (fun x -> x + 1) m else 0

sign : Float -> String
let sign n =
  match n with
  | 0 -> \"zero\"
  | _ -> \"other\"

sql table Items \"items\" {
  id : bigint primary key generated always as BigInt
  label : text not null as String
}

items_of : User -> Sql.Template Sql.Pg {} { label : String }
let items_of owner =
  sql {
    from item in Items
    where item.label == $owner.name || item.label == $(greet owner)
    select { label = item.label }
  }
";

pub(super) struct Fixture {
    analysis: Analysis,
    sources: SourceMap,
}

fn fixture() -> Fixture {
    let fixture = fixture_of(&[("src/App/Domain.fl", DOMAIN), ("src/App/Main.fl", MAIN)]);
    assert!(
        fixture
            .analysis
            .diagnostics
            .iter()
            .all(Diagnostic::unnecessary),
        "{:?}",
        fixture.analysis.diagnostics
    );
    fixture
}

/// A library project of `files` (paths under `src/`), analysed from its last file.
pub(super) fn fixture_of(files: &[(&str, &str)]) -> Fixture {
    let host = Memory(
        files
            .iter()
            .map(|(p, t)| (Path::new("/p").join(p), (*t).to_owned()))
            .collect(),
    );
    let mut sources = SourceMap::default();
    let project = project::Project {
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
    };
    let last = files.last().expect("a file").0;
    let roots = [Path::new("/p").join(last)];
    let analysis = project::analyze(&host, &mut sources, Some(&project), &roots);
    Fixture { analysis, sources }
}

impl Fixture {
    pub(super) fn ide(&self) -> Ide<'_> {
        Ide {
            analysis: &self.analysis,
            sources: &self.sources,
        }
    }

    pub(super) fn file(&self, module: &str) -> FileId {
        self.analysis.module(module).expect("module").file
    }

    /// The offset of the `nth` occurrence of `needle` in `module`.
    pub(super) fn at(&self, module: &str, needle: &str, nth: usize) -> u32 {
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
    assert!(hover("run", 1).contains("run : User ->{Store; StoreError} Int"));
    assert!(
        hover("keep", 1).contains("(Unit -> { name : String | r })^c"),
        "{}",
        hover("keep", 1)
    );
    assert!(hover("total", 1).contains("total : Float"));
    // a parameter of a top-level function, where it is used
    assert!(
        hover("user.age", 0).contains("user : User"),
        "{}",
        hover("user.age", 0)
    );
    assert!(hover(" x)", 0).contains("x : a"), "{}", hover(" x)", 0));
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
fn hover_on_a_keyword_shows_the_type_of_its_whole_block() {
    let f = fixture();
    let ide = f.ide();
    let main = f.file("App.Main");
    let hover = |needle: &str, nth| {
        ide.hover(main, f.at("App.Main", needle, nth) + 1)
            .unwrap_or_else(|| panic!("hover {needle}"))
            .0
    };
    let task = hover("task {", 0);
    assert!(
        task.contains("task { … } : Eff {Store} {StoreError} Float"),
        "{task}"
    );
    let scope = hover("scope {", 0);
    assert!(scope.contains("scope { … } : Int"), "{scope}");
    let deferred = hover("ignore (saved", 0);
    assert!(
        deferred.contains("ignore : Eff {Store} {StoreError} Float -> Unit")
            && deferred.contains("is not run"),
        "{deferred}"
    );
    let awaited = hover("await (saved", 0);
    assert!(
        awaited.contains("await … : Float")
            && awaited.contains("Runs `Eff {Store} {StoreError} Float`"),
        "{awaited}"
    );
    for (needle, shown) in [
        ("fun x", "fun … -> … : Float -> Float"),
        ("if m", "if … then … else … : Float"),
        ("match n", "match … with : String"),
    ] {
        let text = hover(needle, 0);
        assert!(text.contains(shown), "{needle}: {text}");
    }
    let ran = hover("ignore (run", 0);
    assert!(
        ran.contains("ignore : Int -> Unit") && !ran.contains("is not run"),
        "{ran}"
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
    assert_eq!(go("App.Main", "Shape.schema", 0).1, "Shape");
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
    // exposing, signature, let in Domain; exposing, the use and the
    // capture `$(greet owner)` in Main
    assert_eq!(
        (count("App.Domain"), count("App.Main")),
        (3, 3),
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
    // a class method: its signature, instances and calls cannot all be found yet
    let domain = f.file("App.Domain");
    for at in [
        f.at("App.Domain", "name :", 0),
        f.at("App.Domain", "name user", 0),
    ] {
        let refused = ide.renamable(domain, at + 1).map(|(_, range, _)| range);
        assert!(refused.is_err(), "{at}: {refused:?}");
        assert!(ide.rename(domain, at + 1, "label").is_err());
    }
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
    let projected = projection::project(module, text, &fingerprint, &lookup, &|_| None, &|m| {
        projection::relative(&module.name, m)
    });
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

#[test]
fn a_documented_declaration_keeps_its_exact_mappings() {
    let f = fixture_of(&[(
        "src/App/Docs.fl",
        "module App.Docs exposing (User, greet)\n\n/// A user.\ntype alias User = { name : String }\n\n\
         /// Greets.\ngreet : User -> String\nlet greet user = user.name\n",
    )]);
    let ide = f.ide();
    let module = f.analysis.module("App.Docs").expect("module");
    let text = &f.sources.get(module.file).text;
    let lookup = |m: &str| ide.interface(m);
    let projected = projection::project(module, text, "", &lookup, &|_| None, &|m| {
        projection::relative(&module.name, m)
    });
    let ts = &projected.text;
    assert!(
        ts.contains("/** A user. */\nexport interface User {"),
        "{ts}"
    );
    assert!(
        ts.contains("/** Greets. */\nexport declare const greet:"),
        "{ts}"
    );
    let exact: Vec<&projection::Mapping> = projected.mappings.iter().filter(|m| m.exact).collect();
    assert_eq!(exact.len(), 2, "{:?}", projected.mappings);
    for m in exact {
        assert_eq!(
            &ts[m.ts[0] as usize..m.ts[1] as usize],
            &text[m.fl[0] as usize..m.fl[1] as usize]
        );
    }
}

#[test]
fn captures_in_sql_are_names_of_the_program() {
    let f = fixture();
    let ide = f.ide();
    let main = f.file("App.Main");
    let text = &f.sources.get(main).text;
    let go = |needle: &str| {
        let (symbol, _) = ide
            .symbol_at(main, f.at("App.Main", needle, 0) + 1)
            .expect("a symbol");
        let loc = ide.definition(&symbol).expect("a definition");
        (loc.start, f.text_at(&loc.target, loc.start, loc.end))
    };
    let binder = f.at("App.Main", "owner =", 0);
    assert_eq!(go("owner.name"), (binder, "owner".into()));
    assert_eq!(go("owner)"), (binder, "owner".into()));
    assert_eq!(go("greet owner)").1, "greet");
    let (hover, _) = ide
        .hover(main, f.at("App.Main", "owner.name", 0) + 1)
        .expect("a hover");
    assert!(hover.contains("owner : User"), "{hover}");
    let labels = |cs: Vec<super::Completion>| {
        cs.into_iter()
            .map(|c| (c.label, c.kind))
            .collect::<Vec<_>>()
    };
    let fields = labels(ide.complete(main, f.at("App.Main", "owner.name", 0) + 5, true));
    assert!(
        fields.contains(&("age".into(), CompletionKind::Field)),
        "{fields:?}"
    );
    let names = labels(ide.complete(main, f.at("App.Main", "owner.name", 0), false));
    assert!(
        names.contains(&("owner".into(), CompletionKind::Local)),
        "{names:?}"
    );
    let edits = ide
        .rename(main, f.at("App.Main", "owner.name", 0) + 1, "who")
        .expect("renamed");
    let renamed: Vec<&str> = edits
        .iter()
        .map(|(_, (s, e))| &text[*s as usize..*e as usize])
        .collect();
    assert_eq!(renamed, ["owner"; 3]);
}

#[test]
fn hover_on_a_derived_class_shows_the_instance_it_makes() {
    let f = fixture();
    let ide = f.ide();
    let domain = f.file("App.Domain");
    let hover = |needle: &str| {
        ide.hover(domain, f.at("App.Domain", needle, 0) + 1)
            .unwrap_or_else(|| panic!("hover {needle}"))
            .0
    };
    let show = hover("Show, Schema");
    assert!(
        show.contains("instance Show Shape")
            && show.contains("`Circle 2`")
            && show.contains("from `Prelude`"),
        "{show}"
    );
    let ord = hover("Ord\n");
    assert!(
        ord.contains("instance (Ord a, Ord b) => Ord (Pair a b)"),
        "{ord}"
    );
    // no context for `f`, a parameter of a higher kind
    let tagged = hover("Hash, Eq");
    assert!(
        tagged.contains("instance Hash a => Hash (Tagged f a)"),
        "{tagged}"
    );
    let schema = hover("Schema\n");
    assert!(schema.contains("Shape.schema"), "{schema}");
    let at = f.at("App.Domain", "Show, Schema", 0);
    let (_, range) = ide.symbol_at(domain, at + 1).expect("Show");
    assert_eq!(range, (at, at + 4));
}

#[test]
fn hover_on_component_shows_its_instance() {
    let source = "module App.Main exposing (counter)\n\n\
                  component counter (start : Float) =\n  let count = Atom.local start\n  \
                  ui { text (show (Atom.value count)) }\n";
    let f = fixture_of(&[("src/App/Main.fl", source)]);
    let ide = f.ide();
    let main = f.file("App.Main");
    let hover = ide
        .hover(main, f.at("App.Main", "component", 0) + 1)
        .expect("hover component")
        .0;
    assert!(
        hover.contains("component : Float ->{Setup s; none} (Float ->{Read; none} Ui^{s})^{s}"),
        "{hover}"
    );
}
