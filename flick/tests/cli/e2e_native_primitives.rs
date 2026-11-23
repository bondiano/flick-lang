//! End-to-end verification of reviewed native primitive folding (#805) across
//! JS (Effect and RT) and Go in both debug and release profiles, pairing folded
//! literal cases with runtime-supplied dynamic controls and inspecting emitted
//! release JS and Go artifacts.

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

use flick_core::build::go::executable;
use flick_core::build::runner_path;
use flick_core::project::config::MAIN_ENTRY;

const SOURCE: &str = r#"module App.Main exposing (main)

render_opt : Option Int -> String
let render_opt parsed =
  match parsed with
  | Some n ->
    let zero_ok = if n == 0 then show (Float.same_value 0.0 (Int.to_float n)) else "n/a"
    "Some " ++ Int.to_string n ++ " zero=" ++ zero_ok
  | None -> "None"

age : String -> Result String Int
let age text =
  match Int.parse text with
  | Some n when n >= 0 -> Ok n
  | Some _ -> Err "an age is not negative"
  | None -> Err "not a number: \{text}"

show_arg_like : String -> String
let show_arg_like text =
  let opens =
    String.starts_with "[" text
      || String.starts_with "(" text
      || String.starts_with "{" text
      || String.starts_with "\"" text
  if opens || not (String.contains " " text) then text else "(" ++ text ++ ")"

main : Unit ->{Host.Console, Host.Arguments; AppError} Unit
let main () =
  let earth = "a\u{1F30D}b"
  // Folded closed literal cases:
  Platform.stdout_line ("lit:len_earth=" ++ show (String.length earth))
  Platform.stdout_line ("lit:len_lone=" ++ show (String.length (String.slice 1 2 earth)))
  Platform.stdout_line ("lit:lone_show=" ++ show (String.slice 1 2 earth))
  Platform.stdout_line ("lit:low_show=" ++ show (String.slice 2 3 earth))
  Platform.stdout_line ("lit:valid_slice=" ++ String.slice 1 3 earth)
  Platform.stdout_line ("lit:rejoined=" ++ (String.slice 0 2 earth ++ String.slice 2 4 earth))
  Platform.stdout_line ("lit:parse_ascii=" ++ render_opt (Int.parse "36"))
  Platform.stdout_line ("lit:parse_unicode=" ++ render_opt (Int.parse "\u{FEFF}36\u{00A0}"))
  Platform.stdout_line ("lit:parse_neg_zero=" ++ render_opt (Int.parse "-0"))
  Platform.stdout_line ("lit:parse_min=" ++ render_opt (Int.parse "-2147483648"))
  Platform.stdout_line ("lit:parse_max=" ++ render_opt (Int.parse "2147483647"))
  Platform.stdout_line ("lit:parse_over=" ++ render_opt (Int.parse "2147483648"))
  Platform.stdout_line ("lit:parse_under=" ++ render_opt (Int.parse "-2147483649"))
  Platform.stdout_line ("lit:parse_nel=" ++ render_opt (Int.parse "\u{0085}36"))
  Platform.stdout_line ("lit:parse_bad=" ++ render_opt (Int.parse "ada"))
  Platform.stdout_line ("lit:age=" ++ show (age "36", age "-1", age "ada"))
  Platform.stdout_line (
    "lit:preds=" ++ show (
      show_arg_like "36",
      show_arg_like "a b",
      show_arg_like "[1, 2]",
      String.ends_with "b" earth,
      String.contains "\u{1F30D}" earth
    )
  )
  // Runtime-supplied dynamic controls (arguments passed via CLI):
  match Platform.args () with
  | [dyn_earth, dyn_num, dyn_neg, dyn_bad, dyn_over, dyn_nel, dyn_spaced] ->
    Platform.stdout_line ("dyn:len_earth=" ++ show (String.length dyn_earth))
    Platform.stdout_line ("dyn:len_lone=" ++ show (String.length (String.slice 1 2 dyn_earth)))
    Platform.stdout_line ("dyn:lone_show=" ++ show (String.slice 1 2 dyn_earth))
    Platform.stdout_line ("dyn:low_show=" ++ show (String.slice 2 3 dyn_earth))
    Platform.stdout_line ("dyn:valid_slice=" ++ String.slice 1 3 dyn_earth)
    Platform.stdout_line ("dyn:rejoined=" ++ (String.slice 0 2 dyn_earth ++ String.slice 2 4 dyn_earth))
    Platform.stdout_line ("dyn:parse_unicode=" ++ render_opt (Int.parse dyn_num))
    Platform.stdout_line ("dyn:parse_neg_zero=" ++ render_opt (Int.parse dyn_neg))
    Platform.stdout_line ("dyn:parse_bad=" ++ render_opt (Int.parse dyn_bad))
    Platform.stdout_line ("dyn:parse_over=" ++ render_opt (Int.parse dyn_over))
    Platform.stdout_line ("dyn:parse_nel=" ++ render_opt (Int.parse dyn_nel))
    Platform.stdout_line ("dyn:age=" ++ show (age dyn_num, age "-1", age dyn_bad))
    Platform.stdout_line (
      "dyn:preds=" ++ show (
        show_arg_like "36",
        show_arg_like dyn_spaced,
        show_arg_like "[1, 2]",
        String.ends_with "b" dyn_earth,
        String.contains "\u{1F30D}" dyn_earth
      )
    )
  | _ -> Platform.stdout_line "unexpected args"
"#;

fn run(command: &mut Command) -> String {
    let output = command.output().expect("runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn prepare_dir() -> PathBuf {
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR"));
    fs::create_dir_all(tmp).expect("tmpdir");
    let dir = fs::canonicalize(tmp)
        .expect("canonical tmpdir")
        .join("native_primitives_805");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("dir");
    #[cfg(unix)]
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
    dir
}

fn assert_release_artifact(target: &str, out: &Path) {
    if target == "js" {
        let emitted = fs::read_to_string(out.join("App/Main.js")).expect("emitted App/Main.js");
        for needle in [
            "\"lit:len_earth=4\"",
            "\"lit:len_lone=1\"",
            "\"lit:valid_slice=🌍\"",
            "\"lit:rejoined=a🌍b\"",
            "\"lit:parse_over=None\"",
            "\"lit:parse_nel=None\"",
            "slice",
            "length(dyn_earth)",
            "parse(dyn_num)",
        ] {
            assert!(emitted.contains(needle), "missing {needle} in:\n{emitted}");
        }
        assert!(
            !emitted.contains('\u{FFFD}'),
            "emitted JS must never substitute U+FFFD:\n{emitted}"
        );
    } else if target == "go" {
        let emitted =
            fs::read_to_string(out.join("go/main/m_App_Main.go")).expect("emitted Go module");
        for needle in [
            "\"lit:len_earth=4\"",
            "\"lit:len_lone=1\"",
            "\"lit:valid_slice=\\U0001f30d\"",
            "\"lit:rejoined=a\\U0001f30db\"",
            "\"lit:parse_over=None\"",
            "\"lit:parse_nel=None\"",
            "F_String__slice",
            "F_String__length",
            "F_Int__parse",
        ] {
            assert!(emitted.contains(needle), "missing {needle} in:\n{emitted}");
        }
        assert!(
            !emitted.contains('\u{FFFD}'),
            "emitted Go must never substitute U+FFFD:\n{emitted}"
        );
    }
}

#[test]
#[ignore = "needs Node, Go and `pnpm install`; run `just e2e`"]
fn native_primitives_fold_and_match_dynamic_controls_across_js_and_go() {
    let dir = prepare_dir();
    let source = dir.join("Main.fl");
    fs::write(&source, SOURCE).expect("write");

    let expected_stdout = concat!(
        "lit:len_earth=4\n",
        "lit:len_lone=1\n",
        "lit:lone_show=\"\\ud83c\"\n",
        "lit:low_show=\"\\udf0d\"\n",
        "lit:valid_slice=🌍\n",
        "lit:rejoined=a🌍b\n",
        "lit:parse_ascii=Some 36 zero=n/a\n",
        "lit:parse_unicode=Some 36 zero=n/a\n",
        "lit:parse_neg_zero=Some 0 zero=true\n",
        "lit:parse_min=Some -2147483648 zero=n/a\n",
        "lit:parse_max=Some 2147483647 zero=n/a\n",
        "lit:parse_over=None\n",
        "lit:parse_under=None\n",
        "lit:parse_nel=None\n",
        "lit:parse_bad=None\n",
        "lit:age=(Ok 36, Err \"an age is not negative\", Err \"not a number: ada\")\n",
        "lit:preds=(\"36\", \"(a b)\", \"[1, 2]\", true, true)\n",
        "dyn:len_earth=4\n",
        "dyn:len_lone=1\n",
        "dyn:lone_show=\"\\ud83c\"\n",
        "dyn:low_show=\"\\udf0d\"\n",
        "dyn:valid_slice=🌍\n",
        "dyn:rejoined=a🌍b\n",
        "dyn:parse_unicode=Some 36 zero=n/a\n",
        "dyn:parse_neg_zero=Some 0 zero=true\n",
        "dyn:parse_bad=None\n",
        "dyn:parse_over=None\n",
        "dyn:parse_nel=None\n",
        "dyn:age=(Ok 36, Err \"an age is not negative\", Err \"not a number: ada\")\n",
        "dyn:preds=(\"36\", \"(a b)\", \"[1, 2]\", true, true)\n",
    );

    let cli_args = [
        "a🌍b",
        "\u{FEFF}36\u{00A0}",
        "-0",
        "ada",
        "2147483648",
        "\u{0085}36",
        "a b",
    ];

    let targets: &[(&str, &str, &[&str])] = &[
        ("js-effect", "js", &["--runtime", "effect"]),
        ("js-rt", "js", &["--runtime", "rt"]),
        ("go", "go", &["--allow-tool", "go"]),
    ];

    for &(label, target, extra_flags) in targets {
        for profile in ["debug", "release"] {
            let out = dir.join(format!("{label}-{profile}"));
            run(Command::new(env!("CARGO_BIN_EXE_flick"))
                .arg("build")
                .arg(&source)
                .arg("--out-dir")
                .arg(&out)
                .arg("--target")
                .arg(target)
                .arg("--profile")
                .arg(profile)
                .args(extra_flags));

            if profile == "release" {
                assert_release_artifact(target, &out);
            }

            let mut cmd = if target == "go" {
                Command::new(out.join(executable(MAIN_ENTRY)))
            } else {
                let mut node = Command::new("node");
                node.arg(out.join(runner_path(false)));
                node
            };
            cmd.args(cli_args);
            let actual = run(&mut cmd);
            assert_eq!(actual, expected_stdout, "target={label} profile={profile}");
        }
    }
}
