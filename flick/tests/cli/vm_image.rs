//! CLI integration tests for `flick build --target vm` and `flick run --target vm <image.fvm>`.

use std::env;
use std::fs;
use std::path::Path;
use std::process::{self, Command, Output};

use crate::common::{APP, project};

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn flick_no_path(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(dir)
        .env("PATH", "")
        .output()
        .expect("flick runs without PATH")
}

#[test]
fn build_target_vm_emits_fvm_image_and_run_executes_it_standalone() {
    let main = "\
module App.Main exposing (main)

main : Unit ->{Host.Console, Host.Arguments} Unit
let main () =
  let args = Platform.args ()
  Platform.stdout_line (\"hello from fvm: \" ++ show args)
";
    let dir = project(
        "vm_build_and_run_fvm",
        &[("flick.toml", APP), ("src/App/Main.fl", main)],
    );

    for profile in ["debug", "release"] {
        let out_dir = format!("out-{profile}");
        let build_out = flick_no_path(
            &dir,
            &[
                "build",
                "--target",
                "vm",
                "--profile",
                profile,
                "--out-dir",
                &out_dir,
            ],
        );
        assert!(
            build_out.status.success(),
            "flick build --target vm ({profile}) failed:\n{}",
            stderr(&build_out)
        );
        let image_path = dir.join(&out_dir).join("main.fvm");
        assert!(
            image_path.is_file(),
            "expected built VM image at {}",
            image_path.display()
        );
        let bytes = fs::read(&image_path).expect("read main.fvm");
        assert!(
            bytes.starts_with(b"FLKM"),
            "expected FLKM magic header in {}",
            image_path.display()
        );

        // Execute the built `.fvm` from an isolated directory without `flick.toml` or `PATH`.
        let isolated = env::temp_dir().join(format!(
            "flick_fvm_run_isolated_{}_{profile}",
            process::id()
        ));
        let _ = fs::create_dir_all(&isolated);
        let run_out = flick_no_path(
            &isolated,
            &[
                "run",
                "--target",
                "vm",
                &image_path.to_string_lossy(),
                "--",
                "alpha",
                "beta",
            ],
        );
        assert!(
            run_out.status.success(),
            "flick run --target vm ({profile}) failed:\n{}",
            stderr(&run_out)
        );
        assert_eq!(stdout(&run_out), "hello from fvm: [\"alpha\", \"beta\"]\n");
        let _ = fs::remove_dir_all(&isolated);
    }
}

#[test]
fn build_target_vm_handles_standalone_fl_and_library_and_rejects_corrupt_images() {
    let dir = project(
        "vm_build_standalone_and_corrupt",
        &[(
            "hello.fl",
            "module Hello exposing (main)\n\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line \"standalone fvm\"\n",
        )],
    );

    let build_out = flick_no_path(
        &dir,
        &["build", "--target", "vm", "hello.fl", "--out-dir", "dist"],
    );
    assert!(
        build_out.status.success(),
        "standalone build --target vm failed:\n{}",
        stderr(&build_out)
    );
    let image_path = dir.join("dist/main.fvm");
    assert!(image_path.is_file());

    for tier in ["0", "1"] {
        let run_out = flick_no_path(
            &dir,
            &[
                "run",
                "--target",
                "vm",
                "--vm-tier",
                tier,
                &image_path.to_string_lossy(),
            ],
        );
        assert!(
            run_out.status.success(),
            "run standalone main.fvm --vm-tier {tier} failed:\n{}",
            stderr(&run_out)
        );
        assert_eq!(stdout(&run_out), "standalone fvm\n");
    }

    // Truncated image fails cleanly without panicking.
    let valid_bytes = fs::read(&image_path).expect("read valid image");
    let truncated_path = dir.join("dist/truncated.fvm");
    fs::write(&truncated_path, &valid_bytes[..40]).expect("write truncated");
    let trunc_run = flick_no_path(
        &dir,
        &["run", "--target", "vm", &truncated_path.to_string_lossy()],
    );
    assert_eq!(trunc_run.status.code(), Some(1), "{trunc_run:?}");
    assert!(
        stderr(&trunc_run).contains("FLICK-P012") && stderr(&trunc_run).contains("truncated"),
        "expected clean P012 truncation error, got:\n{}",
        stderr(&trunc_run)
    );

    // Compiler-hash mismatch reports BOTH expected and found hashes cleanly.
    let mut bad_hash = valid_bytes;
    bad_hash[8] ^= 0xff;
    let bad_hash_path = dir.join("dist/bad_hash.fvm");
    fs::write(&bad_hash_path, &bad_hash).expect("write bad_hash");
    let hash_run = flick_no_path(
        &dir,
        &["run", "--target", "vm", &bad_hash_path.to_string_lossy()],
    );
    assert_eq!(hash_run.status.code(), Some(1), "{hash_run:?}");
    let err = stderr(&hash_run);
    assert!(
        err.contains("FLICK-P012")
            && err.contains("compiler hash mismatch")
            && err.contains("expected ")
            && err.contains("found "),
        "expected both hashes in error message, got:\n{err}"
    );
}
