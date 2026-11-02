//! `Test.suite`: a layer built once before the suite's first test and
//! released after its last, its services shared by its tests, on both
//! targets; a run that selects none of its tests never builds it.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::flick_in;

const SUITE: &str = r#"module Test.Sample exposing (tests)

service Box =
  value : Unit ->{} String

box : Layer none none {Box}
let box =
  Layer.from_build (fun memo ->
    Platform.stdout_line "acquire"
    Scope.add_finalizer (task { Platform.stdout_line "release" })
    Layer.build_with_memo_map memo (Layer.succeed Box { value = fun () -> "shared" })
  )

tests : List Test.Test
let tests = [
  Test.suite "boxed" box [
    Test.it "reads" (task { Test.expect (Test.equal "shared" (Box.value ())) }),
    Test.it "prints" (task { Platform.stdout_line ("printed " ++ Box.value ()) }),
  ],
  Test.test "outside" (fun () -> Test.equal 1 1),
]
"#;

fn project() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/tmp/test_suite");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("test/Test")).expect("dir");
    fs::create_dir_all(dir.join("src")).expect("dir");
    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
    )
    .expect("write");
    fs::write(dir.join("package.json"), "{ \"name\": \"@acme/suite\" }\n").expect("write");
    fs::write(dir.join("test/Test/Sample.fl"), SUITE).expect("write");
    dir
}

#[test]
#[ignore = "needs Node, pnpm install and Go"]
fn a_suite_builds_its_layer_once_around_its_tests() {
    let dir = project();
    for emit in ["js", "go"] {
        let out = flick_in(&dir, &["test", "--emit", emit]);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{emit}: {out:?}");
        assert_eq!(stdout.matches("acquire").count(), 1, "{emit}: {stdout}");
        assert_eq!(stdout.matches("release").count(), 1, "{emit}: {stdout}");
        let at = |text: &str| {
            stdout
                .find(text)
                .unwrap_or_else(|| panic!("{emit}: {text} in {stdout}"))
        };
        assert!(at("acquire") < at("printed shared"), "{emit}: {stdout}");
        assert!(at("printed shared") < at("release"), "{emit}: {stdout}");
        assert!(stdout.contains("3 passed"), "{emit}: {stdout}");
        let outside = flick_in(&dir, &["test", "--emit", emit, "--filter", "outside"]);
        let stdout = String::from_utf8_lossy(&outside.stdout);
        assert!(outside.status.success(), "{emit}: {outside:?}");
        assert!(!stdout.contains("acquire"), "{emit}: {stdout}");
    }
}
