use super::common::{APP, flick_in, project};
use std::os::unix::fs::symlink;

#[test]
#[ignore = "needs Node and pnpm install"]
fn run_and_test_accept_a_trusted_project_alias() {
    let root = project(
        "cli_trusted_app_alias",
        &[
            ("real/flick.toml", APP),
            (
                "real/src/App/Main.fl",
                "module App.Main exposing (main)\nmain : Unit -> Unit\nlet main () = ()\n",
            ),
            (
                "real/test/Suite.fl",
                "module Suite exposing (tests)\ntests : List Test.Test\nlet tests = [Test.test \"alias\" (fun () -> Test.Pass)]\n",
            ),
        ],
    );
    symlink(root.join("real"), root.join("alias")).expect("project alias");
    let selected = root.join("alias").to_string_lossy().into_owned();
    let run = flick_in(&root, &["--project", &selected, "run"]);
    assert!(run.status.success(), "{run:?}");
    assert!(run.stdout.is_empty());
    for _ in 0..2 {
        let test = flick_in(
            &root,
            &["--project", &selected, "test", "--reporter", "json"],
        );
        assert!(test.status.success(), "{test:?}");
        let report: serde_json::Value = serde_json::from_slice(&test.stdout).expect("real report");
        assert_eq!(report["passed"], 1);
        assert_eq!(report["failed"], 0);
        assert!(root.join("real/.flick/test/test.js").is_file());
    }
}
