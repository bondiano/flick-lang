mod common;

use std::{fs, path};

use common::{flick, flick_stdin};

const MESSY: &str = "let  f x   =\n    x+1\n";
const TIDY: &str = "let f x = x + 1\n";

#[test]
fn fmt_stdin_omits_an_empty_error_row() {
    let out = flick_stdin(&["fmt", "--stdin"], "main : Unit ->{Async; none} Unit\n");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "main : Unit ->{Async} Unit\n"
    );
}

#[test]
fn fmt_stdin_for_editors() {
    let out = flick_stdin(&["fmt", "--stdin", "--stdin-filepath", "src/A.fl"], MESSY);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&out.stdout), TIDY);

    let check = |input| {
        flick_stdin(&["fmt", "--stdin", "--check"], input)
            .status
            .code()
    };
    assert_eq!(check(TIDY), Some(0));
    assert_eq!(check(MESSY), Some(1));

    let broken = flick_stdin(
        &[
            "fmt",
            "--stdin",
            "--stdin-filepath",
            "src/A.fl",
            "--color",
            "never",
        ],
        "let = 1\n",
    );
    assert_eq!(broken.status.code(), Some(1));
    assert!(
        broken.stdout.is_empty(),
        "no output for a file with syntax errors"
    );
    let stderr = String::from_utf8_lossy(&broken.stderr);
    assert!(
        stderr.contains("FLICK-S007") && stderr.contains("src/A.fl:1:5"),
        "{stderr}"
    );
}

#[test]
fn fmt_files_in_place_and_check() {
    let dir = path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("fmt_files");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("nested")).expect("tmp dir");
    let messy = dir.join("nested/messy.fl");
    let tidy = dir.join("tidy.fl");
    fs::write(&messy, MESSY).expect("write");
    fs::write(&tidy, TIDY).expect("write");
    let dir_arg = dir.to_str().expect("utf-8 path");

    let check = flick(&["fmt", "--check", "--diagnostics", "json", dir_arg]);
    assert_eq!(check.status.code(), Some(1));
    let stderr = String::from_utf8(check.stderr).expect("utf-8");
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    assert!(
        stderr.contains("FLICK-P002") && stderr.contains("messy.fl"),
        "{stderr}"
    );
    assert_eq!(
        fs::read_to_string(&messy).expect("read"),
        MESSY,
        "--check writes nothing"
    );

    assert_eq!(flick(&["fmt", dir_arg]).status.code(), Some(0));
    assert_eq!(fs::read_to_string(&messy).expect("read"), TIDY);
    assert_eq!(flick(&["fmt", "--check", dir_arg]).status.code(), Some(0));
}

#[test]
fn fmt_needs_paths_or_stdin() {
    assert_eq!(flick(&["fmt"]).status.code(), Some(2));
    assert_eq!(
        flick(&["fmt", "--stdin-filepath", "a.fl", "x.fl"])
            .status
            .code(),
        Some(2)
    );
}
