//! `flick ffi import` and `flick ffi check` on a project with the fixture
//! package `tests/ffi/lookup-lib`, and on a corpus of real packages
//! installed for `tools/dts-importer`. Needs Node and `pnpm install`: run
//! with `--ignored` (part of `just e2e`).

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::symlink;
#[cfg(windows)]
use std::os::windows::fs::symlink_dir;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use flick_core::build::runner_path;
use flick_core::ffi::module_part;

const MAIN: &str = r#"module App.Main exposing (main)

import Ffi.LookupLib exposing (lookup)
import Ffi.LookupLib.Generated exposing (next_id, each)
import JsError exposing (JsError(..))

name_of : String ->{Async;} String
let name_of id =
  try
    match lookup id with
    | Some name -> name
    | None -> "nobody"
  with
  | JsError { message } -> message

main : Unit ->{Host.Console, Async; AppError} Unit
let main () =
  Platform.stdout_line (name_of "1")
  Platform.stdout_line (name_of "2")
  Platform.stdout_line (name_of "boom")
  let first = next_id ()
  Platform.stdout_line (show (first, next_id ()))
  each [1, 2] (fun _ -> ())
  Platform.stdout_line "visited"
"#;

const POLICY: &str = r#"[exports.lookup]
undefined = "none"
catch = "JsError"

[exports.each]
retention = "call"
"#;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn flick(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(dir)
        .env("NO_COLOR", "1")
        .output()
        .expect("flick runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn install(dir: &Path, version: &str) {
    let to = dir.join("node_modules/lookup-lib");
    let _ = fs::remove_dir_all(&to);
    fs::create_dir_all(&to).expect("dir");
    let from = root().join("tests/ffi/lookup-lib").join(version);
    for file in ["package.json", "index.d.ts", "index.js"] {
        fs::copy(from.join(file), to.join(file)).expect("copy fixture");
    }
}

fn project(name: &str) -> PathBuf {
    let dir = root().join("target/tmp/ffi_command").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/App")).expect("dir");
    fs::create_dir_all(dir.join("src/Ffi/LookupLib")).expect("dir");
    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n\n[tools]\n\"flick/js\" = [\"node\"]\n",
    )
    .expect("write");
    fs::write(
        dir.join("package.json"),
        "{ \"name\": \"app\", \"type\": \"module\", \"dependencies\": { \"@flick-lang/effect-ts\": \"*\" } }\n",
    )
    .expect("write");
    fs::write(dir.join("src/Ffi/LookupLib/policy.toml"), POLICY).expect("write");
    fs::write(dir.join("src/App/Main.fl"), MAIN).expect("write");
    install(&dir, "1.0.0");
    importer(&dir);
    dir
}

/// The repository's `.d.ts` importer, installed in the project at `dir`
/// as `pnpm add -D @flick-lang/dts-importer` would.
fn importer(dir: &Path) {
    let at = dir.join("node_modules/@flick-lang/dts-importer");
    fs::create_dir_all(at.parent().expect("scope")).expect("scope");
    let _ = fs::remove_file(&at);
    link(&root().join("tools/dts-importer"), &at);
}

#[cfg(unix)]
fn link(target: &Path, at: &Path) {
    symlink(target, at).expect("link the importer");
}

#[cfg(windows)]
fn link(target: &Path, at: &Path) {
    symlink_dir(target, at).expect("link the importer");
}

/// The fingerprint the generated module shows above `export`.
fn fingerprint(generated: &str, name: &str) -> String {
    let lines: Vec<&str> = generated.lines().collect();
    let at = lines
        .iter()
        .position(|l| {
            l.starts_with(&format!("{name} :"))
                || l.starts_with(&format!("foreign blocked {name} "))
        })
        .expect("binding");
    lines[at - 1].trim_start_matches("/// ").to_owned()
}

fn reviewed(provenance: &str, catch: bool) -> String {
    format!(
        "module Ffi.LookupLib exposing (lookup)\n\n\
         import JsError exposing (JsError)\n\n\
         lookup : String ->{{Async; JsError}} Option String\n\
         foreign lookup from \"lookup-lib\" \"lookup\"\n  arity 1\n{}  nullable\n  reviewed \"{provenance}\"\n",
        if catch {
            "  catch JsError.from_js\n"
        } else {
            ""
        }
    )
}

#[test]
#[ignore = "needs Node and `pnpm install`"]
fn import_review_update_and_check() {
    let dir = project("app");
    let out = flick(&dir, &["ffi", "import", "lookup-lib", "--out", "src/Ffi"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    insta::assert_snapshot!("lookup_report", text(&out.stdout));
    let generated_path = dir.join("src/Ffi/LookupLib/Generated.fl");
    let generated = fs::read_to_string(&generated_path).expect("generated");
    insta::assert_snapshot!("lookup_generated", generated);

    // the reviewed module re-exports what the app uses
    let provenance = fingerprint(&generated, "lookup");
    let lib = dir.join("src/Ffi/LookupLib.fl");
    fs::write(&lib, reviewed(&provenance, true)).expect("write");
    let check = flick(&dir, &["ffi", "check"]);
    assert!(check.status.success(), "{}", text(&check.stderr));

    let build = flick(&dir, &["build"]);
    assert!(build.status.success(), "{}", text(&build.stderr));
    let run = Command::new("node")
        .arg(dir.join("dist").join(runner_path(false)))
        .output()
        .expect("node runs");
    assert_eq!(
        text(&run.stdout),
        "Ada\nnobody\nTypeError: lookup failed\n(1, 2)\nvisited\n",
        "{}",
        text(&run.stderr)
    );

    // a blocked export: using it is an error, the others stay usable
    let blocked = MAIN
        .replace("(first, next_id ())", "(first, to_date \"x\")")
        .replace(
            "exposing (next_id, each)",
            "exposing (next_id, each, to_date)",
        );
    fs::write(dir.join("src/App/Main.fl"), blocked).expect("write");
    let check = flick(&dir, &["check"]);
    assert!(!check.status.success());
    let stderr = text(&check.stderr);
    assert!(stderr.contains("FLICK-F005"), "{stderr}");
    assert!(
        stderr.contains("`to_date` is blocked: unsupported: parameter `value`"),
        "{stderr}"
    );
    assert_eq!(stderr.matches("error[").count(), 1, "{stderr}");
    fs::write(dir.join("src/App/Main.fl"), MAIN).expect("write");

    // a binding without the policy its shape needs
    fs::write(&lib, reviewed(&provenance, false)).expect("write");
    let check = flick(&dir, &["ffi", "check"]);
    assert!(!check.status.success());
    assert!(text(&check.stderr).contains(
        "`lookup` returns a Promise, which may reject: its reviewed binding `lookup` needs `catch`"
    ));
    fs::write(&lib, reviewed(&provenance, true)).expect("write");

    // the package changes the shape of `lookup`
    install(&dir, "1.1.0");
    let stale = text(&flick(&dir, &["ffi", "check"]).stderr);
    assert!(stale.contains("FLICK-F006"), "{stale}");
    let before = fs::read_to_string(&lib).expect("reviewed");
    let again = flick(&dir, &["ffi", "import", "lookup-lib", "--out", "src/Ffi"]);
    assert!(again.status.success(), "{}", text(&again.stderr));
    assert_eq!(
        fs::read_to_string(&lib).expect("reviewed"),
        before,
        "an import never touches reviewed bindings"
    );
    let check = flick(&dir, &["ffi", "check"]);
    assert!(!check.status.success());
    let stderr = text(&check.stderr);
    assert!(!stderr.contains("FLICK-F006"), "{stderr}");
    assert!(
        stderr.contains("the declaration of `lookup` changed since `lookup` was reviewed against lookup-lib@1.0.0"),
        "{stderr}"
    );

    // `--select` limits the surface; asking for a blocked export is an error
    let select = flick(
        &dir,
        &[
            "ffi",
            "import",
            "lookup-lib",
            "--out",
            "src/Ffi",
            "--select",
            "nextId,toDate",
        ],
    );
    assert!(!select.status.success());
    let generated = fs::read_to_string(&generated_path).expect("generated");
    assert!(generated.contains("next_id :") && generated.contains("foreign blocked to_date"));
    assert!(!generated.contains("\nlookup :"));
    assert!(text(&select.stderr).contains("`toDate` is blocked"));
}

fn identity_binding(
    module_specifier: &str,
    package_name: &str,
    declaration: &str,
    version: &str,
    expected: u32,
) {
    let part = module_part(module_specifier);
    let dir = root().join("target/tmp/ffi_command/identity").join(&part);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/App")).expect("app dir");
    fs::create_dir_all(dir.join("src/Ffi")).expect("ffi dir");
    let installed = dir
        .join("node_modules")
        .join(module_specifier.split('/').next().expect("fixture package"));
    fs::create_dir_all(&installed).expect("package dir");
    importer(&dir);
    let fixture = root().join("tests/ffi/import-identity").join(package_name);
    let implementation = declaration.replace(".d.ts", ".js");
    for file in ["package.json", declaration, &implementation] {
        fs::copy(fixture.join(file), installed.join(file)).expect("copy fixture");
    }
    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n\n[tools]\n\"flick/js\" = [\"node\"]\n",
    )
    .expect("project");
    let dependency = if module_specifier == package_name
        || module_specifier.starts_with(&format!("{package_name}/"))
    {
        version.to_owned()
    } else {
        format!("npm:{package_name}@{version}")
    };
    fs::write(
        dir.join("package.json"),
        serde_json::json!({
            "name": "identity-consumer",
            "type": "module",
            "dependencies": { (installed.file_name().expect("package name").to_str().expect("UTF-8 package name")): dependency },
        })
        .to_string(),
    )
    .expect("package");
    fs::write(
        dir.join("src/App/Main.fl"),
        format!(
            "module App.Main exposing (main)\n\n\
             import Ffi.{part}.Generated as Generated\n\
             import Ffi.Reviewed as Reviewed\n\n\
             main : Unit ->{{Host.Console; AppError}} Unit\n\
             let main () = Platform.stdout_line (show (Generated.answer (), Reviewed.answer ()))\n"
        ),
    )
    .expect("consumer");
    let wrong = format!("{module_specifier}@{version} intentionally-wrong-fingerprint");
    let reviewed = format!(
        "module Ffi.Reviewed exposing (answer)\n\n\
         answer : Unit ->{{}} Float\n\
         foreign answer from \"{module_specifier}\" \"answer\"\n  arity 0\n  reviewed \"{wrong}\"\n"
    );
    let reviewed_path = dir.join("src/Ffi/Reviewed.fl");
    fs::write(&reviewed_path, &reviewed).expect("reviewed");
    // No Generated.fl exists yet: a reviewed binding alone must still be checked.
    let check = flick(&dir, &["ffi", "check"]);
    assert!(!check.status.success(), "{}", text(&check.stderr));
    assert!(
        text(&check.stderr).contains("FLICK-F007"),
        "{}",
        text(&check.stderr)
    );

    let import = flick(
        &dir,
        &["ffi", "import", module_specifier, "--out", "src/Ffi"],
    );
    assert!(import.status.success(), "{}", text(&import.stderr));
    let generated = fs::read_to_string(dir.join("src/Ffi").join(&part).join("Generated.fl"))
        .expect("generated");
    let provenance = fingerprint(&generated, "answer");
    assert!(provenance.starts_with(&format!("{module_specifier}@{version} sha256:")));
    fs::write(&reviewed_path, reviewed.replace(&wrong, &provenance)).expect("reviewed");
    let check = flick(&dir, &["ffi", "check"]);
    assert!(check.status.success(), "{}", text(&check.stderr));
    let build = flick(&dir, &["build", "--target", "js"]);
    assert!(build.status.success(), "{}", text(&build.stderr));
    let run = Command::new("node")
        .arg(dir.join("dist").join(runner_path(false)))
        .output()
        .expect("node runs");
    assert!(run.status.success(), "{}", text(&run.stderr));
    assert_eq!(
        text(&run.stdout),
        format!("({expected}, {expected})\n"),
        "{}",
        text(&run.stderr)
    );
}

#[test]
#[ignore = "needs Node and `pnpm install`"]
fn subpath_only_package_identity() {
    identity_binding(
        "subpath-package/feature",
        "subpath-package",
        "feature.d.ts",
        "1.0.0",
        41,
    );
}

#[test]
#[ignore = "needs Node and `pnpm install`"]
fn npm_alias_package_identity() {
    identity_binding(
        "package-alias",
        "original-package",
        "index.d.ts",
        "2.0.0",
        42,
    );
}

/// A project with the packages the importer's own tests install, and the
/// importer: its `node_modules` links each of theirs.
fn corpus_project() -> PathBuf {
    let dir = root().join("target/tmp/ffi_command/corpus");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).expect("dir");
    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"lib\"\n\n[tools]\n\"flick/js\" = [\"node\"]\n",
    )
    .expect("project");
    let installed = root().join("tools/dts-importer/node_modules");
    for entry in fs::read_dir(&installed)
        .expect("the importer's packages")
        .flatten()
    {
        let name = entry.file_name();
        if name != "@flick-lang" && name != ".bin" {
            let at = dir.join("node_modules").join(&name);
            fs::create_dir_all(at.parent().expect("modules")).expect("modules");
            link(&entry.path(), &at);
        }
    }
    importer(&dir);
    dir
}

/// Imports a selection of a real package; the report is the snapshot.
fn corpus(package: &str, select: &str) -> String {
    let dir = corpus_project();
    let out = flick(
        &dir,
        &["ffi", "import", package, "--out", "src", "--select", select],
    );
    let out_dir = dir.join("src");
    let generated = out_dir.join(module_part(package)).join("Generated.fl");
    let check = flick(&root(), &["check", &generated.to_string_lossy()]);
    assert!(check.status.success(), "{}", text(&check.stderr));
    text(&out.stdout)
}

#[test]
#[ignore = "needs Node and `pnpm install`"]
fn corpus_of_real_packages() {
    insta::assert_snapshot!(
        "date_fns",
        corpus(
            "date-fns",
            "isValid,secondsToHours,millisecondsToSeconds,format,addDays,closestIndexTo,intervalToDuration,isLeapYear,setDefaultOptions,getDefaultOptions,daysInWeek,formatDuration"
        )
    );
    // an opaque type the package exports is that type to TypeScript
    let generated = root().join("target/tmp/ffi_command/corpus/src/DateFns/Generated.fl");
    let generated = fs::read_to_string(generated).expect("the generated module");
    assert!(
        generated
            .contains("@ts \"$Duration\"\n@ts import \"date-fns\" \"Duration\"\ntype Duration\n"),
        "{generated}"
    );
    insta::assert_snapshot!(
        "delay",
        corpus("delay", "default,rangeDelay,clearDelay,createDelay")
    );
    insta::assert_snapshot!("p_map", corpus("p-map", "default,pMapIterable,pMapSkip"));
    insta::assert_snapshot!(
        "get_port",
        corpus("get-port", "default,portNumbers,clearLockedPorts")
    );
}

#[test]
#[ignore = "needs Node and `pnpm install`"]
fn the_backend_adds_ffi_and_a_go_project_names_who_does() {
    let dir = project("backend");
    let generated = dir.join("src/Ffi/LookupLib/Generated.fl");
    let flat = flick(&dir, &["ffi", "import", "lookup-lib", "--out", "src/Ffi"]);
    assert!(flat.status.success(), "{}", text(&flat.stderr));
    let first = fs::read_to_string(&generated).expect("generated");
    fs::remove_file(&generated).expect("remove");
    let qualified = flick(
        &dir,
        &["js:ffi", "import", "lookup-lib", "--out", "src/Ffi"],
    );
    assert!(qualified.status.success(), "{}", text(&qualified.stderr));
    assert_eq!(text(&qualified.stdout), text(&flat.stdout));
    assert_eq!(fs::read_to_string(&generated).expect("generated"), first);

    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n\n[build]\ntarget = \"go\"\n",
    )
    .expect("a Go project");
    let out = flick(&dir, &["ffi", "check"]);
    let stderr = text(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains("FLICK-P052"), "{stderr}");
    assert!(stderr.contains("`flick/js` adds it"), "{stderr}");
}
