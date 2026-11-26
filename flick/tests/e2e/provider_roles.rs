const PROVIDER_ROLES_LIVE_AND_FAKE_FILES: &[(&str, &str)] = &[
    (
        "Contracts.fl",
        "\
module App.Contracts exposing (StoreConfig, StoreRole)

type module StoreConfig =
  prefix : String

type module StoreRole =
  service Store =
    get : String ->{} Option String
  layer : Layer none none {Store}
",
    ),
    (
        "PrimaryConfig.fl",
        "\
module App.PrimaryConfig exposing (prefix)

prefix : String
let prefix = \"live-primary\"
",
    ),
    (
        "ReplicaConfig.fl",
        "\
module App.ReplicaConfig exposing (prefix)

prefix : String
let prefix = \"live-replica\"
",
    ),
    (
        "MakeStore.fl",
        "\
module App.MakeStore(C : App.Contracts.StoreConfig) : App.Contracts.StoreRole

service Store =
  get : String ->{} Option String

layer : Layer none none {Store}
let layer =
  Layer.succeed Store {
    get = fun key -> Some (C.prefix ++ \":\" ++ key);
  }
",
    ),
    (
        "Env.fl",
        "\
module App.Env exposing (Primary, Replica)

module Primary = App.MakeStore(App.PrimaryConfig)
module Replica = App.MakeStore(App.ReplicaConfig)
",
    ),
    (
        "Service.fl",
        "\
module App.Service exposing (read_both)

import App.Env.Primary as Primary
import App.Env.Replica as Replica

read_both : String ->{Primary.Store, Replica.Store} String
let read_both key =
  let p = match Primary.Store.get key with | Some v -> v | None -> \"none\"
  let r = match Replica.Store.get key with | Some v -> v | None -> \"none\"
  p ++ \" | \" ++ r
",
    ),
    (
        "Main.fl",
        "\
module App.Main exposing (main)

import App.Env.Primary as Primary
import App.Env.Replica as Replica
import App.Service as Service

fake_primary : Ref (List String) -> Layer none none {Primary.Store}
let fake_primary trace =
  Layer.succeed Primary.Store {
    get = fun key ->
      Ref.update trace (fun xs -> xs ++ [\"primary:\" ++ key])
      Some (\"fake-primary:\" ++ key);
  }

fake_replica : Ref (List String) -> Layer none none {Replica.Store}
let fake_replica trace =
  Layer.succeed Replica.Store {
    get = fun key ->
      Ref.update trace (fun xs -> xs ++ [\"replica:\" ++ key])
      Some (\"fake-replica:\" ++ key);
  }

main : Unit ->{Host.Console} Unit
let main () =
  let live = provide Layer.merge Primary.layer Replica.layer { Service.read_both \"k1\" }
  let trace = Ref.make []
  let fake = provide Layer.merge (fake_primary trace) (fake_replica trace) { Service.read_both \"k2\" }
  Platform.stdout_line live
  Platform.stdout_line fake
  Platform.stdout_line (String.join \",\" (Ref.get trace))
",
    ),
];

const PROVIDER_ROLES_PACKAGE_LIB_FILES: &[(&str, &str)] = &[
    (
        "Lib/Contracts.fl",
        "\
module Lib.Contracts exposing (StoreConfig, StoreRole)

type module StoreConfig =
  prefix : String

type module StoreRole =
  service Store =
    get : String ->{} Option String
  layer : Layer none none {Store}
",
    ),
    (
        "Lib/PrimaryConfig.fl",
        "\
module Lib.PrimaryConfig exposing (prefix)

prefix : String
let prefix = \"pkg-primary\"
",
    ),
    (
        "Lib/ReplicaConfig.fl",
        "\
module Lib.ReplicaConfig exposing (prefix)

prefix : String
let prefix = \"pkg-replica\"
",
    ),
    (
        "Lib/MakeStore.fl",
        "\
module Lib.MakeStore(C : Lib.Contracts.StoreConfig) : Lib.Contracts.StoreRole

service Store =
  get : String ->{} Option String

layer : Layer none none {Store}
let layer =
  Layer.succeed Store {
    get = fun key -> Some (C.prefix ++ \":\" ++ key);
  }
",
    ),
    (
        "Lib/Env.fl",
        "\
module Lib.Env exposing (Primary, Replica)

module Primary = Lib.MakeStore(Lib.PrimaryConfig)
module Replica = Lib.MakeStore(Lib.ReplicaConfig)
",
    ),
];

const PROVIDER_ROLES_PACKAGE_APP_MAIN: &str = "\
module App.Main exposing (read_both, main)

import Lib.Env.Primary as Primary
import Lib.Env.Replica as Replica

read_both : String ->{Primary.Store, Replica.Store} String
let read_both key =
  let p = match Primary.Store.get key with | Some v -> v | None -> \"none\"
  let r = match Replica.Store.get key with | Some v -> v | None -> \"none\"
  p ++ \"+\" ++ r

main : Unit ->{Host.Console} Unit
let main () =
  let out = provide Layer.merge Primary.layer Replica.layer { read_both \"u1\" }
  Platform.stdout_line out
";

fn assert_distinct_provider_role_dts(out_dir: &Path, stage: &str) {
    let primary_dts =
        fs::read_to_string(out_dir.join("Lib/Env/Primary.d.ts")).expect("Primary.d.ts");
    let replica_dts =
        fs::read_to_string(out_dir.join("Lib/Env/Replica.d.ts")).expect("Replica.d.ts");
    let main_dts = fs::read_to_string(out_dir.join("App/Main.d.ts")).expect("Main.d.ts");
    let primary_brand = primary_dts
        .lines()
        .find(|l| l.contains("\"~flick/brand\""))
        .expect("Primary brand");
    let replica_brand = replica_dts
        .lines()
        .find(|l| l.contains("\"~flick/brand\""))
        .expect("Replica brand");
    assert_ne!(primary_brand, replica_brand, "{stage}: distinct brands");
    assert!(
        primary_dts.contains("export declare const $s_Store:"),
        "{stage}: Primary $s_Store"
    );
    assert!(
        replica_dts.contains("export declare const $s_Store:"),
        "{stage}: Replica $s_Store"
    );
    assert!(
        main_dts.contains("Lib/Env/Primary") && main_dts.contains("Lib/Env/Replica"),
        "{stage}: App/Main.d.ts imports both roles:\n{main_dts}"
    );
}

#[test]
#[ignore = "needs Node and Go; run `just e2e-tests`"]
fn provider_roles_live_and_fake_composition_runs_on_js_effect_and_go() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("md4_provider_roles_live_and_fake");
    let _ = fs::remove_dir_all(&dir);
    let source = dir.join("App");
    fs::create_dir_all(&source).expect("source directory");
    for (name, text) in PROVIDER_ROLES_LIVE_AND_FAKE_FILES {
        fs::write(source.join(name), text).expect("write source");
    }
    let main = source.join("Main.fl");
    let expected = b"live-primary:k1 | live-replica:k1\nfake-primary:k2 | fake-replica:k2\nprimary:k2,replica:k2\n";
    for profile in ["debug", "release"] {
        for (label, emit, runtime_args) in [
            ("js-rt", "js", &["--runtime", "rt"][..]),
            ("js-effect", "js", &["--runtime", "effect"][..]),
            ("go", "go", &["--allow-tool", "go"][..]),
        ] {
            let out = dir.join(format!("out-{label}-{profile}"));
            let build = Command::new(env!("CARGO_BIN_EXE_flick"))
                .arg("build")
                .arg(&main)
                .arg("--out-dir")
                .arg(&out)
                .arg("--target")
                .arg(emit)
                .args(runtime_args)
                .arg("--profile")
                .arg(profile)
                .output()
                .expect("build runs");
            assert!(
                build.status.success(),
                "{label}/{profile}: {}",
                String::from_utf8_lossy(&build.stderr)
            );
            let output = if emit == "js" {
                Command::new("node")
                    .arg(out.join(runner_path(false)))
                    .output()
                    .expect("Node runs")
            } else {
                Command::new(out.join(executable(MAIN_ENTRY)))
                    .output()
                    .expect("Go executable runs")
            };
            assert!(
                output.status.success(),
                "{label}/{profile}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, expected, "{label}/{profile}");
            assert!(
                output.stderr.is_empty(),
                "{label}/{profile}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
#[ignore = "needs Node and Go; run `just e2e-tests`"]
fn package_exported_provider_roles_build_fresh_warm_offline_and_emit_distinct_dts() {
    const HASH: &str = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2";

    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("md4_provider_roles_package_cache");
    let _ = fs::remove_dir_all(&dir);
    let app = dir.join("app");
    let pkg_root = dir.join(format!("store/{HASH}/lib"));
    let pkg_src = pkg_root.join("src");
    fs::create_dir_all(app.join("src/App")).expect("app src dir");
    fs::create_dir_all(pkg_src.join("Lib")).expect("pkg src dir");

    let app_toml = "\
schema-version = 1

[tools]
\"flick/go\" = [\"go\"]

[project]
kind = \"app\"
entry = \"App.Main\"

[dependencies]
stores = { git = \"https://example.com/stores\", tag = \"v1.0.0\", subdir = \"lib\" }
";
    let lock = format!(
        "version = 1\n\n[[package]]\nname = \"stores\"\nversion = \"1.0.0\"\nsource = \"git+https://example.com/stores?tag=v1.0.0&subdir=lib#0123abcd\"\nchecksum = \"sha256:{HASH}\"\n"
    );
    let lib_toml = "\
schema-version = 1

[package]
name = \"stores\"
version = \"1.0.0\"

[project]
kind = \"lib\"
";
    fs::write(app.join("flick.toml"), app_toml).expect("app flick.toml");
    fs::write(app.join("flick.lock"), lock).expect("app flick.lock");
    fs::write(pkg_root.join("flick.toml"), lib_toml).expect("lib flick.toml");
    for (file, text) in PROVIDER_ROLES_PACKAGE_LIB_FILES {
        fs::write(pkg_src.join(file), text).expect("write lib module");
    }
    fs::write(app.join("src/App/Main.fl"), PROVIDER_ROLES_PACKAGE_APP_MAIN).expect("write Main.fl");

    let run_stage = |cache_dir: &Path, stage: &str, emit: &str, extra: &[&str]| {
        let out_dir = app.join(format!("out-{emit}-{stage}"));
        let _ = fs::remove_dir_all(&out_dir);
        let build = Command::new(env!("CARGO_BIN_EXE_flick"))
            .args(["build", "--offline", "--target", emit, "--out-dir"])
            .arg(&out_dir)
            .args(extra)
            .current_dir(&app)
            .env("FLICK_STORE", dir.join("store"))
            .env("XDG_CACHE_HOME", cache_dir)
            .env("NO_COLOR", "1")
            .output()
            .expect("flick build runs");
        assert!(
            build.status.success(),
            "{emit}/{stage} build failed:\n{}",
            String::from_utf8_lossy(&build.stderr)
        );
        let output = if emit == "js" {
            Command::new("node")
                .arg(out_dir.join(runner_path(false)))
                .output()
                .expect("node runs")
        } else {
            Command::new(out_dir.join(executable(MAIN_ENTRY)))
                .output()
                .expect("go runs")
        };
        assert!(
            output.status.success(),
            "{emit}/{stage} run failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            output.stdout, b"pkg-primary:u1+pkg-replica:u1\n",
            "{emit}/{stage}"
        );
        out_dir
    };

    // Exercise fresh (populates cache), warm (sources still on disk — pins Bug 2),
    // and offline (library sources removed) on JS (rt & effect) and Go.
    let cache_rt = dir.join("cache-rt");
    let cache_effect = dir.join("cache-effect");
    let cache_go = dir.join("cache-go");

    for stage in ["fresh", "warm"] {
        run_stage(&cache_rt, stage, "js", &["--runtime", "rt"]);
        let out_effect = run_stage(
            &cache_effect,
            stage,
            "js",
            &["--runtime", "effect", "--declarations"],
        );
        run_stage(&cache_go, stage, "go", &[]);
        assert_distinct_provider_role_dts(&out_effect, stage);
    }

    // Delete original library sources and verify offline builds still succeed from cache.
    fs::remove_dir_all(&pkg_src).expect("remove library sources");
    assert!(!pkg_src.exists());
    run_stage(&cache_rt, "offline", "js", &["--runtime", "rt"]);
    let out_offline = run_stage(
        &cache_effect,
        "offline",
        "js",
        &["--runtime", "effect", "--declarations"],
    );
    run_stage(&cache_go, "offline", "go", &[]);
    assert_distinct_provider_role_dts(&out_offline, "offline");
}
