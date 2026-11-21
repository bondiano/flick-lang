//! `test.each` under `flick test`: a case for each row, named by its index or
//! by the parts of its row, beside the other cases of its suite and with
//! fixtures of its own, alike on Flick's runtime, through the Effect adapter
//! and on Go.

use crate::common;
use crate::test_hooks::{ENGINES, library, report};

use common::flick_in;

const EACH: &str = r#"module App.Test

suite "math" {
  fixture base = 10

  test "first" {}

  test.each [(1, 2), (2, 3)] "adds {0} to get {1}" (a, b) {
    Test.check (Test.equal b (a + await base - 9))
  }

  test.each ["x", "yy"] "named" (s) { Test.check (Test.equal 1 (String.length s)) }
}

test.each [true] "alone {0}" (flag) { Test.check (Test.is_true "true" flag) }
"#;

#[test]
#[ignore = "needs Node, pnpm install and Go"]
fn a_test_each_is_a_case_for_each_row_on_every_engine() {
    for (name, effect, emit) in ENGINES {
        let dir = library(&format!("test_each_{name}"), effect, EACH);
        let out = flick_in(&dir, &["test", "--target", emit, "--reporter", "json"]);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(1), "{name}: {out:?}");
        let (_, results) = report(name, &format!("\n{stdout}"));
        assert_eq!(
            results,
            [
                "App.Test > math > first | passed |  |  | ",
                "App.Test > math > adds 1 to get 2 | passed |  |  | ",
                "App.Test > math > adds 2 to get 3 | passed |  |  | ",
                "App.Test > math > named [0] | passed |  |  | ",
                "App.Test > math > named [1] | failed | assertion | expected 1, got 2 | ",
                "App.Test > alone true | passed |  |  | ",
            ],
            "{name}"
        );
    }
}
