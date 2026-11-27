//! Integration and cross-target parity tests for native Rust VM host services
//! (`Console`, `Clock`, `Random`, `Environment`, `Arguments`, `File`,
//! `FileSystem`, `ChildProcess`, `vm/native` host package, typed OS errors,
//! `TestClock`, and `Random.with_seed`).

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use crate::common::{APP, project};

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn flick_vm(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("flick runs")
}

fn flick_vm_no_path(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(dir)
        .env("PATH", "")
        .output()
        .expect("flick runs without PATH")
}

#[cfg(unix)]
fn link_repo_node_modules(dir: &Path) {
    use std::os::unix::fs::symlink;

    if !dir
        .ancestors()
        .any(|p| p.join("node_modules/@flick-lang/runtime").exists())
    {
        let repo_nm = Path::new(env!("CARGO_MANIFEST_DIR")).join("../node_modules");
        symlink(
            fs::canonicalize(&repo_nm).expect("repo node_modules"),
            dir.join("node_modules"),
        )
        .expect("link node_modules");
    }
}

#[cfg(not(unix))]
fn link_repo_node_modules(_dir: &Path) {}

#[test]
fn vm_host_console_env_args_clock_random_and_exit_codes() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");

    // 1. Standalone `tests/run/effect/host_root.fl` must match both `.stdout` and `.stderr`.
    let host_root = repo.join("tests/run/effect/host_root.fl");
    let expected_out = fs::read_to_string(host_root.with_extension("stdout")).expect("stdout");
    let expected_err = fs::read_to_string(host_root.with_extension("stderr")).expect("stderr");
    let out = flick_vm_no_path(
        &repo,
        &["run", "--target", "vm", "tests/run/effect/host_root.fl"],
    );
    assert!(
        out.status.success(),
        "host_root.fl failed:\nstdout={}\nstderr={}",
        stdout(&out),
        stderr(&out)
    );
    assert_eq!(stdout(&out), expected_out);
    assert_eq!(stderr(&out), expected_err);

    // 2. Forwarded CLI args, custom env var, live clock, live random, and stderr.
    let main = r#"
module App.Main exposing (main)

import Host

main :
  Unit ->{Host.Console, Host.Clock, Host.Random, Host.Environment, Host.Arguments; AppError} Unit
let main () =
  Host.Console.stdout_write "hello "
  Host.Console.stdout_line "world"
  Host.Console.stderr_line "err-line"
  let v = Host.Environment.env "FLICK_VM_HOST_TEST_VAR"
  let args = Host.Arguments.args ()
  let t1 = Host.Clock.current_time_millis ()
  let n1 = Host.Clock.current_time_nanos ()
  let m1 = Host.Clock.monotonic_time_nanos ()
  let m2 = Host.Clock.monotonic_time_nanos ()
  let r = Host.Random.next ()
  Host.Console.stdout_line (show v)
  Host.Console.stdout_line (show args)
  Host.Console.stdout_line (show (t1 > 1700000000000 && n1 > 0n && m2 >= m1 && m1 >= 0n))
  Host.Console.stdout_line (show (r >= 0 && r < 1))
"#;
    let dir = project(
        "vm_host_core",
        &[("flick.toml", APP), ("src/App/Main.fl", main)],
    );
    let run = Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(["run", "--target", "vm", "--", "alpha", "beta", "--flag"])
        .current_dir(&dir)
        .env("PATH", "")
        .env("FLICK_VM_HOST_TEST_VAR", "hello_vm")
        .output()
        .expect("flick runs");
    assert!(
        run.status.success(),
        "vm_host_core failed:\nstdout={}\nstderr={}",
        stdout(&run),
        stderr(&run)
    );
    assert_eq!(
        stdout(&run),
        "hello world\nSome \"hello_vm\"\n[\"alpha\", \"beta\", \"--flag\"]\ntrue\ntrue\n"
    );
    assert_eq!(stderr(&run), "err-line\n");

    // 3. Uncaught `App.fail` matches JS/Go exit code 1 and `error: <message>\n` stderr.
    let app_fail_out = flick_vm_no_path(
        &repo,
        &["run", "--target", "vm", "tests/run/bootstrap/app_fail.fl"],
    );
    assert_eq!(app_fail_out.status.code(), Some(1));
    assert_eq!(stdout(&app_fail_out), "before\n");
    assert_eq!(stderr(&app_fail_out), "error: config \"port\" is missing\n");

    // 4. Uncaught defect exits with code 1 and writes diagnostic to stderr.
    let defect_out = flick_vm_no_path(
        &repo,
        &["run", "--target", "vm", "tests/run/codegen/defect.fl"],
    );
    assert_eq!(defect_out.status.code(), Some(1));
    assert_eq!(stdout(&defect_out), "before\n");
    assert!(
        !stderr(&defect_out).is_empty(),
        "expected non-empty stderr on defect"
    );
}

#[test]
fn vm_host_files_filesystem_and_scoped_cleanup() {
    let main = r#"
module App.Main exposing (main)

import File
import FileSystem
import PlatformError

main : Unit ->{Host.Random, Host.Console, Async; AppError} Unit
let main () =
  let res =
    try
      provide FileSystem.node {
        provide File.node {
          let leaked_paths = Ref.make ("", "")
          let inside_ok = scope {
            let dir =
              FileSystem.make_temp_directory_scoped {
                directory = None
                prefix = Some "flick-vm-fs-"
              }
            let tmp_file =
              FileSystem.make_temp_file_scoped {
                directory = None
                prefix = Some "flick-vm-file-"
                suffix = Some ".txt"
              }
            Ref.set leaked_paths (dir, tmp_file)
            let p1 = dir ++ "/hello.txt"
            File.write_text p1 "abc"
            let t1 = File.read_text p1
            let t2 = scope {
              let h = File.open_scoped p1
              File.read_all h
            }
            let p2 = dir ++ "/bytes.bin"
            FileSystem.write_file { flag = None; mode = None } p2 [65, 66, 67, 68]
            let bs = FileSystem.read_file p2
            let info = FileSystem.stat p2
            let sub = dir ++ "/sub"
            FileSystem.make_directory { recursive = true; mode = None } sub
            FileSystem.copy_file p2 (sub ++ "/copy.bin")
            let entries = FileSystem.read_directory false dir
            let globbed = FileSystem.glob { root = Some dir; exclude = [] } "**/*.bin"
            // Open file handle seek / read_alloc / write_all / file_stat / truncate / sync
            let rw_path = dir ++ "/rw.bin"
            let handle_ok = scope {
              let f =
                FileSystem.open
                  { flag = Some FileSystem.WriteRead; mode = None; noFollow = false }
                  rw_path
              FileSystem.write_all [10, 20, 30, 40] f
              FileSystem.sync f
              let pos = FileSystem.seek 1n FileSystem.Start f
              let chunk = FileSystem.read_alloc 2 None f
              FileSystem.file_truncate (Some 2) f
              let st = FileSystem.file_stat f
              pos == 1n && chunk == Some [20, 30] && st.size == 2n
            }
            let dir_exists_inside = FileSystem.exists dir
            let tmp_exists_inside = FileSystem.exists tmp_file
            t1 == "abc"
              && t2 == "abc"
              && bs == [65, 66, 67, 68]
              && info.size == 4n
              && List.length entries >= 3
              && List.length globbed == 2
              && handle_ok
              && dir_exists_inside
              && tmp_exists_inside
          }
          let (dir_after, tmp_after) = Ref.get leaked_paths
          let cleaned_dir = not (FileSystem.exists dir_after)
          let cleaned_tmp = not (FileSystem.exists tmp_after)
          show (inside_ok, cleaned_dir, cleaned_tmp)
        }
      }
    with
    | e -> "err"
  Platform.stdout_line res
"#;
    let dir = project(
        "vm_host_fs",
        &[("flick.toml", APP), ("src/App/Main.fl", main)],
    );
    let out = flick_vm_no_path(&dir, &["run", "--target", "vm"]);
    assert!(
        out.status.success(),
        "vm_host_fs failed:\nstdout={}\nstderr={}",
        stdout(&out),
        stderr(&out)
    );
    assert_eq!(stdout(&out), "(true, true, true)\n");
}

#[test]
fn vm_host_child_process_fixture_matches_expected_stdout() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let fixture = repo.join("tests/run/stdlib/process/child_process.fl");
    let expected = fs::read_to_string(fixture.with_extension("stdout")).expect("expected stdout");
    let out = flick_vm(
        &repo,
        &[
            "run",
            "--target",
            "vm",
            "tests/run/stdlib/process/child_process.fl",
        ],
    );
    assert!(
        out.status.success(),
        "child_process.fl on vm failed:\nstdout={}\nstderr={}",
        stdout(&out),
        stderr(&out)
    );
    assert_eq!(stdout(&out), expected);
}

#[test]
fn vm_native_host_package_checks_and_runs_without_p064() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let vm_host_pkg = repo.join("packages/vm-host");
    let manifest = format!(
        r#"schema-version = 1

[project]
kind = "app"
entry = "App.Main"

[build]
target = "vm"
environment = "vm/native"

[dependencies]
vm-host = {{ path = "{}" }}
"#,
        vm_host_pkg.display()
    );
    let main = r#"
module App.Main exposing (main)

import File
import FileSystem
import Vm.Host

main : Unit ->{Host.Random, Host.Console, Async; AppError} Unit
let main () =
  let msg =
    try
      provide Vm.Host.file_system {
        provide Vm.Host.file {
          scope {
            let dir =
              FileSystem.make_temp_directory_scoped {
                directory = None
                prefix = Some "flick-vm-host-pkg-"
              }
            let p = dir ++ "/greeting.txt"
            File.write_text p "hello from vm/native"
            File.read_text p
          }
        }
      }
    with
    | _ -> "error"
  Platform.stdout_line msg
"#;
    let dir = project(
        "vm_native_host_pkg",
        &[("flick.toml", &manifest), ("src/App/Main.fl", main)],
    );
    let out = flick_vm_no_path(&dir, &["run"]);
    assert!(
        out.status.success(),
        "vm_native_host_pkg failed:\nstdout={}\nstderr={}",
        stdout(&out),
        stderr(&out)
    );
    assert_eq!(stdout(&out), "hello from vm/native\n");
}

#[cfg(unix)]
const OS_ERROR_MAIN: &str = r#"
module App.Main exposing (main)

import FileSystem
import PlatformError

tag_of : PlatformError.PlatformError -> String
let tag_of err =
  match err with
  | PlatformError.PlatformError (PlatformError.System s) -> show s.tag
  | PlatformError.PlatformError (PlatformError.Bad b) -> "Bad:" ++ b.method

main : Unit ->{Host.Console, Host.Arguments, Async; AppError} Unit
let main () =
  match Platform.args () with
  | [missing_path, readonly_target, no_perm_file] ->
    provide FileSystem.node {
      let a =
        try
          let _ = FileSystem.read_file_string missing_path
          "ok"
        with
        | e -> tag_of e
      let b =
        try
          FileSystem.write_file_string { flag = None; mode = None } readonly_target "data"
          "ok"
        with
        | e -> tag_of e
      let c =
        try
          let _ = FileSystem.read_file_string no_perm_file
          "ok"
        with
        | e -> tag_of e
      Platform.stdout_line ("missing=" ++ a)
      Platform.stdout_line ("readonly=" ++ b)
      Platform.stdout_line ("chmod000=" ++ c)
    }
  | _ -> Platform.stdout_line "bad-args"
"#;

#[cfg(unix)]
fn run_typed_os_error_case(name: &str, check_js_go: bool) {
    use std::os::unix::fs::PermissionsExt as _;

    let dir = project(
        name,
        &[("flick.toml", APP), ("src/App/Main.fl", OS_ERROR_MAIN)],
    );
    if check_js_go {
        link_repo_node_modules(&dir);
    }
    let work = dir.join("fixtures");
    fs::create_dir_all(&work).expect("create fixtures dir");

    let missing_path = work.join("does-not-exist.txt");
    let ro_dir = work.join("readonly-dir");
    fs::create_dir_all(&ro_dir).expect("create ro_dir");
    fs::set_permissions(&ro_dir, fs::Permissions::from_mode(0o555)).expect("chmod 555 ro_dir");
    let ro_target = ro_dir.join("out.txt");

    let no_perm = work.join("secret.txt");
    fs::write(&no_perm, "secret").expect("write secret");
    fs::set_permissions(&no_perm, fs::Permissions::from_mode(0o000)).expect("chmod 000 secret");

    let missing_s = missing_path.to_string_lossy().into_owned();
    let ro_s = ro_target.to_string_lossy().into_owned();
    let no_perm_s = no_perm.to_string_lossy().into_owned();

    let vm_out = flick_vm_no_path(
        &dir,
        &["run", "--target", "vm", "--", &missing_s, &ro_s, &no_perm_s],
    );
    let extras = if check_js_go {
        vec![
            (
                "js",
                flick_vm(
                    &dir,
                    &[
                        "run",
                        "--target",
                        "js",
                        "--runtime",
                        "effect",
                        "--",
                        &missing_s,
                        &ro_s,
                        &no_perm_s,
                    ],
                ),
            ),
            (
                "go",
                flick_vm(
                    &dir,
                    &["run", "--target", "go", "--", &missing_s, &ro_s, &no_perm_s],
                ),
            ),
        ]
    } else {
        Vec::new()
    };

    // Restore permissions before asserting so cleanup always succeeds.
    let _ = fs::set_permissions(&ro_dir, fs::Permissions::from_mode(0o755));
    let _ = fs::set_permissions(&no_perm, fs::Permissions::from_mode(0o644));

    let expected = "missing=NotFound\nreadonly=PermissionDenied\nchmod000=PermissionDenied\n";
    assert!(
        vm_out.status.success(),
        "{name} failed on vm:\nstdout={}\nstderr={}",
        stdout(&vm_out),
        stderr(&vm_out)
    );
    assert_eq!(stdout(&vm_out), expected, "typed OS error mismatch on vm");
    for (label, out) in &extras {
        assert!(
            out.status.success(),
            "{name} failed on {label}:\nstdout={}\nstderr={}",
            stdout(out),
            stderr(out)
        );
        assert_eq!(stdout(out), expected, "typed OS error mismatch on {label}");
        assert_eq!(stdout(&vm_out), stdout(out));
    }
}

#[cfg(unix)]
#[test]
fn vm_host_typed_os_errors_missing_readonly_and_chmod_000() {
    run_typed_os_error_case("vm_host_os_errors_vm", false);
}

#[cfg(unix)]
#[test]
#[ignore = "needs Node, Go and `pnpm install`; run `just e2e`"]
fn vm_host_typed_os_error_parity_missing_readonly_and_chmod_000() {
    run_typed_os_error_case("vm_host_os_errors_parity", true);
}

const DETERMINISM_SUITE: &str = r#"
module App.DeterminismTest exposing (tests)

import Clock
import Duration
import Random
import Test

ms : Float -> Duration.Duration
let ms n = Result.with_default Duration.zero (Duration.millis n)

tests : List Test.Test
let tests = [
  Test.group "determinism" [
    Test.clock_test "TestClock set_time and adjust match JS/Go" (task {
      let t0 = Clock.current_time_millis ()
      let n0 = Clock.current_time_nanos ()
      Test.adjust (ms 1500)
      let t1 = Clock.current_time_millis ()
      let n1 = Clock.current_time_nanos ()
      let m1 = Clock.monotonic_time_nanos ()
      Test.set_time 5000
      let t2 = Clock.current_time_millis ()
      Test.expect (t0, n0, t1, n1, m1, t2)
      |> Test.to.equal (0, 0n, 1500, 1500000000n, 1500000000n, 5000)
    }),
    Test.effect_test "Random.with_seed matches ISAAC JS/Go sequence" (task {
      let draws =
        Random.with_seed
          42
          (task {
            let d0 = Random.next ()
            let d1 = Random.next ()
            let i0 = Random.next_int_between { halfOpen = Some true } 0 1000
            let b0 = Random.next_boolean ()
            (d0, d1, i0, b0)
          })
      let repeat =
        Random.with_seed
          42
          (task {
            let d0 = Random.next ()
            let d1 = Random.next ()
            let i0 = Random.next_int_between { halfOpen = Some true } 0 1000
            let b0 = Random.next_boolean ()
            (d0, d1, i0, b0)
          })
      let (first_draw, _, _, _) = draws
      Test.expect (draws == repeat && first_draw == 0.2272248550901319)
      |> Test.to.be_true
    }),
  ],
]
"#;

const DETERMINISM_MAIN: &str = r"
module App.Main exposing (main)

import Random

main : Unit ->{Host.Random, Host.Console, Async; AppError} Unit
let main () =
  let seq =
    Random.with_seed
      42
      (task {
        let d0 = Random.next ()
        let d1 = Random.next ()
        let d2 = Random.next ()
        let i0 = Random.next_int_between { halfOpen = Some true } 0 1000
        let i1 = Random.next_int_between { halfOpen = Some true } 0 1000
        let b0 = Random.next_boolean ()
        let b1 = Random.next_boolean ()
        (d0, d1, d2, i0, i1, b0, b1)
      })
  Platform.stdout_line (show seq)
";

fn run_determinism_case(name: &str, check_js_go: bool) {
    let dir = project(
        name,
        &[
            ("flick.toml", APP),
            ("src/App/Main.fl", DETERMINISM_MAIN),
            ("test/App/DeterminismTest.fl", DETERMINISM_SUITE),
        ],
    );
    if check_js_go {
        link_repo_node_modules(&dir);
    }

    let targets: &[(&str, &[&str])] = if check_js_go {
        &[
            ("vm", &["--target", "vm"]),
            ("js", &["--target", "js"]),
            ("go", &["--target", "go"]),
        ]
    } else {
        &[("vm", &["--target", "vm"])]
    };

    for &(label, extra) in targets {
        let mut args = vec!["test"];
        args.extend_from_slice(extra);
        args.extend(["--seed", "42", "--reporter", "json"]);
        let out = if label == "vm" {
            flick_vm_no_path(&dir, &args)
        } else {
            flick_vm(&dir, &args)
        };
        assert!(
            out.status.success(),
            "{name} test failed on {label}:\nstdout={}\nstderr={}",
            stdout(&out),
            stderr(&out)
        );
        let parsed: serde_json::Value =
            serde_json::from_str(&stdout(&out)).expect("valid JSON test report");
        assert_eq!(parsed["passed"], 2, "passed mismatch on {label}");
        assert_eq!(parsed["failed"], 0, "failed mismatch on {label}");
    }

    let vm_seq = flick_vm_no_path(&dir, &["run", "--target", "vm"]);
    assert!(
        vm_seq.status.success(),
        "Random.with_seed sequence failed on vm:\nstdout={}\nstderr={}",
        stdout(&vm_seq),
        stderr(&vm_seq)
    );
    if check_js_go {
        let js_seq = flick_vm(&dir, &["run", "--target", "js"]);
        let go_seq = flick_vm(&dir, &["run", "--target", "go"]);
        for (label, out) in [("js", &js_seq), ("go", &go_seq)] {
            assert!(
                out.status.success(),
                "Random.with_seed sequence failed on {label}:\nstdout={}\nstderr={}",
                stdout(out),
                stderr(out)
            );
            assert_eq!(
                stdout(&vm_seq),
                stdout(out),
                "vm vs {label} Random.with_seed"
            );
        }
    }
}

#[test]
fn vm_host_test_clock_and_random_with_seed() {
    run_determinism_case("vm_host_determinism_vm", false);
}

#[test]
#[ignore = "needs Node, Go and `pnpm install`; run `just e2e`"]
fn vm_host_test_clock_and_random_with_seed_parity() {
    run_determinism_case("vm_host_determinism_parity", true);
}
