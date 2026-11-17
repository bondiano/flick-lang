#[test]
fn a_missing_pnpm_says_how_to_install_the_pinned_one_and_changes_nothing() {
    let manifest = "{ \"name\": \"no-pnpm\", \"packageManager\": \"pnpm@1.0.0\" }\n";
    let dir = project(
        "cli_no_pnpm",
        &[
            ("flick.toml", &local_std()),
            ("package.json", manifest),
            ("src/App/Main.fl", MAIN),
        ],
    );
    // no program is on this PATH: pnpm is missing, and so is git
    let empty = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli_no_pnpm_path");
    fs::create_dir_all(&empty).expect("dir");
    let out = flick_with_path(&dir, empty.to_str().expect("utf-8"), &["add", "left-pad"]);
    assert_eq!(code(&out), Some(2), "{out:?}");
    let err = stderr(&out);
    assert!(err.contains("`pnpm` is not on PATH"), "{err}");
    assert!(
        err.contains("corepack prepare pnpm@1.0.0 --activate"),
        "{err}"
    );
    assert!(err.contains("npm install --global pnpm@1.0.0"), "{err}");
    assert!(err.contains("nothing was changed"), "{err}");
    assert_eq!(
        fs::read_to_string(dir.join("package.json")).expect("kept"),
        manifest
    );
    assert!(!dir.join("pnpm-lock.yaml").exists());
    assert!(!dir.join("node_modules/left-pad").exists());
}

/// The pin is exact: a newer *patch* is still another version, and nothing
/// runs.
#[cfg(unix)]
#[test]
fn a_newer_patch_is_refused_by_the_exact_pin() {
    let pin = "12.16.1";
    let manifest = format!("{{ \"name\": \"patch\", \"packageManager\": \"pnpm@{pin}\" }}\n");
    let dir = project(
        "cli_pnpm_newer_patch",
        &[("flick.toml", APP), ("package.json", &manifest)],
    );
    let (bin, marker) = fake_pnpm(&dir, "12.16.2");
    let out = flick_with_path(&dir, bin.to_str().expect("utf-8"), &["add", "left-pad"]);
    assert_eq!(code(&out), Some(2), "{out:?}");
    let err = stderr(&out);
    assert!(err.contains("FLICK-P016"), "{err}");
    assert!(
        err.contains(&format!(
            "the project pins pnpm {pin} in `packageManager`, but `pnpm` is 12.16.2"
        )),
        "{err}"
    );
    assert!(
        err.contains(&format!("corepack prepare pnpm@{pin} --activate")),
        "{err}"
    );
    assert!(
        err.contains(&format!("npm install --global pnpm@{pin}")),
        "{err}"
    );
    assert!(err.contains("nothing was changed"), "{err}");
    // pnpm itself never ran, and the project is as it was
    assert!(!marker.exists(), "a wrong pnpm ran: {marker:?}");
    assert_eq!(
        fs::read_to_string(dir.join("package.json")).expect("kept"),
        manifest
    );
    assert!(!dir.join("pnpm-lock.yaml").exists());
    assert!(!dir.join("node_modules/left-pad").exists());
}

/// The pinned version passes the gate: the fake is not an installer, so the
/// run may end elsewhere, but it must get as far as running pnpm.
#[cfg(unix)]
#[test]
fn the_pinned_version_passes_the_gate_and_runs_pnpm() {
    let pin = "12.16.1";
    let manifest = format!("{{ \"name\": \"patched\", \"packageManager\": \"pnpm@{pin}\" }}\n");
    let dir = project(
        "cli_pnpm_pinned",
        &[("flick.toml", APP), ("package.json", &manifest)],
    );
    let (bin, marker) = fake_pnpm(&dir, pin);
    let out = flick_with_path(&dir, bin.to_str().expect("utf-8"), &["add", "left-pad"]);
    let err = stderr(&out);
    assert!(
        !err.contains("FLICK-P016"),
        "the exact pin was refused:\n{err}"
    );
    // the stub installs nothing and says it worked: the gate is what this
    // project's run is about, and it ends where pnpm's exit status says
    assert_eq!(code(&out), Some(0), "{out:?}");
    let ran = fs::read_to_string(&marker).expect("pnpm ran, past the version gate");
    assert!(ran.contains("left-pad"), "{ran}");
}

#[test]
fn new_says_which_pnpm_install_needs() {
    let dir = project("cli_new_pnpm", &[]);
    fs::create_dir_all(&dir).expect("dir");
    let out = flick_in(&dir, &["new", "demo", "--template", "lib"]);
    assert_eq!(code(&out), Some(0), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Next: flick install"), "{text}");
    // the hint names the version the new project pins, which is what the
    // install then needs: the pnpm on PATH is another version, or none at all
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.join("demo/package.json")).expect("package.json"),
    )
    .expect("json");
    let pinned = manifest["packageManager"]
        .as_str()
        .expect("packageManager")
        .strip_prefix("pnpm@")
        .expect("a pnpm pin")
        .to_owned();
    assert!(!pinned.is_empty(), "{manifest}");
    assert!(
        text.contains(&format!("pnpm {pinned}")),
        "the hint must name the pinned {pinned}:\n{text}"
    );
}

#[test]
fn a_stale_lockfile_is_refused_and_pnpm_speaks_ndjson() {
    let manifest = format!(
        "{{ \"name\": \"stale\", \"packageManager\": \"pnpm@{}\", \"dependencies\": {{ \"left-pad\": \"1.3.0\" }} }}\n",
        pnpm_version()
    );
    // the lockfile predates the dependency
    let lock = "lockfileVersion: '9.0'\n\nimporters:\n\n  .: {}\n";
    let dir = project(
        "cli_stale_lock",
        &[
            ("flick.toml", APP),
            ("package.json", &manifest),
            ("pnpm-lock.yaml", lock),
            ("pnpm-workspace.yaml", "packages: []\npmOnFail: download\n"),
        ],
    );
    let out = flick_in(&dir, &["--diagnostics", "json", "install", "--locked"]);
    assert_eq!(code(&out), Some(2), "{out:?}");
    let events: Vec<serde_json::Value> = stderr(&out)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|_| panic!("not NDJSON: {l:?}")))
        .collect();
    assert!(events.iter().all(|e| e["schema-version"] == 1));
    assert!(
        events.iter().any(|e| e["type"] == "package-manager-output"),
        "{events:?}"
    );
    assert!(
        events.iter().any(|e| e["code"] == "FLICK-P017"),
        "{events:?}"
    );
    assert_eq!(
        fs::read_to_string(dir.join("package.json")).expect("kept"),
        manifest
    );
    assert_eq!(
        fs::read_to_string(dir.join("pnpm-lock.yaml")).expect("kept"),
        lock
    );
    assert!(!dir.join("node_modules/left-pad").exists());
}

#[test]
fn the_pinned_pnpm_installs_offline_and_keeps_a_frozen_lockfile() {
    let manifest = format!(
        "{{ \"name\": \"local\", \"packageManager\": \"pnpm@{}\", \"dependencies\": {{ \"fixture-dep\": \"file:./fixture-dep\" }} }}\n",
        pnpm_version()
    );
    let dependency = "{ \"name\": \"fixture-dep\", \"version\": \"1.0.0\" }\n";
    let dir = project(
        "cli_pnpm_install",
        &[
            ("flick.toml", APP),
            ("package.json", &manifest),
            ("pnpm-workspace.yaml", "packages: []\npmOnFail: download\n"),
            ("fixture-dep/package.json", dependency),
        ],
    );
    let out = flick_in(&dir, &["install", "--offline"]);
    assert_eq!(code(&out), Some(0), "{out:?}");
    assert_eq!(
        fs::read_to_string(dir.join("node_modules/fixture-dep/package.json")).expect("installed"),
        dependency
    );
    let lock = fs::read_to_string(dir.join("pnpm-lock.yaml")).expect("lockfile");
    let out = flick_in(&dir, &["install", "--locked", "--offline"]);
    assert_eq!(code(&out), Some(0), "{out:?}");
    assert_eq!(
        fs::read_to_string(dir.join("package.json")).expect("kept"),
        manifest
    );
    assert_eq!(
        fs::read_to_string(dir.join("pnpm-lock.yaml")).expect("kept"),
        lock
    );
}

#[test]
fn check_and_build_run_no_code_of_the_program() {
    let witness = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cli_effects_witness");
    let _ = fs::remove_file(&witness);
    let js = format!(
        "import {{ writeFileSync }} from \"node:fs\";\nwriteFileSync({:?}, \"loaded\");\nexport const now = () => 1;\n",
        witness.display().to_string()
    );
    let main = "module App.Main exposing (main)\n\nnow : Unit ->{} Float\nforeign now from \"./effect.mjs\" \"now\"\n\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line (show (now ()))\n";
    let dir = project(
        "cli_effects",
        &[
            ("flick.toml", APP),
            ("src/App/Main.fl", main),
            ("src/App/effect.mjs", &js),
        ],
    );
    assert_eq!(code(&flick_in(&dir, &["check"])), Some(0));
    assert_eq!(code(&flick_in(&dir, &["build"])), Some(0));
    assert!(!witness.exists(), "the foreign module was not loaded");
    assert!(!dir.join("node_modules").exists(), "nothing was installed");
}

const LIB: &str = "schema-version = 1\n\n[project]\nkind = \"lib\"\n";

const API: &str = "module Lib.Api exposing (Shape(..), Store, Named, measure, keep, fetch)

/// A shape.
type Shape = Circle Float | Square Float

/// Where things are kept.
service Store =
  put : String ->{; StoreError} Unit

type error StoreError = StoreError String

class Named a =
  name : a -> String

/// The area.
measure : Shape -> Float
let measure s =
  match s with
  | Circle r -> r * r * 3
  | Square a -> a * a

/// Keeps a callback that holds `c`.
keep : forall c. (Unit -> Float)^c -> (Unit -> Float)^c
let keep f = f

/// Puts every name.
fetch : Named a => List a ->{Store; StoreError} Unit
let fetch items = List.each_effect (fun x -> Store.put (name x)) items
";
