use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::ModuleStore;
use crate::project::config::Backend;

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
            writer.lock().expect("the store").insert(key.to_owned(), bytes);
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
    assert_eq!(stored_count(&taken), 0, "the module was saved once, unchecked by `[lint]`");
    assert!(taken_by(&host, &saved, "one", &["/p/src/A/Use.fl"], |_| {}) > 0);
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
    assert_eq!(taken_by(&host, &saved, "one", &one, same), 2, "Base and Use");
    assert_eq!(taken_by(&host, &saved, "two", &one, same), 0, "another compiler");
    let effect = |p: &mut Project| p.config.build.effect = Backend::Effect;
    assert_eq!(taken_by(&host, &saved, "one", &one, effect), 0, "stages by class");
    let both = ["/p/src/A/Use.fl", "/p/src/A/Other.fl"];
    assert_eq!(taken_by(&host, &saved, "one", &both, same), 0, "one more module");
    assert_eq!(taken_by(&host, &saved, "one", &both, same), 3);
}
