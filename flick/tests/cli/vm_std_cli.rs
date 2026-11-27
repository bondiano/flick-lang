//! CLI integration tests for Tier 1 std intrinsics and `examples/mtl` CLI parity.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use crate::common::{APP, example, project};

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

#[test]
fn test_mtl_example_all_suites_on_default_tier1_and_tier0_oracle() {
    let dir = example("mtl", "examples/mtl-vm-tests");

    // Default `--target vm` runs on Tier 1 bytecode with empty PATH (no Node/Go).
    let t1 = flick_no_path(
        &dir,
        &[
            "test",
            "--target",
            "vm",
            "--seed",
            "7",
            "--reporter",
            "json",
        ],
    );
    assert!(
        t1.status.success(),
        "examples/mtl `flick test --target vm` (default tier 1) failed:\nstdout={}\nstderr={}",
        stdout(&t1),
        stderr(&t1)
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout(&t1)).expect("valid JSON test report");
    assert_eq!(parsed["seed"], 7);
    assert_eq!(parsed["passed"], 159);
    assert_eq!(parsed["failed"], 0);
    let eff_laws_count = parsed["tests"]
        .as_array()
        .expect("tests array")
        .iter()
        .filter(|t| {
            t["name"]
                .as_str()
                .is_some_and(|n| n.starts_with("Mtl.EffLawsTest > "))
        })
        .count();
    assert_eq!(
        eff_laws_count, 8,
        "expected all 8 Mtl.EffLawsTest cases (including 100-draw properties)"
    );

    // Tier 0 oracle (`--vm-tier 0`) produces the exact same JSON report.
    let t0 = flick_no_path(
        &dir,
        &[
            "test",
            "--target",
            "vm",
            "--vm-tier",
            "0",
            "--seed",
            "7",
            "--reporter",
            "json",
        ],
    );
    assert!(
        t0.status.success(),
        "examples/mtl `flick test --target vm --vm-tier 0` failed:\n{}",
        stderr(&t0)
    );
    assert_eq!(
        stdout(&t1),
        stdout(&t0),
        "examples/mtl Tier 1 vs Tier 0 JSON report mismatch"
    );
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

const SEEDED_PROPERTY_SUITE: &str = r#"
module App.SeededPropTest exposing (tests)

import Arbitrary
import Check
import Codec
import Gen
import Test

type alias Point = { x : Float; tags : List String }
  deriving Schema

digit_json : PureCodec Json Float
let digit_json =
  Float.json_finite
  |> Check.apply Check.int
  |> Check.apply
    (Check.between { minimum = 0; maximum = 9; exclusiveMinimum = false; exclusiveMaximum = false })

digits : Arbitrary.Arbitrary Float
let digits = Arbitrary.from_schema (Codec.to_schema digit_json)

checking : Arbitrary.CheckOptions
let checking = {
  runs = Some 100
  size = None
  maxDiscards = None
  maxShrinks = None
  seed = Some 7
  replay = None
}

pinned_int_cases : List Int
let pinned_int_cases = [
  202208, -46191, -558048, -20317, -922417, 96628, 247703, 931535, 489584, 135634, 359278, -250626,
  916494, 216834, -16668, -769682, 98643, 507116, 568819, 427535, -609222, -423551, 180495, 859010,
  -395172, 615034, 720801, 672635, 251607, -790819, -438095, 76297, -486732, -610289, 953480,
  937684, 16764, -964626, 767853, 584696, 739401, 590609, 2972, 461570, -666894, -715021, -64792,
  -459712, 830058, -388776, -592934, 213981, 40998, 271176, 126953, -976890, 596074, 743609, 82469,
  196563, -101398, 603449, 661356, 906209, -936839, 101028, -788979, -679605, -81820, -765684,
  -523439, -515834, 253364, -766857, 606339, -531515, -741005, -364553, -985569, 579355, -738978,
  324427, 570298, 259763, -614361, 509425, 395607, 747059, -490144, -712442, 612125, -266981,
  -585081, 61987, -722415, -717816, -614799, 985818, 473932, 908747,
]

tests : List Test.Test
let tests = [
  Test.group "seeded parity" [
    Test.property "Gen.int 100 draws match seed 42" (Gen.int (-1000000) 1000000) (fun n ->
      Test.is_true ("unexpected draw: " ++ show n) (List.contains n pinned_int_cases)
    ),
    Test.property "failing Test.property reports exact counterexample draw and run index" (Gen.int 0 1000) (fun n ->
      Test.is_true ("counterexample draw=" ++ show n) (n < 500)
    ),
    Test.effect_test "Arbitrary.sample draws exact sequence for seed 42" (task {
      let opts : Arbitrary.SampleOptions = {
        count = Some 10
        size = None
        maxDiscards = None
        seed = Some 42
      }
      let drawn = Arbitrary.sample opts digits
      Test.expect (List.length drawn) |> Test.to.equal 10
      Test.expect (List.all (fun n -> n >= 0 && n <= 9) drawn) |> Test.to.be_true
    }),
    Test.effect_test "failing Arbitrary.check shrinks scalar deterministically" (task {
      let result = Arbitrary.check checking (fun n -> n < 5) digits
      let msg = Option.with_default "expected failure" (Arbitrary.format_check_failure result)
      Test.check (Test.Failed msg)
    }),
    Test.effect_test "failing Arbitrary.check shrinks structured record deterministically" (task {
      let result =
        Arbitrary.check checking (fun p -> p.x < 5) (Arbitrary.from_schema Point.schema)
      let msg = Option.with_default "expected failure" (Arbitrary.format_check_failure result)
      Test.check (Test.Failed msg)
    }),
  ],
]
"#;

fn assert_seeded_property_report(report: &serde_json::Value) {
    assert_eq!(report["seed"], 42);
    assert_eq!(report["passed"], 2);
    assert_eq!(report["failed"], 3);
    let prop_msg = report["tests"][1]["message"]
        .as_str()
        .expect("property failure message");
    assert_eq!(
        prop_msg, "counterexample draw=601 (case 0: 601)",
        "expected seeded property counterexample, got:\n{prop_msg}"
    );
    let scalar_msg = report["tests"][3]["message"]
        .as_str()
        .expect("scalar failure message");
    assert!(
        scalar_msg.contains("Shrunk input: 5"),
        "expected shrunk scalar input 5, got:\n{scalar_msg}"
    );
    let record_msg = report["tests"][4]["message"]
        .as_str()
        .expect("record failure message");
    assert!(
        record_msg.contains("Shrunk input:"),
        "expected shrunk record input, got:\n{record_msg}"
    );
}

fn run_seeded_property_parity(name: &str, check_js_go: bool) {
    let manifest = format!("{APP}\n[target.js]\nruntime = \"effect\"\n");
    let dir = project(
        name,
        &[
            ("flick.toml", manifest.as_str()),
            (
                "src/App/Main.fl",
                "module App.Main exposing (main)\n\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
            ("test/App/SeededPropTest.fl", SEEDED_PROPERTY_SUITE),
        ],
    );
    if check_js_go {
        link_repo_node_modules(&dir);
    }

    let vm_t1 = flick_no_path(
        &dir,
        &[
            "test",
            "--target",
            "vm",
            "--seed",
            "42",
            "--reporter",
            "json",
        ],
    );
    assert_eq!(vm_t1.status.code(), Some(1), "vm tier1: {vm_t1:?}");
    let vm_t1_json: serde_json::Value =
        serde_json::from_str(&stdout(&vm_t1)).expect("valid vm tier1 JSON");
    assert_seeded_property_report(&vm_t1_json);

    // Tier 0 oracle must match Tier 1 identically.
    let vm_t0 = flick_no_path(
        &dir,
        &[
            "test",
            "--target",
            "vm",
            "--vm-tier",
            "0",
            "--seed",
            "42",
            "--reporter",
            "json",
        ],
    );
    assert_eq!(vm_t0.status.code(), Some(1), "vm tier0: {vm_t0:?}");
    assert_eq!(
        stdout(&vm_t1),
        stdout(&vm_t0),
        "VM tier 1 vs tier 0 seeded property/shrink report mismatch"
    );

    if check_js_go {
        for target in ["js", "go"] {
            let out = Command::new(env!("CARGO_BIN_EXE_flick"))
                .args([
                    "test",
                    "--target",
                    target,
                    "--seed",
                    "42",
                    "--reporter",
                    "json",
                ])
                .current_dir(&dir)
                .output()
                .expect("flick runs");
            assert_eq!(
                out.status.code(),
                Some(1),
                "{target} exit status: {out:?}\nstderr={}",
                stderr(&out)
            );
            assert_eq!(
                stdout(&vm_t1),
                stdout(&out),
                "VM (tier 1) vs {target} seeded property draw/shrink report mismatch (stderr={})",
                stderr(&out)
            );
        }
    }
}

#[test]
fn seeded_property_draws_and_shrunk_counterexample_match_tier0_and_tier1() {
    run_seeded_property_parity("vm_seeded_prop_tier01", false);
}

#[test]
#[ignore = "needs Node, Go and `pnpm install`; run `just e2e`"]
fn seeded_property_draw_and_shrink_parity_across_vm_js_and_go() {
    run_seeded_property_parity("vm_seeded_prop_parity", true);
}
