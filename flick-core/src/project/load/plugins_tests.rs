use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use flick_plugin::{
    Arena, Block, CallSite, DeriveTarget, Diagnostic as PluginDiagnostic, Fact, Published,
};
use flick_test_pairs::Pairs;

mod calls;
mod derive;
mod forms;
mod loaded;
mod names;
mod nested;
mod published;
mod reach;
mod refined;
mod structure;

use super::{check, compile};
use crate::diagnostics::{Diagnostic, Severity};
use crate::ir;
use crate::plugin::reach::Outside;
use crate::plugin::source::BlockSource;
use crate::plugin::{Expander, Failure, Native};
use crate::project::config::{Build, Config, Dependency, Kind, Source};
use crate::project::{Host, Project};
use crate::source::{FileId, SourceMap, Span};
use crate::types::Thresholds;

/// The `pairs` plugin, counting the calls it answers.
struct Counting {
    inner: Native<Pairs>,
    collects: AtomicUsize,
    expands: AtomicUsize,
    call_states: Mutex<Vec<Vec<u8>>>,
}

impl Expander for Counting {
    fn check_calls(
        &self,
        calls: &[CallSite],
        state: &[u8],
    ) -> Result<Vec<PluginDiagnostic>, String> {
        self.call_states
            .lock()
            .expect("call states")
            .push(state.to_vec());
        self.inner.check_calls(calls, state)
    }

    fn id(&self) -> &str {
        self.inner.id()
    }

    fn collect(&self, blocks: &[Block], source: &mut BlockSource) -> Result<Vec<u8>, String> {
        self.collects.fetch_add(1, Ordering::Relaxed);
        self.inner.collect(blocks, source)
    }

    fn publish(&self, state: &[u8]) -> Result<Vec<Published>, String> {
        self.inner.publish(state)
    }

    fn expand(
        &self,
        block: &Block,
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<Arena, Failure> {
        self.expands.fetch_add(1, Ordering::Relaxed);
        self.inner.expand(block, state, source)
    }

    fn load(&self, path: &str, bytes: &[u8]) -> Result<Arena, Failure> {
        self.inner.load(path, bytes)
    }

    fn derive(
        &self,
        target: &DeriveTarget,
        class: &str,
        args: Option<&Block>,
        module: &[DeriveTarget],
        state: &[u8],
        source: &mut BlockSource,
    ) -> Result<Arena, Failure> {
        self.inner
            .derive(target, class, args, module, state, source)
    }

    fn check(
        &self,
        block: &Block,
        state: &[u8],
        facts: &[Fact],
    ) -> Result<Vec<PluginDiagnostic>, String> {
        self.inner.check(block, state, facts)
    }
}

struct Memory {
    files: RefCell<BTreeMap<PathBuf, String>>,
    plugin: Arc<Counting>,
    /// What plugins gave, when this host keeps it.
    store: Option<RefCell<HashMap<Vec<u8>, Vec<u8>>>>,
    outside: Option<Arc<dyn Outside>>,
}

impl Memory {
    /// The calls the plugin answered so far: collects, then expands.
    fn calls(&self) -> (usize, usize) {
        let count = |c: &AtomicUsize| c.load(Ordering::Relaxed);
        (count(&self.plugin.collects), count(&self.plugin.expands))
    }

    fn write(&self, path: &str, text: &str) {
        self.files
            .borrow_mut()
            .insert(PathBuf::from(path), text.to_owned());
    }
}

impl Host for Memory {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.files
            .borrow()
            .get(path)
            .map(|text| text.clone().into_bytes())
            .ok_or_else(|| ErrorKind::NotFound.into())
    }

    fn display(&self, path: &Path) -> String {
        path.display().to_string()
    }

    fn plugin(&self, package: &str, _: Option<&Path>) -> Result<Arc<dyn Expander>, String> {
        match package {
            "pairs" => Ok(self.plugin.clone()),
            _ => Err("no such plugin".into()),
        }
    }

    fn cached(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.store.as_ref()?.borrow().get(key).cloned()
    }

    fn save(&self, key: &[u8], value: &[u8]) {
        if let Some(store) = &self.store {
            store.borrow_mut().insert(key.to_vec(), value.to_vec());
        }
    }

    fn outside(&self) -> Option<Arc<dyn Outside>> {
        self.outside.clone()
    }
}

const HEAD: &str =
    "module App.Main exposing (point)\n\nimport Pairs exposing (plugin pairs, plugin bad)\n\n";

/// A library of the project, a path dependency with one module.
const LIBRARY: &str = "schema-version = 1\n\n[project]\nkind = \"lib\"\n";

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
            dependencies: ["pairs", "other"]
                .into_iter()
                .map(|name| Dependency {
                    name: name.into(),
                    source: Source::Path(format!("../{name}")),
                    span: Span::new(FileId::default(), 0, 0),
                })
                .collect(),
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

fn host(main: &str) -> Memory {
    let host = Memory {
        files: RefCell::default(),
        plugin: Arc::new(Counting {
            inner: Native::new("pairs", Pairs),
            collects: AtomicUsize::new(0),
            expands: AtomicUsize::new(0),
            call_states: Mutex::default(),
        }),
        store: Some(RefCell::default()),
        outside: None,
    };
    host.write("/p/src/App/Main.fl", main);
    host.write("/pairs/flick.toml", LIBRARY);
    host.write(
        "/pairs/src/Pairs.fl",
        "module Pairs exposing (version)\n\nversion : Float\nlet version = 1.0\n",
    );
    host.write("/other/flick.toml", LIBRARY);
    host.write(
        "/other/src/Other.fl",
        "module Other exposing (o)\n\no : Float\nlet o = 1.0\n",
    );
    host
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
fn a_plugin_that_does_not_load_is_reported_on_its_import_and_each_block() {
    let main = "module App.Main exposing (x)\n\nimport Other exposing (plugin other)\n\nlet x = other { a = 1 }\n";
    assert_eq!(
        report(main),
        [
            "FLICK-P035 `plugin other`: the plugin of package `other` cannot be loaded: no such plugin",
            "FLICK-P036 `other { a = 1 }`: plugin `other` is not loaded, so this block has no code",
        ]
    );
    // a head a plugin that loads gives too keeps that plugin
    let both = "module App.Main exposing (x)\n\nimport Pairs exposing (plugin pairs)\nimport Other exposing (plugin pairs)\n\nx : { a : Float; blocks : Float }\nlet x = pairs { a = 1.0 }\n";
    let found = report(both);
    assert!(
        found.iter().all(|f| !f.starts_with("FLICK-P036")),
        "{found:?}"
    );
    // a module of the project or a built-in one has no plugin
    let main = "module App.Main exposing (x)\n\nimport App.Main exposing (plugin mine)\nimport List exposing (plugin list)\n\nlet x = 1\n";
    let found = report(main);
    assert!(
        found
            .iter()
            .any(|f| f.contains("`plugin mine`: `App.Main` is a module of this project")),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|f| f.contains("`plugin list`: `List` is a built-in module")),
        "{found:?}"
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
    let file = analysis.module("App.Main").expect("the module").file;
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
    let tokens = ide.semantic_tokens(file);
    assert!(tokens.iter().any(|t| t.0 == inside), "{tokens:?}");
}

/// `main` with blocks `p`, `q` and more, each `pairs { label = value }`.
fn blocks(fields: &[(&str, &str)]) -> String {
    let mut main =
        String::from("module App.Main exposing (..)\n\nimport Pairs exposing (plugin pairs)\n");
    for (i, (label, value)) in fields.iter().enumerate() {
        let _ = write!(
            main,
            "\nb{i} : {{ {label} : Float; blocks : Float }}\nlet b{i} = pairs {{ {label} = {value} }}\n"
        );
    }
    main
}

fn compiled(host: &Memory) {
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let (diagnostics, _) = compile(host, &mut sources, Some(&project), &roots());
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn a_plugin_is_called_again_only_for_what_changed() {
    let host = host(&blocks(&[("x", "1.0"), ("y", "2.0")]));
    compiled(&host);
    assert_eq!(host.calls(), (1, 2));
    // the same build again: every result is the one saved
    compiled(&host);
    assert_eq!(host.calls(), (1, 2));
    // one block changed: collect again, which learns the same, and expand it
    host.write("/p/src/App/Main.fl", &blocks(&[("x", "1.0"), ("y", "3.0")]));
    compiled(&host);
    assert_eq!(host.calls(), (2, 3));
    // a new block changes what collect learns: each block of the group again
    host.write(
        "/p/src/App/Main.fl",
        &blocks(&[("x", "1.0"), ("y", "3.0"), ("z", "4.0")]),
    );
    compiled(&host);
    assert_eq!(host.calls(), (3, 6));
}

#[test]
fn a_saved_expansion_that_does_not_read_is_made_again() {
    let host = host(&blocks(&[("x", "1.0")]));
    compiled(&host);
    let store = host.store.as_ref().expect("a store");
    for (key, value) in store.borrow_mut().iter_mut() {
        if key.get(8..14) == Some(b"expand".as_slice()) {
            *value = b"not an arena".to_vec();
        }
    }
    compiled(&host);
    assert_eq!(host.calls(), (1, 2));
}

#[cfg(feature = "ide")]
#[test]
fn an_editor_session_keeps_plugin_results_in_memory() {
    use super::{ModuleCache, analyze_with};

    let other = "module App.Other exposing (o)\n\nimport Pairs exposing (plugin pairs)\n\no : { v : Float; blocks : Float }\nlet o = pairs { v = 1.0 }\n";
    let mut host = host(&blocks(&[("x", "1.0")]));
    host.store = None;
    host.write("/p/src/App/Other.fl", other);
    let roots = [
        PathBuf::from("/p/src/App/Main.fl"),
        PathBuf::from("/p/src/App/Other.fl"),
    ];
    let mut cache = ModuleCache::default();
    let mut analyze = |host: &Memory| {
        let mut sources = SourceMap::default();
        let project = library(&mut sources);
        let analysis = analyze_with(host, &mut sources, Some(&project), &roots, &mut cache);
        assert!(
            analysis.diagnostics.is_empty(),
            "{:?}",
            analysis.diagnostics
        );
        analysis
    };
    let first = analyze(&host);
    assert_eq!(host.calls(), (1, 2));
    let second = analyze(&host);
    assert_eq!(host.calls(), (1, 2));
    let typing = |a: &super::Analysis| Arc::clone(&a.module("App.Other").expect("other").typing);
    assert!(Arc::ptr_eq(&typing(&first), &typing(&second)));
    // a new block in Main changes what Other's block stands for
    host.write("/p/src/App/Main.fl", &blocks(&[("x", "1.0"), ("y", "2.0")]));
    let third = analyze(&host);
    assert!(!Arc::ptr_eq(&typing(&second), &typing(&third)));
}

#[test]
fn a_head_gives_its_arguments_to_the_plugin() {
    let tagged = |tag: &str| {
        format!(
            "{HEAD}point : {{ x : Float; arg0 : String; arg1 : Float; blocks : Float }}\nlet point = pairs \"{tag}\" (1.0 + 2.0) {{ x = 1.0 }}\n"
        )
    };
    let host = host(&tagged("a"));
    compiled(&host);
    assert_eq!(host.calls(), (1, 1));
    // the arguments are part of the block: another one is expanded again
    host.write("/p/src/App/Main.fl", &tagged("b"));
    compiled(&host);
    assert_eq!(host.calls(), (2, 2));
    // an error in an argument lands on it
    let found = report(&format!(
        "{HEAD}point : {{ x : Float; arg0 : Float; blocks : Float }}\nlet point = pairs \"no\" {{ x = 1.0 }}\n"
    ));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("`\"no\"`"), "{found:?}");
}

#[test]
fn a_plugin_reports_under_its_own_codes() {
    let coded = |code: &str| {
        report(&format!(
            "module App.Main exposing (x)\n\nimport Pairs exposing (plugin coded)\n\nlet x = coded {{ {code} }}\n"
        ))
    };
    // a span of the whole block is the block, its head with it
    assert_eq!(coded("X001"), ["pairs/X001 `coded { X001 }`: refused"]);
    // a code of another shape is no code of the plugin's
    assert_eq!(
        coded("FLICK-P001"),
        ["FLICK-P036 `coded { FLICK-P001 }`: plugin `pairs`: refused"]
    );
}

#[test]
fn a_head_of_std_needs_no_import_and_no_other_plugin_gives_it() {
    // `cli_ir` imports nothing
    let block = "greet : Cli.Command none none\nlet greet = cli \"greet\" { handle fun {} -> Platform.stdout_line \"hi\" }\n\ntool : Cli.Command none none\nlet tool = cli \"tool\" { subcommand greet; subcommand version }";
    assert!(cli_ir(block).contains("Cli.handled"));
    let main = "module App.Main exposing (x)\n\nimport Pairs exposing (plugin sql)\n\nlet x = 1\n";
    assert_eq!(
        report(main),
        [
            "FLICK-P044 `plugin sql`: `sql` is a head of `std`'s plugin, which every module has, so the plugin of `pairs` cannot give it"
        ]
    );
}

/// The IR of `App.Main` with `greet` and `tool` as `body` writes them.
fn cli_ir(body: &str) -> String {
    const HANDLER: &str = "fun { loud; times; name } -> Platform.stdout_line (String.join \"\\n\" (List.repeat times (if loud then String.to_upper name else name)))";
    let main = format!(
        "module App.Main exposing (greet, tool)\n\nversion : Cli.Command none none\nlet version = Cli.command \"version\" |> Cli.handle (fun {{}} -> Platform.stdout_line \"0.1.0\")\n\n{}\n",
        body.replace("HANDLER", HANDLER)
    );
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let (diagnostics, compilation) = compile(&host(&main), &mut sources, Some(&project), &roots());
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    ir::print(&compilation.modules[0].ir)
}

#[test]
fn a_cli_block_is_the_calls_of_cli_it_spells_out() {
    let block = cli_ir(
        "greet : Cli.Command none none\nlet greet = cli \"greet\" {\n  description \"Prints a greeting\"\n  /// Print in capitals\n  flag loud : Bool\n  /// How many\n  /// times\n  flag times \"n\" : Int = 1\n  arg name : String\n  handle HANDLER\n}\n\ntool : Cli.Command none none\nlet tool = cli \"tool\" { flag verbose \"v\" : Bool; subcommand greet; subcommand version }",
    );
    let calls = cli_ir(
        "greet : Cli.Command none none\nlet greet =\n  Cli.handled\n    (Cli.arg (Record.label : Record.Label \"name\") \"\" (Cli.required : Cli.Multi String String)\n      (Cli.flag (Record.label : Record.Label \"times\") [\"n\"] \"How many times\" (Cli.default 1)\n        (Cli.flag (Record.label : Record.Label \"loud\") [] \"Print in capitals\" Cli.switch\n          (Cli.description \"Prints a greeting\" (Cli.command \"greet\")))))\n    (HANDLER)\n\ntool : Cli.Command none none\nlet tool =\n  Cli.group\n    (Cli.subcommand version \"\"\n      (Cli.subcommand greet \"\"\n        (Cli.flag (Record.label : Record.Label \"verbose\") [\"v\"] \"\" Cli.switch (Cli.command \"tool\"))))",
    );
    assert_eq!(block, calls);
}
