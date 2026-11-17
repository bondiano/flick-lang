#[cfg(feature = "native-project")]
use super::checking::changed_inputs;

/// The three modules of a step, as `/p/src/A/{Base,Mid,Use}.fl`, with
/// `extra` as `/p/src/A/Extra.fl` when given.
fn three(base: &str, mid: &str, last: &str, extra: Option<&str>) -> Memory {
    let mut files = BTreeMap::from([
        (PathBuf::from("/p/src/A/Base.fl"), base.to_owned()),
        (PathBuf::from("/p/src/A/Mid.fl"), mid.to_owned()),
        (PathBuf::from("/p/src/A/Use.fl"), last.to_owned()),
    ]);
    if let Some(extra) = extra {
        files.insert(PathBuf::from("/p/src/A/Extra.fl"), extra.to_owned());
    }
    Memory(files)
}

const R_BASE: &str = "module A.Base exposing (x)\nx : Float\nlet x = 1\n";
const R_MID: &str =
    "module A.Mid exposing (twice)\nimport A.Base exposing (x)\ntwice : Float\nlet twice = x + x\n";
const R_USE: &str =
    "module A.Use exposing (y)\nimport A.Mid exposing (twice)\ny : Float\nlet y = twice + 1\n";
const R_BODY: &str = "module A.Base exposing (x)\nx : Float\nlet x = 1 + 2\n";
const R_EXPORT: &str =
    "module A.Base exposing (x, w)\nx : Float\nlet x = 1 + 2\nw : Float\nlet w = 2\n";
const R_EXTRA: &str = "module A.Extra exposing (e)\ne : Float\nlet e = 1\n";
const R_USE_EXTRA: &str = "module A.Use exposing (y)\nimport A.Mid exposing (twice)\nimport A.Extra exposing (e)\ny : Float\nlet y = twice + e\n";

/// What the run with `cache` did with each module, as `module: reason`.
fn reasons_of(host: &Memory, cache: &mut ModuleCache, edit: impl Fn(&mut Project)) -> Vec<String> {
    said_in(host, Mode::Check, Some(&mut *cache), 0, edit);
    (cache.reasons().into_iter())
        .map(|(module, reason)| format!("{module}: {reason}"))
        .collect()
}

/// A body edit rechecks its module alone, an edit of an interface its
/// importers too, each with the chain to the edit; a module added, an
/// import removed and the configuration name what changed.
#[test]
fn a_kept_cache_says_why_each_module_was_checked_again() {
    // an editor's cache is not asked, and compares no key again
    let mut quiet = ModuleCache::default();
    assert!(reasons_of(&three(R_BASE, R_MID, R_USE, None), &mut quiet, |_| {}).is_empty());
    let mut cache = ModuleCache::default();
    cache.explain(true);
    let mut step = |host: &Memory, edit: &dyn Fn(&mut Project)| reasons_of(host, &mut cache, edit);
    let same = |_: &mut Project| {};
    assert_eq!(
        step(&three(R_BASE, R_MID, R_USE, None), &same),
        ["A.Base: no earlier entry", "A.Mid: no earlier entry", "A.Use: no earlier entry"]
    );
    assert_eq!(
        step(&three(R_BODY, R_MID, R_USE, None), &same),
        ["A.Base: source", "A.Mid: reused", "A.Use: reused"]
    );
    assert_eq!(
        step(&three(R_EXPORT, R_MID, R_USE, None), &same),
        ["A.Base: source", "A.Mid: import A.Base ← source", "A.Use: reused"]
    );
    // the added module is loaded before A.Base, which takes another file
    assert_eq!(
        step(&three(R_EXPORT, R_MID, R_USE_EXTRA, Some(R_EXTRA)), &same),
        ["A.Base: file", "A.Extra: no earlier entry", "A.Mid: reused", "A.Use: source"]
    );
    // the removed module is no module of the run, and A.Base's file is the
    // one it had before
    assert_eq!(
        step(&three(R_EXPORT, R_MID, R_USE, Some(R_EXTRA)), &same),
        ["A.Base: file", "A.Mid: reused", "A.Use: source"]
    );
    assert_eq!(
        step(&three(R_EXPORT, R_MID, R_USE, None), &|p| p.config.no_let_assert = true),
        ["A.Base: config", "A.Mid: config", "A.Use: config"]
    );
}

/// The same edits, each run a process of its own with a store: an
/// interface edit names the chain, a module added names the set of the
/// project's modules for every module whose key lists them.
#[cfg(feature = "native-project")]
#[test]
fn a_store_says_why_each_module_was_checked_again() {
    let (saved, taken): (Saved, Arc<AtomicUsize>) = Default::default();
    let step = |host: &Memory| {
        let mut cache = ModuleCache::stored(memory_store("one", &saved, &taken));
        cache.explain(true);
        reasons_of(host, &mut cache, |_| {})
    };
    let start = three(R_BASE, R_MID, R_USE, None);
    assert_eq!(
        step(&start),
        ["A.Base: no earlier entry", "A.Mid: no earlier entry", "A.Use: no earlier entry"]
    );
    assert_eq!(
        step(&start),
        ["A.Base: taken from the store", "A.Mid: taken from the store", "A.Use: taken from the store"]
    );
    assert_eq!(
        step(&three(R_BODY, R_MID, R_USE, None)),
        ["A.Base: source", "A.Mid: taken from the store", "A.Use: taken from the store"]
    );
    assert_eq!(
        step(&three(R_EXPORT, R_MID, R_USE, None)),
        ["A.Base: source", "A.Mid: import A.Base ← source", "A.Use: taken from the store"]
    );
    assert_eq!(
        step(&three(R_EXPORT, R_MID, R_USE_EXTRA, Some(R_EXTRA))),
        [
            "A.Base: project modules",
            "A.Extra: no earlier entry",
            "A.Mid: project modules",
            "A.Use: import A.Extra; project modules; source",
        ]
    );
    // without the added module the keys are those saved before it
    assert_eq!(
        step(&three(R_EXPORT, R_MID, R_USE, Some(R_EXTRA))),
        ["A.Base: taken from the store", "A.Mid: taken from the store", "A.Use: taken from the store"]
    );
}

/// The saved and current parts of a store key, named as a person reads
/// them: the compiler, the plugins' inputs and each import by its module.
#[cfg(feature = "native-project")]
#[test]
fn the_parts_of_a_store_key_are_named_without_their_values() {
    let parts = |parts: &[(&str, &str)]| -> Vec<(String, String)> {
        (parts.iter())
            .map(|(label, digest)| ((*label).to_owned(), (*digest).to_owned()))
            .collect()
    };
    let saved = parts(&[
        ("salt", "1"),
        ("config", "1"),
        ("text", "1"),
        ("watching 0 plugin", "1"),
        ("import 1", "1"),
    ]);
    let now = parts(&[
        ("salt", "2"),
        ("config", "1"),
        ("text", "2"),
        ("watching 0 plugin", "2"),
        ("used 0", "1"),
        ("import 1", "2"),
    ]);
    assert_eq!(
        changed_inputs(&saved, &now, &["A.Mid", "A.Extra"]),
        ["compiler", "import A.Extra", "plugin inputs", "plugin output"]
    );
}

/// A cache that reuses a module whose text is unchanged, whatever its
/// imports became, says what a run without a cache does not: the replay of
/// edits catches the wrong reuse that its reasons call `reused`.
#[test]
fn a_replay_catches_a_cache_that_reuses_a_module_its_imports_changed() {
    let mut cache = ModuleCache::default();
    cache.stale = true;
    cache.explain(true);
    let start = three(R_BASE, R_MID, R_USE, None);
    let retyped = three(
        "module A.Base exposing (x)\nx : String\nlet x = \"one\"\n",
        R_MID,
        R_USE,
        None,
    );
    let first = said(&start, Mode::Check, Some(&mut cache));
    assert_eq!(first, said(&start, Mode::Check, None), "nothing to reuse yet");
    let warm = said(&retyped, Mode::Check, Some(&mut cache));
    let cold = said(&retyped, Mode::Check, None);
    assert_ne!(warm.0, cold.0, "the stale reuse hides the type error of A.Mid");
    let reasons = cache.reasons();
    assert!(reasons.contains(&("A.Mid".to_owned(), "reused".to_owned())), "{reasons:?}");
}
