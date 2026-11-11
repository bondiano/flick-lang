/// What a run in `mode` says that a person reads, an importer is checked
/// against and a build emits: the diagnostics with their places, and each
/// module's interface with the IR a compilation lowered it to.
type Said = (Vec<String>, BTreeMap<String, (Interface, Option<ir::Module>)>);

fn said(host: &Memory, mode: Mode, cache: Option<&mut ModuleCache>) -> Said {
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    let roots = [PathBuf::from("/p/src/A/Use.fl")];
    let output = run(host, &mut sources, Some(&project), &roots, mode, cache, None);
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
/// and for a build.
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
    for mode in [Mode::Analyze, Mode::Check, Mode::Compile] {
        let mut cache = ModuleCache::default();
        let mut compiled = 0;
        for (what, [base, mid, last], checks) in steps {
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
        }
        // a compilation with a cache still gives every module it lowered
        assert_eq!(compiled > 0, mode == Mode::Compile);
    }
}
