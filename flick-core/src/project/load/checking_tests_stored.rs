use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{CachedModule, ModuleStore, PackageCache};
use crate::diagnostics::Diagnostic;
use crate::project::config::Backend;
use crate::source::Span;

/// What a host keeps of the modules of a project between processes.
type Saved = Arc<Mutex<BTreeMap<String, Vec<u8>>>>;

/// A store as a host keeps one on disk: each module by its key, written as
/// JSON; `taken` counts the modules it gave back.
fn memory_store(salt: &str, saved: &Saved, taken: &Arc<AtomicUsize>) -> ModuleStore {
    let (reader, writer, taken) = (Arc::clone(saved), Arc::clone(saved), Arc::clone(taken));
    ModuleStore {
        salt: salt.to_owned(),
        load: Box::new(move |key| {
            let bytes = reader.lock().expect("the store").get(key).cloned()?;
            taken.fetch_add(1, Ordering::Relaxed);
            Some(serde_json::from_slice(&bytes).expect("a saved module"))
        }),
        save: Box::new(move |key, module| {
            let bytes = serde_json::to_vec(module).expect("a module to save");
            writer
                .lock()
                .expect("the store")
                .insert(key.to_owned(), bytes);
        }),
    }
}

fn stored_count(taken: &AtomicUsize) -> usize {
    taken.load(Ordering::Relaxed)
}

/// How many modules a check of `roots` in a process of its own takes from
/// `saved` under `salt`, with the project changed by `edit`.
fn taken_by(
    host: &Memory,
    saved: &Saved,
    salt: &str,
    roots: &[&str],
    edit: impl Fn(&mut Project),
) -> usize {
    let taken = Arc::default();
    let mut cache = ModuleCache::stored(memory_store(salt, saved, &taken));
    let mut sources = SourceMap::default();
    let mut project = library(&mut sources);
    edit(&mut project);
    let roots: Vec<PathBuf> = roots.iter().map(PathBuf::from).collect();
    let output = run(
        host,
        &mut sources,
        Some(&project),
        &roots,
        Mode::Check,
        Some(&mut cache),
        None,
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    stored_count(&taken)
}

fn three_modules() -> Memory {
    Memory(BTreeMap::from([
        (
            PathBuf::from("/p/src/A/Base.fl"),
            "module A.Base exposing (x)\nx : Float\nlet x = 1\n".to_owned(),
        ),
        (PathBuf::from("/p/src/A/Use.fl"), USE.to_owned()),
        (
            PathBuf::from("/p/src/A/Other.fl"),
            "module A.Other exposing (o)\no : Float\nlet o = 2\n".to_owned(),
        ),
    ]))
}

/// A module saved under one `[lint]` serves no run under another: there it
/// says what a run without a store says.
#[test]
fn a_saved_module_serves_only_the_configuration_it_was_checked_under() {
    let host = Memory(BTreeMap::from([(
        PathBuf::from("/p/src/A/Use.fl"),
        "module A.Use exposing (first)\nfirst : List Float -> Float\nlet first xs =\n  let assert [x, ..._] = xs\n  x\n"
            .to_owned(),
    )]));
    let (saved, taken): (Saved, Arc<AtomicUsize>) = Default::default();
    for strict in [false, true] {
        let edit = |p: &mut Project| p.config.no_let_assert = strict;
        let mut cache = ModuleCache::stored(memory_store("one", &saved, &taken));
        let stored = said_in(&host, Mode::Check, Some(&mut cache), 1, edit);
        let alone = said_in(&host, Mode::Check, None, 1, edit);
        assert_eq!(stored, alone);
        assert_eq!(stored.0.iter().any(|d| d.contains("T027")), strict);
    }
    assert_eq!(
        stored_count(&taken),
        0,
        "the module was saved once, unchecked by `[lint]`"
    );
    assert!(taken_by(&host, &saved, "one", &["/p/src/A/Use.fl"], |_| {}) > 0);
}

/// A dependent saved by a clean run is checked again when its import stops
/// parsing: the saved interface must not hide the names the parser lost or
/// the types it changed.
#[test]
fn a_saved_importer_is_rechecked_when_its_import_stops_parsing() {
    let base = |text: &str| format!("module A.Base exposing (x)\n{text}");
    let clean = Memory(BTreeMap::from([
        (
            PathBuf::from("/p/src/A/Base.fl"),
            base("x : Float\nlet x = 1\n"),
        ),
        (PathBuf::from("/p/src/A/Use.fl"), USE.to_owned()),
    ]));
    let saved = Saved::default();
    assert_eq!(
        taken_by(&clean, &saved, "one", &["/p/src/A/Use.fl"], |_| {}),
        0,
        "the first run checks both modules"
    );

    // The parser loses the body of `T`, and `x` changes its type. A saved
    // `A.Use` checked against the old interface would miss the mismatch.
    let broken = Memory(BTreeMap::from([
        (
            PathBuf::from("/p/src/A/Base.fl"),
            base(
                "class T (t : Type -> Type)) =\n  lift : forall a. a -> t a\nx : String\nlet x = \"s\"\n",
            ),
        ),
        (PathBuf::from("/p/src/A/Use.fl"), USE.to_owned()),
    ]));
    let taken = Arc::default();
    let mut cache = ModuleCache::stored(memory_store("one", &saved, &taken));
    let stored = said(&broken, Mode::Check, Some(&mut cache));
    assert!(
        stored
            .0
            .iter()
            .any(|d| d.starts_with("FLICK-T003") && d.contains("String")),
        "the importer is checked against the new type: {:?}",
        stored.0
    );
    let fresh = said(&broken, Mode::Check, None);
    assert_eq!(stored, fresh);
    assert_eq!(stored_count(&taken), 0, "no changed module is taken back");
}

/// A module saved by one compiler, std or mode of the stages serves no run
/// of another, and none where another module joins the run, which can
/// change what is local to it.
#[test]
fn a_saved_module_serves_only_the_run_it_was_checked_in() {
    let host = three_modules();
    let saved = Saved::default();
    let one = ["/p/src/A/Use.fl"];
    let same = |_: &mut Project| {};
    assert_eq!(taken_by(&host, &saved, "one", &one, same), 0);
    assert_eq!(
        taken_by(&host, &saved, "one", &one, same),
        2,
        "Base and Use"
    );
    assert_eq!(
        taken_by(&host, &saved, "two", &one, same),
        0,
        "another compiler"
    );
    let effect = |p: &mut Project| p.config.build.effect = Backend::Effect;
    assert_eq!(
        taken_by(&host, &saved, "one", &one, effect),
        0,
        "stages by class"
    );
    let both = ["/p/src/A/Use.fl", "/p/src/A/Other.fl"];
    assert_eq!(
        taken_by(&host, &saved, "one", &both, same),
        0,
        "one more module"
    );
    assert_eq!(taken_by(&host, &saved, "one", &both, same), 3);
}

fn package_preflight_diagnostics(diagnostics: &[Diagnostic], sources: &SourceMap) -> Vec<String> {
    diagnostics
        .iter()
        .map(|d| {
            let span = d.span.expect("source span");
            let source = sources.get(span.file);
            let start = usize::try_from(span.start).expect("start fits");
            let end = usize::try_from(span.end).expect("end fits");
            format!(
                "{} {} {}:{start}-{end} {:?}",
                d.code,
                d.message,
                source.path,
                &source.text[start..end]
            )
        })
        .collect()
}

/// Target preflight follows re-exports into cached source-package bodies even
/// when only the selected environment changes between checks.
#[test]
fn a_saved_package_keeps_target_preflight_identical_to_a_cold_check() {
    const DB: &str = "module Lib.Db exposing (run)\nservice Db =\n  query : String ->{} String\nrun : String ->{Db} String\nlet run key = Db.query key\n";
    let host = Memory(BTreeMap::from([
        (
            PathBuf::from("/lib/flick.toml"),
            "schema-version = 1\n[project]\nkind = \"lib\"\n".into(),
        ),
        (PathBuf::from("/lib/src/Lib/Db.fl"), DB.into()),
        (
            PathBuf::from("/p/src/App/Root.fl"),
            "module App.Root exposing (run)\nimport Lib.Db exposing (run)\n".into(),
        ),
    ]));
    let check = |environment: Option<&str>, packages: Option<&PackageCache<'_>>| {
        let mut sources = SourceMap::default();
        let mut project = library(&mut sources);
        project.config.dependencies.push(Dependency {
            name: "lib".into(),
            source: Source::Path("../lib".into()),
            span: Span::new(project.config_file, 0, 0),
        });
        project.config.build.environment = environment.map(|e| e.parse().expect("environment"));
        let output = run(
            &host,
            &mut sources,
            Some(&project),
            &[PathBuf::from("/p/src/App/Root.fl")],
            Mode::Check,
            None,
            packages,
        );
        (output, sources)
    };
    let identity = |_: &Path, _: Option<&str>| Some("immutable-package-v1".to_owned());
    let empty = PackageCache {
        salt: "target-preflight",
        identity: &identity,
        saved: &|_| None,
    };
    let (node, _) = check(Some("js/node"), Some(&empty));
    assert!(node.diagnostics.is_empty(), "{:?}", node.diagnostics);
    let saved: BTreeMap<_, Vec<CachedModule>> = node
        .fresh
        .into_iter()
        .map(|p| {
            let bytes = serde_json::to_vec(&p.modules).expect("package serializes");
            (
                p.key,
                serde_json::from_slice(&bytes).expect("package round trips"),
            )
        })
        .collect();
    assert_eq!(saved.values().flatten().count(), 1, "Lib.Db was saved");
    let taken = AtomicUsize::new(0);
    let load = |key: &str| {
        let modules = saved.get(key)?.clone();
        taken.fetch_add(modules.len(), Ordering::Relaxed);
        Some(modules)
    };
    let cache = PackageCache {
        salt: "target-preflight",
        identity: &identity,
        saved: &load,
    };
    let (unselected, _) = check(None, Some(&cache));
    assert_eq!(stored_count(&taken), 1, "the package cache was read");
    assert!(
        unselected.diagnostics.is_empty(),
        "{:?}",
        unselected.diagnostics
    );
    assert!(unselected.fresh.is_empty(), "the package was not rechecked");
    assert!(
        unselected.compilation.modules.is_empty(),
        "unselected checks need no IR"
    );
    let (cold, cold_sources) = check(Some("js/browser"), None);
    let (warm, warm_sources) = check(Some("js/browser"), Some(&cache));
    assert_eq!(
        stored_count(&taken),
        2,
        "the same Node-warmed key was reused"
    );
    assert!(warm.fresh.is_empty(), "the package was not rechecked");
    let start = DB.find("Db.query").expect("operation use");
    assert_eq!(
        package_preflight_diagnostics(&cold.diagnostics, &cold_sources),
        vec![format!(
            "FLICK-E020 unhandled effect Db on js/browser /lib/src/Lib/Db.fl:{start}-{} \"Db.query\"",
            start + "Db.query".len()
        )]
    );
    assert_eq!(
        package_preflight_diagnostics(&warm.diagnostics, &warm_sources),
        package_preflight_diagnostics(&cold.diagnostics, &cold_sources)
    );
}
