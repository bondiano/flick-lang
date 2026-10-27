mod common;

use std::fs;

use common::flick;

#[test]
fn check_exit_codes_and_json_stream() {
    let ok = flick(&["check", "tests/check/pass/bootstrap/hello.fl"]);
    assert_eq!(ok.status.code(), Some(0));
    assert!(ok.stderr.is_empty() && ok.stdout.is_empty());

    let out = flick(&[
        "check",
        "--diagnostics",
        "json",
        "tests/check/fail/syntax/other_languages.fl",
        "missing.fl",
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty(), "diagnostics never go to stdout");
    let stderr = String::from_utf8(out.stderr).expect("utf-8 stderr");
    let codes: Vec<String> = stderr
        .lines()
        .map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).expect("NDJSON line");
            assert_eq!(v["schema-version"], 1);
            assert_eq!(v["type"], "diagnostic");
            v["code"].as_str().expect("code").to_owned()
        })
        .collect();
    assert_eq!(
        codes,
        [
            "FLICK-S010",
            "FLICK-S010",
            "FLICK-T001",
            "FLICK-R013",
            "FLICK-S010",
            "FLICK-S010",
            "FLICK-S010",
            "FLICK-P001"
        ]
    );
}

#[test]
fn color_flag_controls_ansi() {
    let path = "tests/check/fail/syntax/recovery.fl";
    let always = flick(&["check", "--color", "always", path]).stderr;
    let never = flick(&["check", "--color", "never", path]).stderr;
    // piped stderr is not a terminal, so `auto` means no color
    let auto = flick(&["check", path]).stderr;
    assert!(always.contains(&0x1b));
    assert!(!never.contains(&0x1b));
    assert_eq!(auto, never);
}

#[test]
fn fix_removes_annotations_the_values_already_have() {
    let text = r#"module App.Kinds exposing (..)

type Kind = A | B

let kinds : List Kind = [A, B]

let none : List Kind = []

type alias Id = String

let id : Id = "x"

type Callback = Callback (Unit -> Number)

let free : Callback^{} = Callback (fun () -> 1)

widen : forall c. Unit -> Callback^c
let widen () =
  let value : Callback^c = free
  value

make_cell : Unit ->{} Ref (Option (Callback^{}))
let make_cell () =
  let cell : Ref (Option (Callback^{})) = Ref.make None
  cell
"#;
    let dir = common::project(
        "check_fix",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"lib\"\n",
            ),
            ("src/App/Kinds.fl", text),
        ],
    );
    let out = common::flick_in(&dir, &["check", "--fix"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(out.stderr.is_empty(), "nothing left to report: {out:?}");
    let fixed = fs::read_to_string(dir.join("src/App/Kinds.fl")).expect("source");
    assert_eq!(fixed, text.replace("kinds : List Kind =", "kinds ="));
}
