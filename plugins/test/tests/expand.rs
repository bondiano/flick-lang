//! The code `suite` and `test` blocks stand for, and what is wrong with the
//! ones that stand for none.

use flick_core::plugin::testing::{blocks, expand};
use flick_plugin::Plugin;
use flick_plugin_test::Test;

const HEADER: &str = "module App.UsersTest exposing (..)\n\n";

/// What each block of a module of `blocks` stands for.
fn all(blocks: &str) -> Vec<Result<String, Vec<String>>> {
    expand(&Test, &format!("{HEADER}{blocks}\n"))
}

fn code(blocks: &str) -> String {
    (all(blocks).into_iter())
        .map(|found| found.unwrap_or_else(|errors| panic!("{errors:?}")))
        .collect::<Vec<_>>()
        .join("\n---\n")
}

fn errors(blocks: &str) -> String {
    (all(blocks).into_iter())
        .filter_map(Result::err)
        .flatten()
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_suite_is_a_group_of_its_cases_and_a_test_a_case_of_its_body() {
    insta::assert_snapshot!(code(
        "suite \"Users\" {\n  test \"creates one\" { Test.expect (Test.equal \"Ada\" \"Ada\") }\n\n  suite.skip \"names\" {\n    test.only \"trims\" { Test.expect (Test.equal 1 1) }\n    test \"empty\" {}\n  }\n}"
    ), @r#"
    let tests : (List Test.Test) = [
      (Test.group "Users" [
        (Test.effect_test "creates one" (task { (Test.expect (Test.equal "Ada" "Ada")) })),
        (Test.skip
          (Test.group "names" [
            (Test.only (Test.effect_test "trims" (task { (Test.expect (Test.equal 1 1)) }))),
            (Test.effect_test "empty" (task { () })),
          ])),
      ]),
    ]
    "#);
    // a body on lines of its own is the body of the task
    let lines = code(
        "test \"lines\" {\n  let user = { name = \"Ada\" }\n  Test.expect (Test.equal \"Ada\" user.name)\n}",
    );
    assert!(lines.contains("(task { (let user = { name = \"Ada\" }\n  Test.expect (Test.equal \"Ada\" user.name)) })"), "{lines}");
}

#[test]
fn the_module_s_tests_are_its_blocks_in_order() {
    // `tests$i` is a name only generated code writes, which the formatter does
    // not read: the code is shown as rendered
    insta::assert_snapshot!(code(
        "test \"first\" { Test.expect (Test.equal 1 1) }\n\nsuite.only \"second\" {}\n\ntest.skip \"third\" {}"
    ), @r#"
    let tests$0 = (Test.effect_test "first" (task { (Test.expect (Test.equal 1 1)) }))
    let tests : (List Test.Test) = [tests$0, tests$1, tests$2]
    ---
    let tests$1 = (Test.only (Test.group "second" []))
    ---
    let tests$2 = (Test.skip (Test.effect_test "third" (task { () })))
    "#);
    // a wrong block is no case of the module's `tests`
    let found = all("test \"first\" {}\n\ntest {}\n\ntest \"third\" {}");
    assert!(found[1].is_err(), "{found:?}");
    assert_eq!(
        found[0].as_deref(),
        Ok(
            "let tests$0 = (Test.effect_test \"first\" (task { () }))\nlet tests : (List Test.Test) = [tests$0, tests$1]"
        )
    );
    assert_eq!(
        found[2].as_deref(),
        Ok("let tests$1 = (Test.effect_test \"third\" (task { () }))")
    );
}

#[test]
fn a_block_names_its_case_and_holds_cases_or_a_body() {
    insta::assert_snapshot!(errors("suite { test \"a\" {} }"), @r#"T001 on the block: a `suite` block starts a top-level declaration, names its case and holds its tests in braces: `suite "name" { … }`, maybe `suite.skip` or `suite.only`, and `timeout millis` after the name"#);
    insta::assert_snapshot!(errors("test.later \"a\" {}"), @r#"T001 on the block: a `test` block starts a top-level declaration, names its case and holds its body in braces: `test "name" { … }`, maybe `test.skip` or `test.only`, and `timeout millis` after the name"#);
    // a block of an expression, which an import of the head allows
    let expression = expand(
        &Test,
        "module M exposing (x)\n\nimport Test exposing (plugin test)\n\nlet x = test \"a\" {}\n",
    );
    assert!(
        matches!(&expression[..], [Err(e)] if e[0].contains("starts a top-level declaration")),
        "{expression:?}"
    );
    insta::assert_snapshot!(errors("test \"a\""), @r#"T001 on the block: a `test` block starts a top-level declaration, names its case and holds its body in braces: `test "name" { … }`, maybe `test.skip` or `test.only`, and `timeout millis` after the name"#);
    insta::assert_snapshot!(errors("suite \"a\" {\n  let x = 1\n}"), @r#"T001 `let`: a suite holds `test "name" { … }`, `suite "name" { … }`, maybe with `.skip` or `.only` and `timeout millis`, and the hooks `before_all`, `before_each`, `after_each` and `after_all { … }`"#);
    insta::assert_snapshot!(errors("suite \"a\" {\n  test {}\n}"), @r#"T001 `test`: a suite holds `test "name" { … }`, `suite "name" { … }`, maybe with `.skip` or `.only` and `timeout millis`, and the hooks `before_all`, `before_each`, `after_each` and `after_all { … }`"#);
}

#[test]
fn hooks_are_cases_of_their_suite_s_group_and_a_timeout_wraps_its_case() {
    insta::assert_snapshot!(code(
        "suite \"Db\" timeout 2000 {\n  before_all { Platform.stdout_line \"open\" }\n  before_each { Platform.stdout_line \"begin\" }\n  test \"reads\" timeout 50 {}\n  after_each {}\n  after_all { Platform.stdout_line \"close\" }\n  after_all {}\n}"
    ), @r#"
    let tests : (List Test.Test) = [
      (Test.timeout
        2000
        (Test.group "Db" [
          (Test.before_all (task { (Platform.stdout_line "open") })),
          (Test.before_each (task { (Platform.stdout_line "begin") })),
          (Test.timeout 50 (Test.effect_test "reads" (task { () }))),
          (Test.after_each (task { () })),
          (Test.after_all (task { (Platform.stdout_line "close") })),
          (Test.after_all (task { () })),
        ])),
    ]
    "#);
    // a modifier wraps the timeout
    insta::assert_snapshot!(code("test.skip \"slow\" timeout (2 * 1000) {}"), @r#"
    let tests : (List Test.Test) = [
      (Test.skip (Test.timeout ((2 * 1000)) (Test.effect_test "slow" (task { () })))),
    ]
    "#);
}

#[test]
fn a_hook_has_no_name_and_a_timeout_its_milliseconds() {
    insta::assert_snapshot!(errors("suite \"a\" {\n  before_each \"x\" {}\n}"), @r#"T001 `before_each`: a suite holds `test "name" { … }`, `suite "name" { … }`, maybe with `.skip` or `.only` and `timeout millis`, and the hooks `before_all`, `before_each`, `after_each` and `after_all { … }`"#);
    insta::assert_snapshot!(errors("test \"a\" timeout {}"), @r#"T001 on the block: a `test` block starts a top-level declaration, names its case and holds its body in braces: `test "name" { … }`, maybe `test.skip` or `test.only`, and `timeout millis` after the name"#);
    insta::assert_snapshot!(errors("test \"a\" later 5 {}"), @r#"T001 on the block: a `test` block starts a top-level declaration, names its case and holds its body in braces: `test "name" { … }`, maybe `test.skip` or `test.only`, and `timeout millis` after the name"#);
    // hooks of one kind are no cases, which a full name stands for
    assert_eq!(
        errors("suite \"a\" {\n  before_each {}\n  before_each {}\n  test \"b\" {}\n}"),
        ""
    );
}

#[test]
fn a_full_name_stands_for_one_case() {
    insta::assert_snapshot!(errors(
        "suite \"a\" {\n  test \"x\" {}\n  suite \"y\" { test \"z\" {}; test \"z\" {} }\n  test \"x\" {}\n}"
    ), @r#"
    T002 `"x"`: another case at this level is named "x" too: a full name stands for one case
    T002 `"z"`: another case at this level is named "z" too: a full name stands for one case
    T002 `"z"`: another case at this level is named "z" too: a full name stands for one case
    T002 `"x"`: another case at this level is named "x" too: a full name stands for one case
    "#);
    // and so do the blocks of a module
    insta::assert_snapshot!(errors("test \"x\" {}\n\ntest \"y\" {}\n\nsuite \"x\" {}"), @r#"
    T002 `"x"`: another case at this level is named "x" too: a full name stands for one case
    T002 `"x"`: another case at this level is named "x" too: a full name stands for one case
    "#);
}

#[test]
fn the_words_of_cases_are_keywords_and_the_user_s_code_is_not() {
    let source = format!("{HEADER}suite \"a\" {{\n  test.skip \"b\" {{ x.only }}\n}}\n");
    let block = &blocks(&source)[0];
    let words: Vec<&str> = (Test.highlight(block, &[]).iter())
        .map(|h| block.text_of(h.span))
        .collect();
    assert_eq!(words, ["test", "skip"]);
    let skip = block.text.find("skip").expect("skip");
    let found = Test.hover(block, &[], u32::try_from(skip).expect("small"), &[]);
    assert!(found.is_some_and(|h| h.markdown.contains("never run")));
    // `only` of the user's `x.only` is no modifier
    let only = block.text.find("only").expect("only");
    assert!(
        Test.hover(block, &[], u32::try_from(only).expect("small"), &[])
            .is_none()
    );
}
