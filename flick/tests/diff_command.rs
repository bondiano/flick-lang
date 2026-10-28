mod common;

use std::fs;
use std::path::Path;
use std::process::Command;

use common::flick_in;

const CONFIG: &str = "schema-version = 1\n\n[project]\nkind = \"lib\"\n";

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(["-c", "user.name=test", "-c", "user.email=test@example.com"])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

#[test]
fn diff_names_the_changes_since_a_tag_and_the_bump() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("diff_api");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/Lib")).expect("dirs");
    fs::write(dir.join("flick.toml"), CONFIG).expect("write");
    fs::write(
        dir.join("package.json"),
        "{ \"name\": \"lib\", \"version\": \"1.2.3\" }\n",
    )
    .expect("write");
    let v1 = "module Lib.Api exposing (greet, count)\n\ngreet : String -> String\nlet greet name = \"Hi \" ++ name\n\ncount : Float\nlet count = 1\n";
    fs::write(dir.join("src/Lib/Api.fl"), v1).expect("write");
    git(&dir, &["init", "--quiet"]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "--quiet", "-m", "v1"]);
    git(&dir, &["tag", "v1.2.3"]);

    let v2 = "module Lib.Api exposing (greet, shout)\n\ngreet : String -> Int -> String\nlet greet name times = String.repeat times name\n\nshout : String -> String\nlet shout s = s ++ \"!\"\n";
    fs::write(dir.join("src/Lib/Api.fl"), v2).expect("write");
    let out = flick_in(&dir, &["diff"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        stdout,
        "API changes since v1.2.3:\n\nLib.Api\n  MAJOR  removed value count\n  MAJOR  changed value greet\n           was: String -> String\n           now: String -> Int -> String\n  MINOR  added value shout\n\nThis is a MAJOR change: 1.2.3 -> 2.0.0\n"
    );
    // the temporary worktree is gone
    let list = Command::new("git")
        .args(["worktree", "list"])
        .current_dir(&dir)
        .output()
        .expect("git");
    assert_eq!(String::from_utf8_lossy(&list.stdout).lines().count(), 1);

    let out = flick_in(&dir, &["diff", "no-such-ref"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stderr).contains("FLICK-P025"));
}
