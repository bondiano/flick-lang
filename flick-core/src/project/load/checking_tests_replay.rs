/// What a run in `mode` says that a person reads, an importer is checked
/// against and a build emits: the diagnostics with their places, and each
/// module's interface with the IR a compilation lowered it to.
type Said = (Vec<String>, BTreeMap<String, (Interface, Option<ir::Module>)>);

fn said(host: &Memory, mode: Mode, cache: Option<&mut ModuleCache>) -> Said {
    said_in(host, mode, cache, 0, |_| {})
}

/// [`said`] as a process of its own says it: its first `shift` files are
/// others, so each module has another file than in another process, and
/// `edit` changes the project.
fn said_in(
    host: &Memory,
    mode: Mode,
    cache: Option<&mut ModuleCache>,
    shift: usize,
    edit: impl Fn(&mut Project),
) -> Said {
    let roots = [PathBuf::from("/p/src/A/Use.fl")];
    said_for(host, &roots, mode, cache, shift, edit)
}

fn said_for(
    host: &Memory,
    roots: &[PathBuf],
    mode: Mode,
    cache: Option<&mut ModuleCache>,
    shift: usize,
    edit: impl Fn(&mut Project),
) -> Said {
    let mut sources = SourceMap::default();
    for n in 0..shift {
        sources.add(format!("/elsewhere/{n}.fl"), "");
    }
    let mut project = library(&mut sources);
    edit(&mut project);
    let output = run(host, &mut sources, Some(&project), roots, mode, cache, None);
    let mut diagnostics: Vec<String> = output
        .diagnostics
        .iter()
        .map(|d| {
            let at = d
                .span
                .map(|s| format!("{}:{}-{}", sources.get(s.file).path, s.start, s.end));
            format!("{} {} {at:?}", d.code, d.message)
        })
        .collect();
    diagnostics.sort();
    // an editor's run with a cache keeps no IR, by design: its modules are
    // the ones it analyzed
    let modules = if mode == Mode::Analyze {
        (output.analyzed.into_iter())
            .map(|m| (m.name, (m.interface, None)))
            .collect()
    } else {
        (output.compilation.modules.into_iter())
            .map(|m| (m.name, (m.interface, Some(m.ir))))
            .collect()
    };
    (diagnostics, modules)
}

/// A session of edits over three modules, each step one edit after the step
/// before it: with one cache kept through all of them, every step says what
/// a run without a cache says of the same texts, for an editor, for a check
/// and for a build. So does each step as a process of its own that takes
/// what the processes before it saved, with its modules in other files; a
/// build after the checks takes only modules saved with their IR.
#[test]
fn a_cache_kept_through_a_session_of_edits_changes_nothing_a_run_says() {
    const BASE: &str = "module A.Base exposing (x)\nx : Float\nlet x = 1\n";
    const MID: &str = "module A.Mid exposing (twice)\nimport A.Base exposing (x)\ntwice : Float\nlet twice = x + x\n";
    const USE: &str =
        "module A.Use exposing (y)\nimport A.Mid exposing (twice)\ny : Float\nlet y = twice + 1\n";
    let renamed = "module A.Base exposing (z)\nz : Float\nlet z = 1\n";
    // what changes, the three texts, and whether they check
    let steps: [(&str, [&str; 3], bool); 13] = [
        ("the start", [BASE, MID, USE], true),
        (
            "a body, with the interface as it was",
            ["module A.Base exposing (x)\nx : Float\nlet x = 1 + 2\n", MID, USE],
            true,
        ),
        (
            "a comment",
            ["module A.Base exposing (x)\n// one\nx : Float\nlet x = 1 + 2\n", MID, USE],
            true,
        ),
        (
            "another type of an export",
            ["module A.Base exposing (x)\nx : String\nlet x = \"one\"\n", MID, USE],
            false,
        ),
        ("the type back", [BASE, MID, USE], true),
        (
            "a syntax error",
            ["module A.Base exposing (x)\nx : Float\nlet x = (1\n", MID, USE],
            false,
        ),
        ("the error fixed", [BASE, MID, USE], true),
        (
            "one more export that nothing uses",
            ["module A.Base exposing (x, w)\nx : Float\nlet x = 1\nw : Float\nlet w = 2\n", MID, USE],
            true,
        ),
        ("a renamed export", [renamed, MID, USE], false),
        (
            "its importer follows",
            [
                renamed,
                "module A.Mid exposing (twice)\nimport A.Base exposing (z)\ntwice : Float\nlet twice = z + z\n",
                USE,
            ],
            true,
        ),
        (
            "the module between changes the type it exports",
            [
                renamed,
                "module A.Mid exposing (twice)\nimport A.Base exposing (z)\ntwice : String\nlet twice = show z\n",
                USE,
            ],
            false,
        ),
        (
            "the last module imports nothing",
            [renamed, MID, "module A.Use exposing (y)\ny : Float\nlet y = 1\n"],
            true,
        ),
        ("the start again", [BASE, MID, USE], true),
    ];
    #[cfg(feature = "native-project")]
    let (saved, taken): (Saved, Arc<AtomicUsize>) = Default::default();
    for mode in [Mode::Analyze, Mode::Check, Mode::Compile] {
        let mut cache = ModuleCache::default();
        let mut compiled = 0;
        #[cfg(feature = "native-project")]
        let taken_before = stored_count(&taken);
        for (n, (what, [base, mid, last], checks)) in steps.into_iter().enumerate() {
            let host = Memory(BTreeMap::from([
                (PathBuf::from("/p/src/A/Base.fl"), base.to_owned()),
                (PathBuf::from("/p/src/A/Mid.fl"), mid.to_owned()),
                (PathBuf::from("/p/src/A/Use.fl"), last.to_owned()),
            ]));
            let warm = said(&host, mode, Some(&mut cache));
            let cold = said(&host, mode, None);
            assert_eq!(cold.0.is_empty(), checks, "{what}: {:?}", cold.0);
            assert_eq!(warm.0, cold.0, "{what}: the diagnostics");
            assert_eq!(warm.1, cold.1, "{what}: the interfaces and the IR");
            compiled += warm.1.values().filter(|(_, ir)| ir.is_some()).count();
            #[cfg(not(feature = "native-project"))]
            let _ = n;
            #[cfg(feature = "native-project")]
            {
                let shift = n % 3;
                let mut apart = ModuleCache::stored(memory_store("one", &saved, &taken));
                let apart = said_in(&host, mode, Some(&mut apart), shift, |_| {});
                let alone = said_in(&host, mode, None, shift, |_| {});
                assert_eq!(apart, alone, "{what}: a process of its own");
            }
        }
        // an editor's run saves and takes nothing; a check and a build take
        // the modules no step changed
        #[cfg(feature = "native-project")]
        assert_eq!(stored_count(&taken) > taken_before, mode != Mode::Analyze);
        // a compilation with a cache still gives every module it lowered
        assert_eq!(compiled > 0, mode == Mode::Compile);
    }
}

#[test]
fn the_module_cache_retains_only_loaded_paths_in_both_maps() {
    const USE_BASE: &str =
        "module A.Use exposing (y)\nimport A.Base exposing (x)\ny : Int\nlet y = x\n";
    const USE_RENAMED: &str =
        "module A.Use exposing (y)\nimport A.Next exposing (x)\ny : Int\nlet y = x\n";
    let modules = |files: &[(&str, &str)]| {
        Memory(
            files
                .iter()
                .map(|(name, text)| (PathBuf::from(format!("/p/src/A/{name}.fl")), (*text).into()))
                .collect(),
        )
    };
    let before = modules(&[
        ("Use", USE_BASE),
        ("Base", "module A.Base exposing (x)\nx : Int\nlet x = 1\n"),
        ("Extra", "module A.Extra exposing (z)\nz : Int\nlet z = 2\n"),
    ]);
    let after_rename = modules(&[
        ("Use", USE_RENAMED),
        ("Next", "module A.Next exposing (x)\n\nx : Int\nlet x = 1\n"),
    ]);
    let other = modules(&[(
        "Other",
        "module A.Other exposing (x)\nx : Int\nlet x = unknown\n",
    )]);
    let use_file = PathBuf::from("/p/src/A/Use.fl");
    let extra = PathBuf::from("/p/src/A/Extra.fl");
    let other_file = PathBuf::from("/p/src/A/Other.fl");
    let both_roots = [use_file.clone(), extra];
    let use_only = [use_file];
    let other_only = [other_file];
    let steps: [(_, &[_], &[_], _); 6] = [
        (&before, &both_roots, &["Base", "Extra", "Use"], true),
        (&before, &both_roots, &["Base", "Extra", "Use"], true),
        (&before, &use_only, &["Base", "Use"], true),
        (&after_rename, &use_only, &["Next", "Use"], true),
        (&other, &other_only, &["Other"], false),
        (&other, &[], &[], true),
    ];
    let mut cache = ModuleCache::default();
    for (host, roots, retained, valid) in steps {
        let warm = said_for(host, roots, Mode::Analyze, Some(&mut cache), 0, |_| {});
        let cold = said_for(host, roots, Mode::Analyze, None, 0, |_| {});
        assert_eq!(warm.0.is_empty(), valid, "{:?}", warm.0);
        assert_eq!(warm, cold, "cached diagnostics, interfaces and spans");
        let paths: Vec<_> = retained
            .iter()
            .map(|name| PathBuf::from(format!("/p/src/A/{name}.fl")))
            .collect();
        let expected = paths.iter().map(PathBuf::as_path).collect();
        let (checked, parsed) = cache.cached_paths();
        assert_eq!(checked, expected, "checked cache");
        assert_eq!(parsed, expected, "parse cache");
    }
}
