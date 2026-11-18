// The standard-library split through its real release/install path.

use std::fmt::Write as _;
#[cfg(unix)]
use std::os::unix::fs::symlink;
#[cfg(windows)]
use std::os::windows::fs::symlink_dir as symlink;

#[test]
#[ignore = "requires built JS packages; run by just e2e-tests"]
fn released_source_packages_are_fetched_pinned_and_run_offline_without_the_adapter() {
    let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("../packages");
    let store = project("std-tarball-store", &[]);
    let out = store.join("dist");
    let mut manifest =
        "schema-version = 1\n\n[project]\nkind = \"app\"\nentry = \"App.Main\"\n\n[dependencies]\n"
            .to_owned();
    for name in IMPLICIT_PACKAGES {
        let packed = released(
            &packages.join(name),
            &store,
            &["pack", "--out-dir", &out.to_string_lossy()],
        );
        assert!(packed.status.success(), "{name}: {packed:?}");
        let line = String::from_utf8_lossy(&packed.stdout).into_owned();
        let (sha, file) = line.trim().split_once("  ").expect("sha and path");
        let url = serve_once(fs::read(file).expect("tarball"));
        writeln!(
            manifest,
            "{name} = {{ url = \"{url}\", sha256 = \"{sha}\" }}"
        )
        .expect("manifest string");
    }
    let main = "module App.Main exposing (main)\n\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line (Http.Method.to_string Http.Method.POST ++ \":\" ++ show (List.length Http.Method.all))\n";
    let app = project(
        "std-tarball-app",
        &[("flick.toml", &manifest), ("src/App/Main.fl", main)],
    );
    let installed = released(&app, &store, &["install"]);
    assert!(installed.status.success(), "{installed:?}");
    let checked = released(&app, &store, &["check"]);
    assert!(checked.status.success(), "{checked:?}");
    let lock = fs::read_to_string(app.join("flick.lock")).expect("all source packages pinned");
    for name in IMPLICIT_PACKAGES {
        assert!(lock.contains(&format!("name = \"{name}\"")), "{lock}");
    }
    // Each loopback tarball server has exited after the fetch. A locked
    // reinstall can now succeed only using the content-addressed store.
    let offline = released(&app, &store, &["install", "--locked", "--offline"]);
    assert!(offline.status.success(), "{offline:?}");
    assert_eq!(
        fs::read_to_string(app.join("flick.lock")).expect("lock"),
        lock
    );
    let built = released(&app, &store, &["build"]);
    assert!(built.status.success(), "{built:?}");
    let executed = released(&app, &store, &["run"]);
    assert!(executed.status.success(), "{executed:?}");
    assert_eq!(String::from_utf8_lossy(&executed.stdout).trim(), "POST:9");
    let package_json = fs::read_to_string(app.join("package.json")).unwrap_or_default();
    assert!(
        !package_json.contains("@flick-lang/effect-ts"),
        "{package_json}"
    );
    let emitted = fs::read_to_string(app.join("dist/App/Main.js")).expect("emitted program");
    assert!(!emitted.contains("@flick-lang/effect-ts"), "{emitted}");

    // The same pinned source packages also work with the real installed
    // adapter. Its manifest supplies exact production pins and runtime ABI.
    let repo = packages.parent().expect("repository");
    symlink(repo.join("node_modules"), app.join("node_modules"))
        .expect("workspace runtime and adapter");
    let metadata: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(packages.join("effect-ts/package.json")).expect("adapter manifest"),
    )
    .expect("adapter metadata");
    fs::write(
        app.join("package.json"),
        serde_json::json!({"type":"module", "dependencies": {
            "@flick-lang/effect-ts": metadata["version"],
            "@flick-lang/runtime": env!("CARGO_PKG_VERSION")
        }})
        .to_string(),
    )
    .expect("exact adapter version");
    fs::write(
        app.join("flick.toml"),
        format!("{manifest}\n[build]\neffect = \"effect\"\n"),
    )
    .expect("mixed consumer");
    fs::write(
        app.join("src/App/Main.fl"),
        "module App.Main exposing (main)\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line (show (Stream.run_collect (Stream.from_list [2, 3, 5])))\n",
    )
    .expect("moved streaming program");
    let mixed = released(&app, &store, &["run"]);
    assert!(mixed.status.success(), "{mixed:?}");
    assert_eq!(String::from_utf8_lossy(&mixed.stdout).trim(), "[2, 3, 5]");
    assert_eq!(
        fs::read_to_string(app.join("flick.lock")).expect("lock"),
        lock
    );
}

#[test]
fn a_partial_implicit_package_override_is_rejected_instead_of_using_embedded_sources() {
    let root = project(
        "partial-implicit-override",
        &[
            (
                "std/flick.toml",
                "schema-version = 1\n[project]\nkind = \"lib\"\n",
            ),
            (
                "std/src/modules.txt",
                "# the other implicit packages are missing\n",
            ),
            (
                "app/flick.toml",
                "schema-version = 1\n[project]\nkind = \"lib\"\n",
            ),
            (
                "app/src/App/Main.fl",
                "module App.Main exposing (x)\nx : Int\nlet x = 1\n",
            ),
        ],
    );
    let out = Command::new(env!("CARGO_BIN_EXE_flick"))
        .arg("check")
        .current_dir(root.join("app"))
        .env("FLICK_STD_PATH", root.join("std"))
        .env("FLICK_STORE", root.join("store"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .output()
        .expect("flick runs");
    let diagnostic = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "partial overrides cannot select embedded sources"
    );
    assert!(
        diagnostic.contains("stream"),
        "the missing sibling is named: {diagnostic}"
    );
}
