//! Project commands without Node: scaffolding, the build cache and its
//! manifest, `clean`, toolchain checks, the pnpm adapter's refusals,
//! `explain`, `doc` and `inspect`.

#[path = "cli_project/artifacts.rs"]
mod artifacts;
use crate::common;
#[path = "cli_project/doc.rs"]
mod doc_tests;
#[path = "cli_project/foreign_inputs.rs"]
mod foreign_inputs;
#[path = "cli_project/inspect.rs"]
mod inspect_tests;
#[path = "cli_project/output_guards.rs"]
mod output_guards;
#[path = "cli_project/variance.rs"]
mod variance_tests;

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Output, Stdio};

use common::{APP, flick_in, project};
use flick_core::build::runner_path;
use flick_core::diagnostics::codes::REGISTRY;
use flick_core::implicit_packages::IMPLICIT_PACKAGES;

const MAIN: &str = "module App.Main exposing (main)\n\nmain : Unit ->{Host.Console; AppError} Unit\nlet main () = Platform.stdout_line \"hi\"\n";

fn code(out: &Output) -> Option<i32> {
    out.status.code()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn new_creates_a_project_and_init_refuses_before_writing() {
    let dir = project("cli_new", &[]);
    fs::create_dir_all(&dir).expect("dir");
    let out = flick_in(&dir, &["new", "demo", "--template", "lib"]);
    assert_eq!(code(&out), Some(0), "{out:?}");
    let manifest = fs::read_to_string(dir.join("demo/package.json")).expect("manifest");
    for pin in [
        "\"pnpm@",
        "\"@flick-lang/runtime\": \"0.1.0\"",
        "\"compiler\": \"0.1.0\"",
    ] {
        assert!(manifest.contains(pin), "{pin} in {manifest}");
    }
    assert!(
        !manifest.contains("\"@flick-lang/effect-ts\"") && !manifest.contains("\"effect\":"),
        "a library's default runtime needs no Effect adapter"
    );
    assert_eq!(code(&flick_in(dir.join("demo"), &["check"])), Some(0));
    assert_eq!(code(&flick_in(&dir, &["new", "demo"])), Some(2));

    let half = project(
        "cli_init",
        &[("flick.toml", "mine\n"), ("src/App/Main.fl", "mine\n")],
    );
    let out = flick_in(&half, &["init"]);
    assert_eq!(code(&out), Some(2));
    let err = stderr(&out);
    assert!(
        err.contains("`flick.toml` exists") && err.contains("`src/App/Main.fl` exists"),
        "{err}"
    );
    assert!(!half.join("package.json").exists(), "nothing is written");
    assert_eq!(
        fs::read_to_string(half.join("flick.toml")).expect("kept"),
        "mine\n"
    );
}

#[test]
fn every_code_has_an_explanation() {
    let dir = project("cli_explain", &[]);
    fs::create_dir_all(&dir).expect("dir");
    for info in REGISTRY {
        let name = info.code.to_string();
        let out = flick_in(&dir, &["explain", &name]);
        assert_eq!(code(&out), Some(0), "{name}");
        assert!(
            String::from_utf8_lossy(&out.stdout).starts_with(&format!("{name}: {}", info.title))
        );
    }
    assert_eq!(code(&flick_in(&dir, &["explain", "FLICK-Z999"])), Some(2));
}

#[test]
fn explain_reads_a_plugins_code_in_its_package() {
    let dir = project(
        "cli_explain_plugin",
        &[
            (
                "app/flick.toml",
                &format!("{APP}\n[dependencies]\ntool = {{ path = \"../tool\" }}\n"),
            ),
            ("app/src/App/Main.fl", MAIN),
            ("tool/explain/X001.md", "What X001 means.\n"),
            ("tool/explain/secret.md", "not a code\n"),
        ],
    );
    let app = dir.join("app");
    let out = flick_in(&app, &["explain", "tool/x001"]);
    assert_eq!(code(&out), Some(0), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "tool/X001\n\nWhat X001 means.\n"
    );
    for wrong in [
        "tool/X002",
        "tool/../tool/explain/secret",
        "tool/secret",
        "other/X001",
    ] {
        let out = flick_in(&app, &["explain", wrong]);
        assert_eq!(code(&out), Some(2), "{wrong}: {out:?}");
        assert!(
            stderr(&out).contains("FLICK-P018"),
            "{wrong}: {}",
            stderr(&out)
        );
    }
}

#[test]
fn expand_prints_the_code_of_each_plugin_block() {
    let main = "module App.Main exposing (main)\n\nimport Cli\n\ntool : Cli.Command {Host.Console} none\nlet tool = cli \"tool\" {\n  flag loud : Bool\n  handle fun { loud } -> Platform.stdout_line (show loud)\n}\n\nmain : Unit ->{Host.Console; AppError} Unit\nlet main () = Platform.stdout_line \"hi\"\n";
    let dir = project(
        "cli_expand",
        &[("flick.toml", APP), ("src/App/Main.fl", main)],
    );
    let out = flick_in(&dir, &["expand"]);
    assert_eq!(code(&out), Some(0), "{out:?}");
    let shown = String::from_utf8_lossy(&out.stdout);
    assert!(
        shown.starts_with("src/App/Main.fl:6:12: plugin `std`\n"),
        "{shown}"
    );
    assert!(
        shown.contains("(Cli.switch : (Cli.Multi _ Bool))"),
        "{shown}"
    );
    assert!(
        shown.contains("Platform.stdout_line (show loud)"),
        "{shown}"
    );
}

/// The number of cached builds.
fn cached(dir: &Path) -> usize {
    fs::read_dir(dir.join(".flick/cache")).map_or(0, Iterator::count)
}

fn dist(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.join("dist")];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).expect("dist").flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p.strip_prefix(dir).expect("inside").display().to_string();
                out.push((rel, fs::read_to_string(&p).unwrap_or_default()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn the_cache_key_follows_every_input_and_a_miss_changes_nothing() {
    let dir = project(
        "cli_cache",
        &[("flick.toml", APP), ("src/App/Main.fl", MAIN)],
    );
    let build = |args: &[&str]| {
        let mut all = vec!["build"];
        all.extend(args);
        let out = flick_in(&dir, &all);
        assert_eq!(code(&out), Some(0), "{out:?}");
    };
    build(&[]);
    let first = dist(&dir);
    assert_eq!(cached(&dir), 1);
    build(&[]);
    assert_eq!(cached(&dir), 1, "the same inputs hit the cache");
    assert_eq!(dist(&dir), first, "a hit writes what the build wrote");
    build(&["--profile", "release"]);
    assert_eq!(cached(&dir), 2, "the profile is in the key");
    build(&["--typescript"]);
    assert_eq!(cached(&dir), 3, "backend options are in the key");
    fs::write(
        dir.join("flick.toml"),
        format!("{APP}\n[build]\nsolver-steps = 5000\n"),
    )
    .expect("write");
    build(&[]);
    assert_eq!(cached(&dir), 4, "the effective configuration is in the key");
    fs::write(
        dir.join("package.json"),
        "{ \"name\": \"cache-demo\", \"dependencies\": { \"dep\": \"1.0.0\" } }",
    )
    .expect("write");
    build(&[]);
    assert_eq!(cached(&dir), 5);
    fs::create_dir_all(dir.join("node_modules/dep")).expect("dir");
    fs::write(
        dir.join("node_modules/dep/package.json"),
        "{ \"name\": \"dep\", \"version\": \"1.0.0\" }",
    )
    .expect("write");
    build(&[]);
    assert_eq!(cached(&dir), 6, "an installed dependency is in the key");
    // a miss gives what a hit gives
    let hit = dist(&dir);
    assert_eq!(code(&flick_in(&dir, &["clean", "--cache"])), Some(0));
    build(&[]);
    assert_eq!(dist(&dir), hit);
}

#[test]
fn clean_keeps_user_files_and_running_generations() {
    let dir = project(
        "cli_clean",
        &[
            ("flick.toml", APP),
            ("src/App/Main.fl", MAIN),
            ("dist/notes.txt", "mine"),
        ],
    );
    assert_eq!(code(&flick_in(&dir, &["build"])), Some(0));
    for (generation, pid) in [
        ("7", process::id().to_string()),
        ("8", "999999999".to_owned()),
    ] {
        let g = dir.join(".flick/run").join(generation);
        fs::create_dir_all(&g).expect("dir");
        fs::write(g.join(".lock"), pid).expect("lock");
        fs::write(g.join(runner_path(false)), "").expect("file");
    }
    assert_eq!(code(&flick_in(&dir, &["clean"])), Some(0));
    assert!(!dir.join("dist/App/Main.js").exists());
    assert!(!dir.join("dist/_flick").exists());
    assert_eq!(
        fs::read_to_string(dir.join("dist/notes.txt")).expect("kept"),
        "mine"
    );
    assert!(
        dir.join(".flick/run/7/.lock").exists(),
        "a running generation stays with its lock"
    );
    assert!(!dir.join(".flick/run/8").exists(), "an abandoned one goes");
    assert!(
        dir.join(".flick/cache").exists(),
        "the cache stays without --cache"
    );
}

#[test]
fn a_wrong_pnpm_or_a_missing_lockfile_changes_nothing() {
    let manifest = "{ \"name\": \"pins\", \"packageManager\": \"pnpm@1.0.0\" }\n";
    let dir = project(
        "cli_pnpm",
        &[("flick.toml", APP), ("package.json", manifest)],
    );
    let out = flick_in(&dir, &["add", "left-pad"]);
    assert_eq!(code(&out), Some(2), "{out:?}");
    let err = stderr(&out);
    assert!(err.contains("pins pnpm 1.0.0"), "{err}");
    assert!(
        err.contains("corepack prepare pnpm@1.0.0 --activate"),
        "{err}"
    );
    assert!(err.contains("packageManager"), "{err}");
    assert_eq!(
        fs::read_to_string(dir.join("package.json")).expect("kept"),
        manifest
    );
    assert!(!dir.join("pnpm-lock.yaml").exists());
    assert!(!dir.join("node_modules/left-pad").exists());

    let pinned = format!(
        "{{ \"name\": \"locked\", \"packageManager\": \"pnpm@{}\" }}\n",
        pnpm_version()
    );
    let dir = project(
        "cli_locked",
        &[("flick.toml", APP), ("package.json", &pinned)],
    );
    let out = flick_in(&dir, &["--diagnostics", "json", "install", "--locked"]);
    assert_eq!(code(&out), Some(2), "{out:?}");
    let events: Vec<serde_json::Value> = stderr(&out)
        .lines()
        .map(|l| serde_json::from_str(l).expect("NDJSON"))
        .collect();
    assert!(events.iter().all(|e| e["schema-version"] == 1));
    assert!(
        events.iter().any(|e| e["code"] == "FLICK-P017"),
        "{events:?}"
    );
    assert_eq!(
        fs::read_to_string(dir.join("package.json")).expect("kept"),
        pinned
    );
    assert!(!dir.join("pnpm-lock.yaml").exists());
    assert!(!dir.join("node_modules/left-pad").exists());
}

/// Runs the built `flick` in `dir` with `PATH` as given, for a toolchain
/// that lacks a program.
fn flick_with_path(dir: &Path, path: &str, args: &[&str]) -> Output {
    process::Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(dir)
        .env("PATH", path)
        .env_remove("NO_COLOR")
        .output()
        .expect("flick runs")
}

/// A `pnpm` on `PATH` of its own: `--version` answers `version`, and every
/// other invocation is recorded in the returned file, so a test can see
/// whether the version gate let a command through without a real install.
#[cfg(unix)]
fn fake_pnpm(dir: &Path, version: &str) -> (PathBuf, PathBuf) {
    let bin = dir.join("fake-bin");
    fs::create_dir_all(&bin).expect("dir");
    let marker = dir.join("pnpm-calls.txt");
    common::write_script(
        &bin.join("pnpm"),
        &format!(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo \"{version}\"; exit 0; fi\n\
             echo \"$*\" >> \"{}\"\nexit 0\n",
            marker.display()
        ),
    );
    (bin, marker)
}

/// The `[dependencies]` entries that keep the implicit packages local, so a
/// test fetches nothing and needs no `git`.
fn local_std() -> String {
    let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages");
    let mut manifest =
        "schema-version = 1\n\n[tools]\n\"flick/js\" = [\"pnpm\"]\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n\n[dependencies]\n"
            .to_owned();
    for name in IMPLICIT_PACKAGES {
        writeln!(
            manifest,
            "{name} = {{ path = \"{}\" }}",
            packages.join(name).display()
        )
        .expect("manifest string");
    }
    manifest
}

include!("cli_project/pnpm.rs");

#[test]
fn doctor_reports_without_failing_on_a_missing_content_mapper() {
    let dir = project("cli_doctor", &[("flick.toml", APP)]);
    let out = flick_in(&dir, &["doctor"]);
    let text = String::from_utf8_lossy(&out.stdout);
    for check in [
        "compiler: flick",
        "store:",
        "git:",
        "plugins:",
        "backend: flick/js@",
        "Node.js:",
        "pnpm:",
        "@flick-lang/runtime:",
        "effect:",
        "Content Mapper:",
    ] {
        assert!(text.contains(check), "`{check}` in\n{text}");
    }
    assert!(
        !text.contains("Go:"),
        "JS project doctor must not mention Go:\n{text}"
    );
    let mapper = text
        .lines()
        .find(|l| l.contains("Content Mapper"))
        .expect("a line");
    assert!(!mapper.starts_with("FAIL"), "{mapper}");
    let git = text.lines().find(|l| l.contains("git:")).expect("a line");
    assert!(!git.starts_with("FAIL"), "{git}");
}

#[test]
fn doctor_notes_a_missing_git_rather_than_failing_on_it() {
    let dir = project("cli_doctor_git", &[("flick.toml", APP)]);
    let empty = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli_doctor_path");
    fs::create_dir_all(&empty).expect("dir");
    let out = flick_with_path(&dir, empty.to_str().expect("utf-8"), &["doctor"]);
    let text = String::from_utf8_lossy(&out.stdout);
    let git = text
        .lines()
        .find(|l| l.contains("git:"))
        .expect("a git line");
    assert!(git.starts_with("note"), "{git}");
    assert!(
        git.contains("FLICK_STD_PATH"),
        "how to work without git:\n{git}"
    );
    assert!(
        !text.contains("Go:"),
        "JS project doctor must not mention Go:\n{text}"
    );
    // only the toolchain the selected backend cannot do without fails, and git is not one
    let failed: Vec<&str> = text.lines().filter(|l| l.starts_with("FAIL")).collect();
    assert_eq!(failed.len(), 2, "{failed:?}");
    assert!(failed.iter().any(|l| l.contains("Node.js")), "{failed:?}");
    assert!(failed.iter().any(|l| l.contains("pnpm")), "{failed:?}");
    assert_eq!(code(&out), Some(2), "{out:?}");
}

#[test]
fn module_store_persists_plugin_blocks_and_derives_across_cli_runs() {
    let schema = "module App.Schema exposing (Users, User, user_json)\n\nimport Codec\n\nsql table Users \"users\" {\n  id : bigint primary key as BigInt\n  name : text not null as String\n}\n\ntype alias User = { name : String } deriving Schema\n\nuser_json : PureCodec Json User\nlet user_json = codec User.schema {\n  name = field \"name\" String.json\n}\n";
    let ui = "module App.View exposing (badge)\n\nimport Ui\n\nbadge : String -> Ui\nlet badge label = ui { text label }\n";
    let api = "module App.Api exposing (Public)\n\nimport HttpApi.Derive exposing (..)\n\ntype alias Public = {\n  ping : Get \"/ping\" {} Unit {};\n} deriving HttpApi\n";
    let main = "module App.Main exposing (main, greet, query, check_api)\n\nimport App.Api exposing (Public)\nimport App.Schema exposing (Users)\nimport App.View exposing (badge)\n\ncheck_api : Public -> Public\nlet check_api p = p\n\nlet query = sql {\n  from u in Users\n  select { id = u.id; name = u.name }\n}\n\ngreet : Cli.Command {Host.Console} none\nlet greet = cli \"greet\" {\n  handle fun {} -> Platform.stdout_line \"hi\"\n}\n\nmain : Unit ->{Host.Console; AppError} Unit\nlet main () =\n  let _ = badge \"ok\"\n  Platform.stdout_line \"ok\"\n";
    let dir = project(
        "cli_module_store_plugins",
        &[
            ("app/flick.toml", APP),
            ("app/src/App/Schema.fl", schema),
            ("app/src/App/View.fl", ui),
            ("app/src/App/Api.fl", api),
            ("app/src/App/Main.fl", main),
        ],
    );
    let cache = dir.join("cache");
    let check_timed = || {
        let out = process::Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(["--timings", "check"])
            .current_dir(dir.join("app"))
            .env("XDG_CACHE_HOME", &cache)
            .env("NO_COLOR", "1")
            .output()
            .expect("flick runs");
        let stderr_text = String::from_utf8_lossy(&out.stderr).into_owned();
        let checked: usize = (stderr_text.lines())
            .filter_map(|l| {
                l.strip_prefix("Slowest of ")?
                    .strip_suffix(" modules checked")
            })
            .map(|n| n.parse::<usize>().expect("module count"))
            .sum();
        (out.status.code(), stderr_text, checked)
    };

    // Cold run checks all 4 modules and persists them to ModuleStore.
    let (status, err, checked) = check_timed();
    assert_eq!((status, checked), (Some(0), 4), "{err}");
    let saved_count = fs::read_dir(cache.join("flick/modules"))
        .expect("modules cache dir")
        .count();
    assert_eq!(
        saved_count, 4,
        "all 4 plugin/derive modules persisted: {err}"
    );

    // Warm run in a separate CLI process restores all 4 modules without checking.
    let (status, err, checked) = check_timed();
    assert_eq!((status, checked), (Some(0), 0), "{err}");

    // Modifying `App.Schema` invalidates `App.Schema`, `App.Main` (SQL query
    // & interface), and `App.View` (its `ui` block shares `std`'s collected
    // `EditorBlock::state`), while `App.Api` (`deriving HttpApi`) stays cached.
    let schema_v2 = schema.replace(
        "name : text not null as String\n",
        "name : text not null as String\n  role : text not null as String\n",
    );
    fs::write(dir.join("app/src/App/Schema.fl"), schema_v2).expect("write");
    let (status, err, checked) = check_timed();
    assert_eq!((status, checked), (Some(0), 3), "{err}");

    // Subsequent run restores all 4 again.
    let (status, err, checked) = check_timed();
    assert_eq!((status, checked), (Some(0), 0), "{err}");

    // Modifying `App.View`'s `ui` block body (keeping its interface and `sql`
    // state identical) re-checks only `App.View`.
    let ui_v2 = ui.replace("ui { text label }", "ui { text (\"[\" ++ label ++ \"]\") }");
    fs::write(dir.join("app/src/App/View.fl"), ui_v2).expect("write");
    let (status, err, checked) = check_timed();
    assert_eq!((status, checked), (Some(0), 1), "{err}");

    // Final warm run restores all 4.
    let (status, err, checked) = check_timed();
    assert_eq!((status, checked), (Some(0), 0), "{err}");
}

fn pnpm_version() -> String {
    process::Command::new("pnpm")
        .env("npm_config_manage_package_manager_versions", "false")
        .env("PNPM_CONFIG_PM_ON_FAIL", "ignore")
        .env("COREPACK_ENABLE_PROJECT_SPEC", "0")
        .env("COREPACK_ENABLE_AUTO_PIN", "0")
        .env("COREPACK_ENABLE_NETWORK", "0")
        .env("COREPACK_DEFAULT_TO_LATEST", "0")
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}
