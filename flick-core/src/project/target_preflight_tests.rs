use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use super::target_preflight::{self, CheckOptions, gated_service, handles};
use crate::diagnostics::{Diagnostic, codes};
use crate::ir::tests::check_text;
use crate::project::config::BackendEnvironment;
use crate::project::{Compilation, Compiled, ResolvedEnvironment};
use crate::source::SourceMap;

#[cfg(feature = "native-project")]
#[path = "target_preflight_env_tests.rs"]
mod env_tests;

const DB_PROVIDED: &str = r#"module App.Main exposing (main)

service Db =
  query : String ->{} String

main : Unit ->{} Unit
let main () =
  provide Db ({ query = fun key -> key }) {
    ignore (Db.query "x")
  }
"#;

const DB_UNUSED: &str = r"module App.Main exposing (main)

service Db =
  query : String ->{} String

unused : String ->{Db} String
let unused key = Db.query key

main : Unit ->{} Unit
let main () = ()
";

const DB_EXPORT: &str = r"module App.Main exposing (run)

service Db =
  query : String ->{} String

run : String ->{Db} String
let run key = Db.query key
";

fn environment(pair: &str) -> BackendEnvironment {
    pair.parse().expect("environment")
}

fn diagnostics(text: &str, pair: &str, app: bool) -> Vec<Diagnostic> {
    let (ir, interface) = check_text("App.Main", text).expect("the source checks");
    let mut sources = SourceMap::default();
    let file = sources.add("App/Main.fl", text);
    let compilation = Compilation {
        modules: vec![Compiled {
            name: ir.name.clone(),
            path: PathBuf::from("/app/src/App/Main.fl"),
            file,
            test: false,
            package: None,
            imports: vec![],
            ir,
            declarations: Arc::new(interface.clone()),
            interface,
            expansion: None,
        }],
        ..Compilation::default()
    };
    let entries = [("main".into(), "App.Main".into(), None)];
    let pair_env = environment(pair);
    let mut tags = BTreeSet::from([pair_env.backend.clone(), pair_env.platform.clone()]);
    if pair_env.backend == "js" {
        tags.insert("rt".into());
    }
    let env = ResolvedEnvironment {
        backend_id: format!("flick/{}", pair_env.backend),
        backend: pair_env.backend,
        platform: pair_env.platform,
        host_package: None,
        tags,
        explicit_environment: true,
        ui: None,
    };
    let mut found = Vec::new();
    target_preflight::check(
        &compilation,
        &CheckOptions {
            entries: if app { &entries } else { &[] },
            runners: &[],
            by_class: true,
            for_tests: false,
            test_paths: &[],
        },
        &sources,
        &env,
        &mut found,
    );
    found
}

#[test]
fn inventory_handles_database_on_node_not_browser() {
    assert!(handles(&environment("js/node"), "Db"));
    assert!(handles(&environment("js/node"), "Database"));
    assert!(!handles(&environment("js/browser"), "Db"));
    assert_eq!(gated_service("App.Main.Db"), Some("Db"));
    assert_eq!(gated_service("Database.Database"), Some("Database"));
}

#[test]
fn browser_rejects_a_reachable_unhandled_db_at_its_use() {
    // Open Db on main is refused by entry typing; a library export is the
    // reachable unhandled case with an exact requirement span.
    let source = DB_EXPORT;
    let found = diagnostics(source, "js/browser", false);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].code, codes::E020);
    assert_eq!(found[0].message, "unhandled effect Db on js/browser");
    let span = found[0].span.expect("requirement span");
    assert_eq!(&source[span.start as usize..span.end as usize], "Db.query");
}

#[test]
fn node_accepts_the_same_reachable_db_export() {
    let found = diagnostics(DB_EXPORT, "js/node", false);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn browser_accepts_a_locally_provided_db() {
    let found = diagnostics(DB_PROVIDED, "js/browser", true);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn unused_db_does_not_fail_a_reachable_only_executable() {
    // main cannot leave Db open; unused stays unreachable from main.
    let found = diagnostics(DB_UNUSED, "js/browser", true);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_library_export_policy_checks_public_db_use() {
    let found = diagnostics(DB_EXPORT, "js/browser", false);
    assert_eq!(found[0].code, codes::E020);
}

#[cfg(feature = "native-project")]
mod environments {
    use std::collections::BTreeMap;
    use std::io;
    use std::path::{Path, PathBuf};

    use crate::diagnostics::{Diagnostic, Severity, codes};
    use crate::project::{BackendAxis, BackendTags, Host, Project, check, compile, discover};
    use crate::source::SourceMap;

    struct MemHost {
        files: BTreeMap<PathBuf, Vec<u8>>,
        backends: Vec<BackendTags>,
    }

    impl MemHost {
        fn new(files: &[(&str, &str)], backends: Vec<BackendTags>) -> Self {
            let files = files
                .iter()
                .map(|(p, t)| (PathBuf::from(p), t.as_bytes().to_vec()))
                .collect();
            Self { files, backends }
        }
    }

    impl Host for MemHost {
        fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
            self.files
                .get(path)
                .cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.display().to_string()))
        }

        fn display(&self, path: &Path) -> String {
            path.display().to_string()
        }

        fn backend_tags(&self, _project: &Project) -> Vec<BackendTags> {
            self.backends.clone()
        }
    }

    fn lua_backend() -> BackendTags {
        BackendTags {
            id: "community/lua".into(),
            short: "lua".into(),
            tags: vec!["lua".into()],
            axes: vec![
                BackendAxis {
                    name: "flavor".into(),
                    tags: vec!["luau".into(), "luajit".into()],
                    default: Some("luau".into()),
                    exclusive: true,
                },
                BackendAxis {
                    name: "capabilities".into(),
                    tags: vec!["ffi".into()],
                    default: None,
                    exclusive: false,
                },
            ],
            environments: vec![("lua".into(), "standalone".into())],
        }
    }

    fn check_project(
        files: &[(&str, &str)],
        roots: &[&str],
        backends: Vec<BackendTags>,
    ) -> (Vec<Diagnostic>, SourceMap) {
        let host = MemHost::new(files, backends);
        let mut sources = SourceMap::default();
        let project = discover(&host, &mut sources, Path::new("/app"), None)
            .expect("manifest parses")
            .expect("project found");
        let roots: Vec<PathBuf> = roots.iter().map(PathBuf::from).collect();
        let diags = check(&host, &mut sources, Some(&project), &roots);
        (diags, sources)
    }

    fn lua_host_files<'a>(app_toml: &'a str, main_src: &'a str) -> Vec<(&'a str, &'a str)> {
        const ROBLOX_TOML: &str = "\
schema-version = 1
[package]
name = \"roblox-host\"
version = \"0.1.0\"
[project]
kind = \"lib\"
[host]
name = \"roblox\"
backend = \"lua\"
requires = \"luau\"
tags = [\"sandbox\"]
";
        const OPENRESTY_TOML: &str = "\
schema-version = 1
[package]
name = \"openresty-host\"
version = \"0.1.0\"
[project]
kind = \"lib\"
[host]
name = \"openresty\"
backend = \"lua\"
requires = \"luajit && ffi\"
tags = [\"nginx\"]
";
        const ROBLOX_MOD: &str = "@when \"lua && sandbox\"\nmodule Host.Roblox exposing (tick)\ntick : Unit -> Int\nlet tick () = 1\n";
        const OPENRESTY_MOD: &str = "@when \"lua && nginx\"\nmodule Host.OpenResty exposing (tick)\ntick : Unit -> Int\nlet tick () = 2\n";
        vec![
            ("/app/flick.toml", app_toml),
            ("/app/src/App/Main.fl", main_src),
            ("/roblox/flick.toml", ROBLOX_TOML),
            ("/roblox/src/Host/Roblox.fl", ROBLOX_MOD),
            ("/openresty/flick.toml", OPENRESTY_TOML),
            ("/openresty/src/Host/OpenResty.fl", OPENRESTY_MOD),
        ]
    }

    #[test]
    fn custom_backend_axes_and_two_host_packages_select_modules_and_enforce_requires() {
        let main_roblox = "\
module App.Main exposing (main)
import Host.Roblox
import Host.OpenResty

unused : Unit -> Int
let unused () = Host.OpenResty.tick ()

main : Unit -> Unit
let main () =
  let _ = Host.Roblox.tick ()
  ()
";
        let app_toml = |env: &str, extra: &str| {
            format!(
                "schema-version = 1\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n\
                 [dependencies]\nroblox-host = {{ path = \"../roblox\" }}\n\
                 openresty-host = {{ path = \"../openresty\" }}\n\
                 [build]\nenvironment = \"{env}\"\n{extra}"
            )
        };

        // 1. lua/roblox (default flavor = luau) accepts Host.Roblox and ignores unused Host.OpenResty.
        let toml = app_toml("lua/roblox", "");
        let files = lua_host_files(&toml, main_roblox);
        let (diags, _) = check_project(&files, &["/app/src/App/Main.fl"], vec![lua_backend()]);
        assert!(
            !diags.iter().any(|d| d.severity == Severity::Error),
            "{diags:?}"
        );

        // 2. lua/openresty without luajit + ffi fails with FLICK-P065.
        let toml = app_toml("lua/openresty", "");
        let files = lua_host_files(&toml, main_roblox);
        let (diags, _) = check_project(&files, &["/app/src/App/Main.fl"], vec![lua_backend()]);
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, codes::P065);

        // 3. lua/openresty with flavor = "luajit" & ffi = true rejects reachable Host.Roblox.tick with FLICK-P064.
        let toml = app_toml(
            "lua/openresty",
            "[target.lua]\nflavor = \"luajit\"\nffi = true\n",
        );
        let files = lua_host_files(&toml, main_roblox);
        let (diags, sources) =
            check_project(&files, &["/app/src/App/Main.fl"], vec![lua_backend()]);
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, codes::P064);
        let span = errors[0].span.expect("use span");
        assert_eq!(
            &sources.get(span.file).text[span.range()],
            "Host.Roblox.tick"
        );
        assert!(
            errors[0].message.contains("App.Main.main")
                && errors[0].message.contains("Host.Roblox.tick"),
            "{}",
            errors[0].message
        );
    }

    #[test]
    fn package_when_gates_all_modules_of_a_dependency_for_reachable_uses_only() {
        let dep_toml = "\
schema-version = 1
[package]
name = \"node-only\"
version = \"0.1.0\"
when = \"js && node\"
[project]
kind = \"lib\"
";
        let dep_mod = "module NodeOnly exposing (pid)\npid : Unit -> Int\nlet pid () = 42\n";
        let app_toml = "\
schema-version = 1
[project]
kind = \"app\"
entry = \"App.Main\"
[dependencies]
node-only = { path = \"../node-only\" }
[build]
environment = \"js/browser\"
";
        let unused_main = "module App.Main exposing (main)\nimport NodeOnly\nmain : Unit -> Unit\nlet main () = ()\n";
        let (diags, _) = check_project(
            &[
                ("/app/flick.toml", app_toml),
                ("/app/src/App/Main.fl", unused_main),
                ("/node-only/flick.toml", dep_toml),
                ("/node-only/src/NodeOnly.fl", dep_mod),
            ],
            &["/app/src/App/Main.fl"],
            vec![],
        );
        assert!(
            !diags.iter().any(|d| d.severity == Severity::Error),
            "unused import must not fail: {diags:?}"
        );

        let used_main = "module App.Main exposing (main)\nimport NodeOnly\nmain : Unit -> Unit\nlet main () =\n  let _ = NodeOnly.pid ()\n  ()\n";
        let (diags, sources) = check_project(
            &[
                ("/app/flick.toml", app_toml),
                ("/app/src/App/Main.fl", used_main),
                ("/node-only/flick.toml", dep_toml),
                ("/node-only/src/NodeOnly.fl", dep_mod),
            ],
            &["/app/src/App/Main.fl"],
            vec![],
        );
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].code, codes::P064);
        assert!(
            diags[0].message.contains("node-only"),
            "{}",
            diags[0].message
        );
        let span = diags[0].span.expect("span");
        assert_eq!(&sources.get(span.file).text[span.range()], "NodeOnly.pid");
    }

    #[test]
    fn unknown_tags_unknown_hosts_duplicate_hosts_and_unknown_backends_report_exact_codes() {
        // 1. Unknown backend in `[build] environment` -> FLICK-P059.
        let (diags, sources) = check_project(
            &[
                (
                    "/app/flick.toml",
                    "schema-version = 1\n[project]\nkind = \"lib\"\n[build]\nenvironment = \"ruby/mri\"\n",
                ),
                (
                    "/app/src/Lib.fl",
                    "module Lib exposing (x)\nx : Int\nlet x = 1\n",
                ),
            ],
            &["/app/src/Lib.fl"],
            vec![],
        );
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].code, codes::P059);
        let span = diags[0].span.expect("env span");
        assert_eq!(&sources.get(span.file).text[span.range()], "\"ruby/mri\"");

        // 2. Unknown host -> FLICK-P065 with suggestion.
        let (diags, _) = check_project(
            &[
                (
                    "/app/flick.toml",
                    "schema-version = 1\n[project]\nkind = \"lib\"\n[build]\nenvironment = \"js/nod\"\n",
                ),
                (
                    "/app/src/Lib.fl",
                    "module Lib exposing (x)\nx : Int\nlet x = 1\n",
                ),
            ],
            &["/app/src/Lib.fl"],
            vec![],
        );
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].code, codes::P065);
        assert!(format!("{:?}", diags[0].fixes).contains("js/node"));

        // 3. Tag typo in `@when`, `[package] when`, and `[host] requires` -> FLICK-P066 at exact tag spans.
        let lib_toml = "\
schema-version = 1
[package]
name = \"demo\"
version = \"0.1.0\"
when = \"js && nodde\"
[project]
kind = \"lib\"
[host]
name = \"custom\"
backend = \"js\"
requires = \"node\"
";
        let lib_fl = "@when \"js && browzer\"\nmodule Lib exposing (x)\nx : Int\nlet x = 1\n";
        let (diags, sources) = check_project(
            &[("/app/flick.toml", lib_toml), ("/app/src/Lib.fl", lib_fl)],
            &["/app/src/Lib.fl"],
            vec![],
        );
        let p066: Vec<(&str, &str)> = diags
            .iter()
            .filter(|d| d.code == codes::P066)
            .map(|d| {
                let s = d.span.expect("tag span");
                (
                    sources.get(s.file).path.as_str(),
                    &sources.get(s.file).text[s.range()],
                )
            })
            .collect();
        assert_eq!(
            p066,
            vec![
                ("/app/flick.toml", "nodde"),
                ("/app/flick.toml", "node"),
                ("/app/src/Lib.fl", "browzer"),
            ]
        );

        // 4. Duplicate `[host]` for same backend -> FLICK-P065.
        let host_a = "schema-version = 1\n[project]\nkind = \"lib\"\n[host]\nname = \"edge\"\nbackend = \"js\"\n";
        let host_b = "schema-version = 1\n[project]\nkind = \"lib\"\n[host]\nname = \"edge\"\nbackend = \"flick/js\"\n";
        let app_dup = "schema-version = 1\n[project]\nkind = \"lib\"\n[dependencies]\na = { path = \"../a\" }\nb = { path = \"../b\" }\n";
        let (diags, _) = check_project(
            &[
                ("/app/flick.toml", app_dup),
                (
                    "/app/src/Lib.fl",
                    "module Lib exposing (x)\nx : Int\nlet x = 1\n",
                ),
                ("/a/flick.toml", host_a),
                ("/b/flick.toml", host_b),
            ],
            &["/app/src/Lib.fl"],
            vec![],
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == codes::P065 && d.message.contains("duplicate host")),
            "{diags:?}"
        );
    }

    #[test]
    fn library_export_policy_skips_gated_modules_and_checks_active_exports() {
        let toml = "schema-version = 1\n[project]\nkind = \"lib\"\n[build]\ntarget = \"go\"\n";
        let js_mod = "@when \"js\"\nmodule Lib.JsOnly exposing (view)\nimport Ui\nview : Unit -> Ui\nlet view () = Ui.text \"hi\"\n";
        let go_mod =
            "@when \"go\"\nmodule Lib.GoOnly exposing (answer)\nanswer : Int\nlet answer = 42\n";
        let shared_ok =
            "module Lib.Shared exposing (value)\nimport Lib.JsOnly\nvalue : Int\nlet value = 1\n";
        let (diags, _) = check_project(
            &[
                ("/app/flick.toml", toml),
                ("/app/src/Lib/JsOnly.fl", js_mod),
                ("/app/src/Lib/GoOnly.fl", go_mod),
                ("/app/src/Lib/Shared.fl", shared_ok),
            ],
            &[
                "/app/src/Lib/JsOnly.fl",
                "/app/src/Lib/GoOnly.fl",
                "/app/src/Lib/Shared.fl",
            ],
            vec![],
        );
        assert!(
            !diags.iter().any(|d| d.severity == Severity::Error),
            "gated JS module skipped and unused import allowed: {diags:?}"
        );

        let shared_bad = "module Lib.Shared exposing (bad)\nimport Lib.JsOnly\nbad : Unit -> Ui\nlet bad () = Lib.JsOnly.view ()\n";
        let (diags, sources) = check_project(
            &[
                ("/app/flick.toml", toml),
                ("/app/src/Lib/JsOnly.fl", js_mod),
                ("/app/src/Lib/Shared.fl", shared_bad),
            ],
            &["/app/src/Lib/JsOnly.fl", "/app/src/Lib/Shared.fl"],
            vec![],
        );
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].code, codes::P064);
        let span = diags[0].span.expect("use span");
        assert_eq!(
            &sources.get(span.file).text[span.range()],
            "Lib.JsOnly.view"
        );
    }

    #[test]
    fn own_host_library_without_package_sets_host_package() {
        let toml = "schema-version = 1\n[project]\nkind = \"lib\"\n\
            [host]\nname = \"edge\"\nbackend = \"js\"\ntags = [\"worker\"]\n\
            [build]\nenvironment = \"js/edge\"\n";
        let lib = "@when \"js && worker\"\nmodule Edge exposing (ok)\nok : Bool\nlet ok = true\n";
        let host = MemHost::new(
            &[("/app/flick.toml", toml), ("/app/src/Edge.fl", lib)],
            vec![],
        );
        let mut sources = SourceMap::default();
        let project = discover(&host, &mut sources, Path::new("/app"), None)
            .expect("manifest parses")
            .expect("project");
        let (diags, compiled) = compile(
            &host,
            &mut sources,
            Some(&project),
            &[PathBuf::from("/app/src/Edge.fl")],
        );
        assert!(
            !diags.iter().any(|d| d.severity == Severity::Error),
            "{diags:?}"
        );
        let env = compiled.environment.expect("resolved environment");
        assert_eq!(env.host_package.as_deref(), Some("edge"));
        assert!(env.tags.contains("edge") && env.tags.contains("worker"));
    }
}
