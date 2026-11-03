use std::collections::BTreeMap;
use std::io;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use super::config::{Build, Config, Kind};
#[cfg(feature = "native-project")]
use super::discover;
use super::{Host, Project, check, compile, is_module_name, module_name, module_path, normalize};
use crate::diagnostics::{Severity, codes};
use crate::source::SourceMap;
use crate::types::Thresholds;

struct Memory(BTreeMap<PathBuf, String>);

impl Host for Memory {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        #[cfg(not(feature = "native-project"))]
        assert!(
            path.extension().is_some_and(|ext| ext == "fl"),
            "unexpected native project read: {}",
            path.display()
        );
        self.0
            .get(path)
            .map(|text| text.clone().into_bytes())
            .ok_or_else(|| ErrorKind::NotFound.into())
    }

    fn display(&self, path: &Path) -> String {
        path.strip_prefix("/p")
            .unwrap_or(path)
            .display()
            .to_string()
    }
}

fn memory(files: &[(&str, &str)]) -> Memory {
    Memory(
        files
            .iter()
            .map(|(p, t)| (Path::new("/p").join(p), (*t).to_owned()))
            .collect(),
    )
}

#[cfg(feature = "native-project")]
/// Checks `roots` of the project at `/p`; each diagnostic as `code path text-at-span: message`.
fn run(host: &Memory, roots: &[&str]) -> Vec<String> {
    let mut sources = SourceMap::default();
    let project: Option<Project> =
        discover(host, &mut sources, Path::new("/p"), None).expect("valid config");
    report(host, &mut sources, project.as_ref(), roots)
}

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

fn run_library(host: &Memory, roots: &[&str]) -> Vec<String> {
    let mut sources = SourceMap::default();
    let project = library(&mut sources);
    report(host, &mut sources, Some(&project), roots)
}

fn report(
    host: &Memory,
    sources: &mut SourceMap,
    project: Option<&Project>,
    roots: &[&str],
) -> Vec<String> {
    let roots: Vec<PathBuf> = roots.iter().map(|r| Path::new("/p").join(r)).collect();
    check(host, sources, project, &roots)
        .iter()
        .filter(|d| !d.unnecessary())
        .map(|d| {
            let at = d.span.map_or(String::new(), |s| {
                let file = sources.get(s.file);
                format!("{} `{}`", file.path, &file.text[s.range()])
            });
            format!("{} {at}: {}", d.code, d.message)
        })
        .collect()
}

#[test]
fn diagnostics_do_not_depend_on_import_order() {
    let left = "module A.Left exposing (..)\nfind : Float\nlet find = 1\ntype T = | T\n";
    let right = "module A.Right exposing (..)\nfind : Float\nlet find = 2\ntype T = | T\n";
    let main =
        |imports: &str| format!("module A.Main exposing (x)\n{imports}\nx : Float\nlet x = find\n");
    let orders = [
        "import A.Left exposing (..)\nimport A.Right exposing (..)\nimport A.Left as Q\nimport A.Right as Q exposing (T)\nimport A.Left exposing (T)",
        "import A.Right as Q exposing (T)\nimport A.Left exposing (T)\nimport A.Right exposing (..)\nimport A.Left as Q\nimport A.Left exposing (..)",
    ];
    let results: Vec<Vec<String>> = orders
        .iter()
        .map(|imports| {
            let main = main(imports);
            let host = memory(&[
                ("src/A/Left.fl", left),
                ("src/A/Right.fl", right),
                ("src/A/Main.fl", &main),
            ]);
            run_library(&host, &["src/A/Main.fl"])
        })
        .collect();
    assert_eq!(results[0], results[1]);
    assert_eq!(results[0].len(), 3, "{:#?}", results[0]);
    assert!(
        results[0]
            .iter()
            .any(|d| d.contains("`find` is ambiguous: it is exposed by `A.Left`, `A.Right`"))
    );
}

#[test]
fn named_paths_still_check_their_imports() {
    let host = memory(&[
        (
            "src/A/Main.fl",
            "module A.Main exposing (x)\nimport A.Util exposing (y)\nx : Float\nlet x = y\n",
        ),
        (
            "src/A/Util.fl",
            "module A.Util exposing (y, z)\ny : Float\nlet y = 1\n",
        ),
        ("src/A/Unrelated.fl", "module A.Unrelated exposing (nope)\n"),
    ]);
    assert_eq!(
        run_library(&host, &["src/A/Main.fl"]),
        [
            "FLICK-R011 src/A/Util.fl `z`: cannot export `z`: it is neither defined in this module nor imported"
        ]
    );
}

#[test]
fn an_invalid_import_does_not_export_its_open_type_variables() {
    let host = memory(&[
        (
            "src/A/Bad.fl",
            "module A.Bad exposing (x)\nf : forall a. Unit ->{;} Option a\nlet f () = None\nlet x = f ()\n",
        ),
        (
            "src/A/Use.fl",
            "module A.Use exposing (y)\nimport A.Bad exposing (x)\ny : Option Int\nlet y = x\n",
        ),
    ]);
    let diagnostics = run_library(&host, &["src/A/Use.fl"]);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.contains("the type of `x` is not fully known")),
        "{diagnostics:?}"
    );
}

#[test]
fn test_modules_see_sources_but_not_the_reverse() {
    let host = memory(&[
        (
            "src/A/Lib.fl",
            "module A.Lib exposing (f)\nimport A.Helper\nf : Float\nlet f = 1\n",
        ),
        (
            "test/A/Helper.fl",
            "module A.Helper exposing (h)\nh : Float\nlet h = 1\n",
        ),
        (
            "test/A/LibTest.fl",
            "module A.LibTest exposing (t)\nimport A.Lib exposing (f)\nimport A.Helper exposing (h)\nt : Float\nlet t = f\n",
        ),
    ]);
    let result = run_library(&host, &["test/A/LibTest.fl", "src/A/Lib.fl"]);
    assert_eq!(
        result,
        ["FLICK-R003 src/A/Lib.fl `A.Helper`: unknown module `A.Helper`"]
    );
}

#[test]
fn without_a_project_imports_resolve_from_the_module_root() {
    let host = memory(&[
        (
            "lib/A/Main.fl",
            "module A.Main exposing (x)\nimport A.Util exposing (y)\nx : Float\nlet x = y\n",
        ),
        (
            "lib/A/Util.fl",
            "module A.Util exposing (y)\ny : Float\nlet y = 1\n",
        ),
        (
            "loose.fl",
            "module Loose exposing (z)\nimport A.Util\nz : Float\nlet z = 1\n",
        ),
    ]);
    let mut sources = SourceMap::default();
    assert_eq!(
        report(&host, &mut sources, None, &["lib/A/Main.fl"]),
        Vec::<String>::new()
    );
    let loose = report(&host, &mut sources, None, &["loose.fl"]);
    assert_eq!(loose.len(), 1);
    assert!(
        loose[0].starts_with("FLICK-R003 loose.fl `A.Util`"),
        "{loose:?}"
    );
}

#[test]
fn standalone_compilation_checks_the_real_import_graph() {
    let mut host = memory(&[
        (
            "lib/A/Main.fl",
            "module A.Main exposing (x)\nimport A.Util exposing (y)\nx : Int\nlet x = y\n",
        ),
        (
            "lib/A/Util.fl",
            "module A.Util exposing (y)\nimport List\ny : Int\nlet y = List.length []\n",
        ),
    ]);
    let roots = [PathBuf::from("/p/lib/A/Main.fl")];
    let mut sources = SourceMap::default();
    let (diagnostics, compilation) = compile(&host, &mut sources, None, &roots);
    assert!(
        diagnostics.iter().all(|d| d.severity != Severity::Error),
        "{diagnostics:?}"
    );
    assert_eq!(
        compilation
            .modules
            .iter()
            .map(|m| m.name.as_str())
            .collect::<Vec<_>>(),
        ["A.Main", "A.Util"]
    );

    for (body, expected) in [
        ("import Missing\ny : Int\nlet y = 1\n", codes::R003),
        ("import A.Main\ny : Int\nlet y = 1\n", codes::R006),
        ("y : Int\nlet y = \"wrong\"\n", codes::T003),
    ] {
        host.0.insert(
            PathBuf::from("/p/lib/A/Util.fl"),
            format!("module A.Util exposing (y)\n{body}"),
        );
        let (diagnostics, compilation) = compile(&host, &mut SourceMap::default(), None, &roots);
        assert!(
            diagnostics.iter().any(|d| d.code == expected),
            "{diagnostics:?}"
        );
        assert!(compilation.modules.is_empty());
    }
}

#[test]
fn programmatic_projects_keep_entry_path_runner_and_backend_checks() {
    use super::config::{Backend, Entry};
    use crate::source::Span;

    let mut host = memory(&[(
        "src/App/Main.fl",
        "module App.Main exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
    )]);
    let mut sources = SourceMap::default();
    let mut project = library(&mut sources);
    let span = Span::new(project.config_file, 0, 0);
    project.config.kind = Kind::App;
    project.config.entries.push(Entry {
        name: "main".into(),
        module: "App.Main".into(),
        span,
        runner: Some(("Runner.Browser".into(), span)),
        client: false,
    });
    let roots = ["src/App/Main.fl"];
    assert!(report(&host, &mut sources, Some(&project), &roots).is_empty());
    for (source, expected) in [
        (
            "module App.Main exposing (main)\nmain : Float -> Unit\nlet main _ = ()\n",
            "FLICK-P026",
        ),
        (
            "module Wrong exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
            "FLICK-R001",
        ),
    ] {
        host.0
            .insert(PathBuf::from("/p/src/App/Main.fl"), source.into());
        let diagnostics = report(&host, &mut sources, Some(&project), &roots);
        assert!(
            diagnostics.iter().any(|d| d.starts_with(expected)),
            "{diagnostics:?}"
        );
    }
    project.config.entries[0].runner = None;
    project.config.build.effect = Backend::None;
    host.0.insert(
        PathBuf::from("/p/src/App/Main.fl"),
        "module App.Main exposing (main)\nmain : Unit ->{; AppError} Unit\nlet main () = ()\n"
            .into(),
    );
    let diagnostics = report(&host, &mut sources, Some(&project), &roots);
    assert!(
        diagnostics.iter().any(|d| d.starts_with("FLICK-P026")),
        "{diagnostics:?}"
    );
}

#[test]
fn module_names_follow_paths() {
    assert_eq!(
        module_name(Path::new("App/Users.fl")).as_deref(),
        Some("App.Users")
    );
    assert_eq!(module_name(Path::new("Main.fl")).as_deref(), Some("Main"));
    assert_eq!(module_name(Path::new("app/Users.fl")), None);
    assert_eq!(module_name(Path::new("App/Users.txt")), None);
    assert_eq!(module_name(Path::new("App/my-file.fl")), None);
    assert_eq!(module_path("App.Users"), Path::new("App/Users.fl"));
    assert!(is_module_name("App.Users") && !is_module_name("App..Users") && !is_module_name("app"));
}

#[test]
fn normalize_is_lexical() {
    assert_eq!(normalize(Path::new("/a/./b/../c")), Path::new("/a/c"));
    assert_eq!(normalize(Path::new("../x")), Path::new("../x"));
}

#[cfg(feature = "native-project")]
#[test]
fn entry_main_is_pure_or_an_effect_a_runner_can_start() {
    const APP: &str = "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n";
    let check = |main: &str| {
        let source = format!(
            "module App.Main exposing (main)\n\ntype error Boom = Boom\n\nservice Greeter =\n  greet : String ->{{}} String\n\n{main}"
        );
        let host = memory(&[("flick.toml", APP), ("src/App/Main.fl", &source)]);
        run(&host, &["src/App/Main.fl"])
    };
    let unit = |row: &str| format!("main : Unit {row} Unit\nlet main () = ()\n");
    assert_eq!(check(&unit("->")), Vec::<String>::new());
    assert_eq!(check(&unit("->{; AppError}")), Vec::<String>::new());
    assert_eq!(check(&unit("->{}")), Vec::<String>::new());
    let wrong = [
        (
            "main : Float -> Unit\nlet main _ = ()\n",
            "has type `Float -> Unit`",
        ),
        (&unit("->{Greeter;}"), "needs `{Greeter}`"),
        (&unit("->{; Boom}"), "can fail with `{Boom}`"),
    ];
    for (main, message) in wrong {
        let diagnostics = check(main);
        assert!(
            matches!(&diagnostics[..], [d] if d.starts_with("FLICK-P026 src/App/Main.fl `main`") && d.contains(message)),
            "{main}: {diagnostics:?}"
        );
    }
}

#[cfg(feature = "native-project")]
#[test]
fn without_effects_nothing_runs_at_runtime() {
    const APP: &str = "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n[build]\neffect = \"none\"\n";
    let check = |body: &str| {
        let source = format!("module App.Main exposing (main)\n\n{body}");
        let host = memory(&[("flick.toml", APP), ("src/App/Main.fl", &source)]);
        run(&host, &["src/App/Main.fl"])
    };
    assert_eq!(
        check("main : Unit -> Unit\nlet main () = ()\n"),
        Vec::<String>::new()
    );
    let wrong = [
        (
            "main : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line \"x\"\n",
            "FLICK-E001 src/App/Main.fl `Platform.stdout_line",
        ),
        (
            "main : Unit -> Unit\nlet main () =\n  let _ = task { 1 }\n  ()\n",
            "FLICK-E008 src/App/Main.fl `task`",
        ),
        (
            "main : Unit ->{; AppError} Unit\nlet main () = ()\n",
            "FLICK-P026 src/App/Main.fl `main`",
        ),
    ];
    for (body, expected) in wrong {
        let diagnostics = check(body);
        assert!(
            diagnostics.iter().any(|d| d.starts_with(expected)),
            "{body}: {diagnostics:?}"
        );
    }
}

#[cfg(all(feature = "native-project", feature = "server-stdlib"))]
#[test]
fn a_runner_gives_the_type_of_main() {
    let check = |runner: &str, main: &str| {
        let config = format!(
            "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App.Main\"\nrunner = \"{runner}\"\n"
        );
        let source = format!("module App.Main exposing (main)\n\n{main}");
        let host = memory(&[("flick.toml", &config), ("src/App/Main.fl", &source)]);
        run(&host, &["src/App/Main.fl"])
    };
    let effect = "main : Unit ->{; AppError} Unit\nlet main () = ()\n";
    let pure = "main : Unit -> Unit\nlet main () = ()\n";
    assert_eq!(check("Runner.Node", effect), Vec::<String>::new());
    assert_eq!(check("Runner.Browser", pure), Vec::<String>::new());
    let wrong = [
        (
            "Runner.Node",
            "main : Float -> Unit\nlet main _ = ()\n",
            "FLICK-P026 src/App/Main.fl `main`: entry `main` is not a `Runner.Node.Main`",
        ),
        (
            "Runner.Cloudflare",
            effect,
            "FLICK-P026 src/App/Main.fl `main`: entry `main` is not a `Runner.Cloudflare.Main`",
        ),
        (
            "Runner.Nope",
            effect,
            "FLICK-P026 flick.toml `\"Runner.Nope\"`: runner `Runner.Nope` not found",
        ),
        (
            "List",
            effect,
            "FLICK-P026 flick.toml `\"List\"`: runner `List` does not export `Main` and `run`",
        ),
    ];
    for (runner, main, expected) in wrong {
        let diagnostics = check(runner, main);
        assert!(
            matches!(&diagnostics[..], [d] if d.starts_with(expected)),
            "{runner}: {diagnostics:?}"
        );
    }
}

#[cfg(all(feature = "native-project", feature = "server-stdlib"))]
#[test]
fn main_may_need_the_services_its_runner_provides() {
    let check = |main: &str| {
        let config = "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App.Main\"\nrunner = \"Runner.Cloudflare\"\n";
        let source = format!(
            "module App.Main exposing (main)\n\nimport Runner.Cloudflare exposing (Fetch, Env)\nimport Http.ServerResponse exposing (ServerResponse)\n\nservice Other\n\n{main}"
        );
        let host = memory(&[("flick.toml", config), ("src/App/Main.fl", &source)]);
        run(&host, &["src/App/Main.fl"])
    };
    let answer = "Http.ServerResponse.text \"hi\" Http.ServerResponse.defaults";
    for services in ["Env", ""] {
        assert_eq!(
            check(&format!(
                "main : Fetch ->{{{services}; AppError}} ServerResponse\nlet main _ = {answer}\n"
            )),
            Vec::<String>::new(),
            "{services}"
        );
    }
    let reads = format!(
        "main : Fetch ->{{Env; AppError}} ServerResponse\nlet main _ =\n  let _ = Runner.Cloudflare.var \"KEY\"\n  {answer}\n"
    );
    assert_eq!(check(&reads), Vec::<String>::new());
    let diagnostics = check(&format!(
        "main : Fetch ->{{Env, Other; AppError}} ServerResponse\nlet main _ = {answer}\n"
    ));
    assert!(
        matches!(&diagnostics[..], [d] if d.starts_with("FLICK-P026 src/App/Main.fl `main`: entry `main` is not a `Runner.Cloudflare.Main`")),
        "{diagnostics:?}"
    );
}
