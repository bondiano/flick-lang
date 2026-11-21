//! The fixtures of a suite under `flick test`: each made the first time a
//! case or a hook uses it, a shared one once for the suite, an unused one
//! never, each released after the hooks that close its scope, a value it
//! owns seeing how its case ended, and the cases of a suite that shares one
//! run one at a time under `--jobs`, alike on Flick's runtime, through the
//! Effect adapter and on Go.

use crate::common;
use crate::test_hooks::{ENGINES, library, report};

use common::flick_in;

const FIXTURES: &str = r#"module App.Test

/// A transaction: it commits when its case passes and rolls back otherwise.
type Tx = Tx

instance Resource.Close Tx =
  let close _ _ = Host.raw_stdout_line "rollback"
  let close_exit _ exit _ = Host.raw_stdout_line (if Exit.is_success exit then "commit" else "rollback")

note : String ->{...Host.Live} String
let note text =
  Platform.stdout_line text
  text

suite "store" {
  fixture shared database = note "open database" |> Fixture.cleanup (fun _ -> Platform.stdout_line "close database")
  fixture conn = note ("connect to " ++ await database) |> Fixture.cleanup (fun _ -> Platform.stdout_line "disconnect")
  fixture tx = Resource.own Tx
  fixture unused = note "never"

  before_each { Platform.stdout_line ("before_each sees " ++ await database) }
  after_each { Platform.stdout_line ("after_each sees " ++ await conn) }
  after_all { Platform.stdout_line ("after_all sees " ++ await database) }

  test "one" {
    let _tx = await tx
    Platform.stdout_line ("one uses " ++ await conn)
    Effect.sleep (Result.with_default Duration.zero (Duration.millis 50))
    Platform.stdout_line "one ends"
  }

  test "two" {
    Platform.stdout_line "two starts"
    let _tx = await tx
    Test.check (Test.equal 1 2)
  }
}
"#;

#[test]
#[ignore = "needs Node, pnpm install and Go"]
fn fixtures_are_made_on_first_use_and_released_after_the_hooks_on_every_engine() {
    let order = [
        "open database",
        "before_each sees open database",
        "connect to open database",
        "one uses connect to open database",
        "one ends",
        "after_each sees connect to open database",
        "disconnect",
        "commit",
        "before_each sees open database",
        "two starts",
        "connect to open database",
        "after_each sees connect to open database",
        "disconnect",
        "rollback",
        "after_all sees open database",
        "close database",
    ];
    for (name, effect, emit) in ENGINES {
        let dir = library(&format!("test_fixtures_{name}"), effect, FIXTURES);
        let args = [
            "test",
            "--target",
            emit,
            "--reporter",
            "json",
            "--jobs",
            "4",
        ];
        let out = flick_in(&dir, &args);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(1), "{name}: {out:?}");
        let (printed, results) = report(name, &stdout);
        assert_eq!(printed, order, "{name}");
        assert_eq!(
            results,
            [
                "App.Test > store > one | passed |  |  | ",
                "App.Test > store > two | failed | assertion | expected 1, got 2 | ",
            ],
            "{name}"
        );
    }
}

const BROKEN: &str = r#"module App.Test

suite "broken" {
  fixture conn = Test.check (Test.equal 1 2)
  test "awaits it" { await conn }
}
"#;

#[test]
#[ignore = "needs Node, pnpm install and Go"]
fn a_case_whose_fixture_fails_fails_its_setup_on_every_engine() {
    for (name, effect, emit) in ENGINES {
        let dir = library(&format!("test_fixtures_broken_{name}"), effect, BROKEN);
        let out = flick_in(&dir, &["test", "--target", emit, "--reporter", "json"]);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(1), "{name}: {out:?}");
        let (_, results) = report(name, &format!("\n{stdout}"));
        assert_eq!(
            results,
            ["App.Test > broken > awaits it | failed | setup | fixture conn: expected 1, got 2 | "],
            "{name}"
        );
    }
}
