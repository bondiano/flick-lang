use std::process::{Command, Output};

fn flick(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
        .env_remove("NO_COLOR")
        .output()
        .expect("flick runs")
}

fn stdout(args: &[&str]) -> String {
    let out = flick(args);
    assert!(out.status.success(), "{args:?} failed: {out:?}");
    String::from_utf8(out.stdout).expect("utf-8 stdout")
}

#[test]
fn version() {
    insta::assert_snapshot!(stdout(&["--version"]), @"flick 0.1.0 (Flick 0.7)");
}

#[test]
fn help() {
    insta::assert_snapshot!(stdout(&["help"]));
    assert_eq!(stdout(&["--help"]), stdout(&["help"]));
}

#[test]
fn help_check() {
    insta::assert_snapshot!(stdout(&["help", "check"]));
}

#[test]
fn bad_invocation_exits_2() {
    assert_eq!(flick(&["frobnicate"]).status.code(), Some(2));
    assert_eq!(flick(&["check"]).status.code(), Some(2));
    assert_eq!(
        flick(&["--color", "sometimes", "check", "x.fl"])
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn check_exit_codes_and_json_stream() {
    let ok = flick(&["check", "tests/check/pass/bootstrap/hello.fl"]);
    assert_eq!(ok.status.code(), Some(0));
    assert!(ok.stderr.is_empty() && ok.stdout.is_empty());

    let out = flick(&[
        "check",
        "--diagnostics",
        "json",
        "tests/check/fail/bootstrap/unsupported_expr.fl",
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
        ["FLICK-S002", "FLICK-S002", "FLICK-S002", "FLICK-P001"]
    );
}

#[test]
fn color_flag_controls_ansi() {
    let path = "tests/check/fail/bootstrap/bad_signature.fl";
    let always = flick(&["check", "--color", "always", path]).stderr;
    let never = flick(&["check", "--color", "never", path]).stderr;
    // piped stderr is not a terminal, so `auto` means no color
    let auto = flick(&["check", path]).stderr;
    assert!(always.contains(&0x1b));
    assert!(!never.contains(&0x1b));
    assert_eq!(auto, never);
}
