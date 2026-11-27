//! CLI integration tests for Tier 1 std intrinsics and `examples/mtl` CLI parity.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use crate::common::example;

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
fn run_tier1_executes_std_schema_json_temporal_numeric_and_arbitrary() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for rel in [
        "tests/run/schema/derive.fl",
        "tests/run/schema/json_wire.fl",
        "tests/run/schema/enums.fl",
        "tests/run/schema/combinators.fl",
        "tests/run/schema/route_nested.fl",
        "tests/run/stdlib/effect/schema.fl",
        "tests/run/stdlib/effect/codec.fl",
        "tests/run/stdlib/effect/check.fl",
        "tests/run/stdlib/effect/json_schema.fl",
        "tests/run/stdlib/effect/duration.fl",
        "tests/run/stdlib/effect/duration_text.fl",
        "tests/run/stdlib/effect/datetime.fl",
        "tests/run/stdlib/effect/datetime_more.fl",
        "tests/run/stdlib/decimals.fl",
        "tests/run/codegen/bigints.fl",
        "tests/run/stdlib/effect/arbitrary.fl",
    ] {
        let path = repo.join(rel);
        let expected = fs::read_to_string(path.with_extension("stdout"))
            .unwrap_or_else(|e| panic!("read expected stdout for {rel}: {e}"));
        let out = flick_no_path(&repo, &["run", "--target", "vm", "--vm-tier", "1", rel]);
        assert!(
            out.status.success(),
            "{rel} (--vm-tier 1) failed:\n{}",
            stderr(&out)
        );
        assert_eq!(
            stdout(&out),
            expected,
            "{rel} (--vm-tier 1) stdout mismatch"
        );
    }
}

struct MtlCase {
    args: &'static [&'static str],
    code: i32,
    out: &'static [&'static str],
    err: &'static [&'static str],
}

const MTL_CASES: &[MtlCase] = &[
    MtlCase {
        args: &["catalog"],
        code: 0,
        out: &[
            "Available tasks (before per-task overhead):",
            "docs: 20 min",
            "tests: 30 min",
            "lint: 10 min",
        ],
        err: &[],
    },
    MtlCase {
        args: &["plan", "--budget", "60", "docs", "lint"],
        code: 0,
        out: &[
            "Budget: 60 min; overhead: 0 min/task",
            "Completed: docs, lint",
            "Remaining: 30 min",
            "Plan complete.",
        ],
        err: &[],
    },
    MtlCase {
        args: &["plan", "--budget", "36", "--overhead", "3", "docs", "lint"],
        code: 0,
        out: &[
            "Budget: 36 min; overhead: 3 min/task",
            "Completed: docs, lint",
            "Remaining: 0 min",
            "Plan complete.",
        ],
        err: &[],
    },
    MtlCase {
        args: &["plan", "--budget", "25", "docs", "lint", "tests"],
        code: 1,
        out: &["Completed: docs", "Remaining: 5 min"],
        err: &["Plan stopped. Unscheduled: lint, tests."],
    },
    MtlCase {
        args: &["--help"],
        code: 0,
        out: &["DESCRIPTION", "USAGE", "SUBCOMMANDS", "plan", "catalog"],
        err: &[],
    },
    MtlCase {
        args: &["plan", "--help"],
        code: 0,
        out: &["USAGE", "--budget, -b", "--overhead, -o", "tasks"],
        err: &[],
    },
    MtlCase {
        args: &["--version"],
        code: 0,
        out: &["mtl v0.0.0\n"],
        err: &[],
    },
    MtlCase {
        args: &["plan", "--budget", "abc", "docs"],
        code: 1,
        out: &["USAGE", "--budget, -b"],
        err: &["Invalid value for flag --budget: \"abc\". Expected: not a number: abc"],
    },
    MtlCase {
        args: &["plan", "--budget", "-1", "docs"],
        code: 1,
        out: &["USAGE", "--budget, -b"],
        err: &["Invalid value for flag --budget: \"-1\". Expected: not a valid Minutes: -1"],
    },
    MtlCase {
        args: &["--completions", "bash"],
        code: 0,
        out: &[
            "###-begin-mtl-completions-###",
            "###-end-mtl-completions-###",
        ],
        err: &[],
    },
    MtlCase {
        args: &["--completions", "zsh"],
        code: 0,
        out: &["#compdef mtl", "_mtl()", "###-end-mtl-completions-###"],
        err: &[],
    },
    MtlCase {
        args: &["--completions", "fish"],
        code: 0,
        out: &[
            "###-begin-mtl-completions-###",
            "complete -c mtl",
            "###-end-mtl-completions-###",
        ],
        err: &[],
    },
    MtlCase {
        args: &["--completions", "powershell"],
        code: 0,
        out: &[
            "###-begin-mtl-completions-###",
            "Register-ArgumentCompleter",
            "###-end-mtl-completions-###",
        ],
        err: &[],
    },
];

#[test]
fn run_mtl_example_cli_on_tier0_and_tier1() {
    let dir = example("mtl", "examples/mtl-vm-cli");

    let build = flick_no_path(&dir, &["build", "--target", "vm", "--out-dir", "dist-vm"]);
    assert!(
        build.status.success(),
        "examples/mtl build --target vm failed:\n{}",
        stderr(&build)
    );
    let image = dir.join("dist-vm/main.fvm");
    let image_str = image.to_string_lossy().into_owned();

    let run_tier = |tier: &str, args: &[&str]| -> Output {
        let mut full = vec!["run", "--target", "vm", "--vm-tier", tier, &image_str, "--"];
        full.extend_from_slice(args);
        flick_no_path(&dir, &full)
    };

    for case in MTL_CASES {
        let t0 = run_tier("0", case.args);
        let t1 = run_tier("1", case.args);
        assert_eq!(
            t0.status.code(),
            Some(case.code),
            "tier0 {:?} status: {t0:?}",
            case.args
        );
        assert_eq!(
            t1.status.code(),
            Some(case.code),
            "tier1 {:?} status: {t1:?}",
            case.args
        );
        assert_eq!(
            stdout(&t0),
            stdout(&t1),
            "tier0 vs tier1 stdout mismatch for {:?}",
            case.args
        );
        assert_eq!(
            stderr(&t0),
            stderr(&t1),
            "tier0 vs tier1 stderr mismatch for {:?}",
            case.args
        );
        let out_text = stdout(&t1);
        let err_text = stderr(&t1);
        for needle in case.out {
            assert!(
                out_text.contains(needle),
                "{:?} stdout missing {needle:?}:\n{out_text}",
                case.args
            );
        }
        for needle in case.err {
            assert!(
                err_text.contains(needle),
                "{:?} stderr missing {needle:?}:\n{err_text}",
                case.args
            );
        }
    }

    // Also run directly from source (without pre-building `.fvm`) on tier 1.
    let direct = flick_no_path(
        &dir,
        &["run", "--target", "vm", "--vm-tier", "1", "--", "catalog"],
    );
    assert!(
        direct.status.success(),
        "direct tier1 run failed:\n{}",
        stderr(&direct)
    );
    assert!(stdout(&direct).contains("docs: 20 min"));
}
