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

type Callback = Callback (Unit -> Float)

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

#[test]
fn fix_rewrites_the_flick_blocks_of_a_markdown_page() {
    let page = r#"---
title: Kinds
---

Prose with two trailing spaces  
and `let kinds : List Kind = [A, B]` inline.

```flick
# type Kind = A | B
#
let kinds : List Kind = [A, B]
```

```flick
# firsts : Option Float -> Option Float
let firsts o =
  do {
    y <- o
    pure y
  }
# let more = 1
```

```flick ignore
let kinds : List Kind = [A, B]
```

```flick keep
# type Kind = A | B
let kinds : List Kind = [A, B]
```

```flick fail
# type Kind = A | B
let kinds : List Kind = [A, B]
let wrong : Int = "no"
```

```flick
# let shown = 1
let wrong : Int = "no"
```
"#;
    let dir = common::project("check_fix_page", &[("page.md", page)]);
    let run = || common::flick_in(&dir, &["check", "--fix", "--color", "never", "page.md"]);
    let out = run();
    let fixed = fs::read_to_string(dir.join("page.md")).expect("page");
    // only the default blocks change, and the line count with them; `keep`
    // is checked but left as written
    let expected = page
        .replace("#\nlet kinds : List Kind =", "#\nlet kinds =")
        .replace("  do {\n    y <- o\n    pure y\n  }\n", "  o\n");
    assert_eq!(fixed, expected, "{out:?}");
    // the error left in the last block, at its line and column of the page
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).expect("utf-8 stderr");
    let at = fixed.rfind("let wrong").expect("the line");
    let line = 1 + fixed[..at].matches('\n').count();
    assert!(
        stderr.contains(&format!("--> page.md:{line}:19")),
        "{stderr}"
    );
    assert_eq!(stderr.matches("error[").count(), 1, "{stderr}");
    // a second run has nothing left to fix
    let again = run();
    assert_eq!(
        fs::read_to_string(dir.join("page.md")).expect("page"),
        fixed
    );
    assert_eq!(String::from_utf8(again.stderr).expect("utf-8"), stderr);
}

#[test]
fn fix_only_applies_the_fixes_of_the_codes_given() {
    let text = "module App.Waits exposing (..)\n\n\
                sleep : Float ->{} Unit\n\
                foreign async sleep from \"./timers.mjs\" \"sleep\"\n\n\
                nap : Unit ->{} Unit\n\
                let nap () = sleep 1\n";
    let files = [
        (
            "flick.toml",
            "schema-version = 1\n[project]\nkind = \"lib\"\n",
        ),
        ("src/App/Waits.fl", text),
    ];
    let dir = common::project("check_fix_only", &files);
    let out = common::flick_in(&dir, &["check", "--fix", "--only", "F001"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "`nap` still lacks `Async`: {out:?}"
    );
    let fixed = fs::read_to_string(dir.join("src/App/Waits.fl")).expect("source");
    let row = "sleep : Float ->{Async;} Unit";
    assert_eq!(fixed, text.replace("sleep : Float ->{} Unit", row));

    // the fix of every code reaches the caller, one pass after the other
    let out = common::flick_in(&dir, &["check", "--fix"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let fixed = fs::read_to_string(dir.join("src/App/Waits.fl")).expect("source");
    assert!(fixed.contains("nap : Unit ->{Async;} Unit"), "{fixed}");
}
