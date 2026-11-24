use crate::diagnostics::{Diagnostic, codes};
use crate::types::lint_tests::{HEAD, assert_fix, check, fixed};

const USER: &str = "type alias Address = { city : String }\n\ntype alias User = { name : String; address : Address }\n\n";

/// The help of the diagnostic coded `code` at `at`, the first text of
/// `source`, and whether it carries edits.
fn help(source: &str, code: &str, at: &str) -> (String, bool) {
    let diagnostics = check(source);
    let start = u32::try_from(source.find(at).expect("the place")).expect("fits");
    let diagnostic: &Diagnostic = (diagnostics.iter())
        .find(|d| d.code.to_string() == code && d.span.is_some_and(|s| s.start == start))
        .unwrap_or_else(|| panic!("no {code} at `{at}`: {diagnostics:?}"));
    match &diagnostic.fixes[..] {
        [fix] => (fix.message.clone(), !fix.edits.is_empty()),
        fixes => panic!("expected one fix, got {fixes:?}"),
    }
}

/// `.name` on the line after `user` reads its field once glued to it; the
/// fix takes the place of the one that would `ignore` the value.
#[test]
fn an_accessor_on_the_next_line_is_glued_to_the_value() {
    let body = format!("{USER}name : User -> String\nlet name user =\n  user\n  .name\n");
    assert_eq!(
        help(&format!("{HEAD}{body}"), "FLICK-T011", "user\n  .name"),
        (
            "`.name` here starts a separate accessor, a function; to read the field, write it right after the value: `user.name`".to_owned(),
            true
        )
    );
    assert_fix(
        &body,
        "FLICK-T011",
        &format!("{USER}name : User -> String\nlet name user =\n  user.name\n"),
    );
}

/// A run of accessors, each on its own line, is glued as one path, and
/// the accessors after the first are not offered `ignore`.
#[test]
fn a_run_of_accessors_is_glued_as_one_path() {
    let body =
        format!("{USER}city : User -> String\nlet city user =\n  user\n  .address\n  .city\n");
    let (message, _) = help(&format!("{HEAD}{body}"), "FLICK-T011", "user\n  .address");
    assert!(message.ends_with("`user.address.city`"), "{message}");
    assert_fix(
        &body,
        "FLICK-T011",
        &format!("{USER}city : User -> String\nlet city user =\n  user.address.city\n"),
    );
}

/// After a `let`, the accessor is meant as a field of its value.
#[test]
fn an_accessor_after_a_let_is_glued_to_its_value() {
    let body =
        format!("{USER}name : User -> String\nlet name user =\n  let n = user\n  .name\n  n\n");
    assert_fix(
        &body,
        "FLICK-T011",
        &format!("{USER}name : User -> String\nlet name user =\n  let n = user.name\n  n\n"),
    );
    // as the last line, the accessor is the value of the block
    let last =
        format!("{HEAD}{USER}name : User -> String\nlet name user =\n  let n = user\n  .name\n");
    assert_eq!(
        help(&last, "FLICK-T003", ".name\n"),
        (
            "`.name` here starts a separate accessor, a function; to read the field, write it right after the value: `user.name`".to_owned(),
            true
        )
    );
    assert_eq!(
        fixed(&last, "FLICK-T003").as_deref(),
        Some(&*format!(
            "{HEAD}{USER}name : User -> String\nlet name user =\n  let n = user.name\n"
        ))
    );
}

/// An indented accessor continues the line: the value is applied to it.
#[test]
fn an_accessor_taken_for_an_argument_is_glued_to_the_value() {
    assert_fix(
        &format!("{USER}name : User -> String\nlet name user =\n  user\n    .name\n"),
        "FLICK-T003",
        &format!("{USER}name : User -> String\nlet name user =\n  user.name\n"),
    );
    assert_fix(
        &format!("{USER}city : User -> String\nlet city user = user .address .city\n"),
        "FLICK-T003",
        &format!("{USER}city : User -> String\nlet city user = user.address.city\n"),
    );
}

/// Glued after an application, the accessor would read a field of its
/// last argument: the help shows no example and there is no fix.
#[test]
fn an_accessor_after_an_application_gets_only_the_help() {
    let source = format!(
        "{HEAD}{USER}first : List User -> String\nlet first users = List.head users .name\n"
    );
    assert_eq!(
        help(&source, "FLICK-T003", "List.head users .name"),
        (
            "`.name` here starts a separate accessor, a function; to read the field, write it right after the value, with no space or line break".to_owned(),
            false
        )
    );
    assert_eq!(fixed(&source, "FLICK-T003"), None);
}

/// A comment between the value and the accessor is not deleted.
#[test]
fn a_comment_before_the_accessor_keeps_it_apart() {
    let source = format!(
        "{HEAD}{USER}name : User -> String\nlet name user =\n  user\n  // the name\n  .name\n"
    );
    let diagnostics = check(&source);
    let t011 = (diagnostics.iter())
        .find(|d| d.code == codes::T011)
        .unwrap_or_else(|| panic!("{diagnostics:?}"));
    let messages: Vec<(&str, bool)> = (t011.fixes.iter())
        .map(|f| (f.message.as_str(), f.edits.is_empty()))
        .collect();
    assert_eq!(
        messages,
        [
            (
                "`.name` here starts a separate accessor, a function; to read the field, write it right after the value: `user.name`",
                true
            ),
            ("discard it explicitly with `ignore`", false),
        ]
    );
}

/// An accessor that is the value a block is expected to give is no
/// mistake: nothing is reported.
#[test]
fn an_accessor_expected_as_a_function_gets_no_help() {
    let source = format!(
        "{HEAD}{USER}getter : Unit -> (User -> String)\nlet getter () =\n  let n = 1\n  .name\n"
    );
    let errors: Vec<_> = check(&source)
        .into_iter()
        .filter(|d| d.code != codes::R013)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}
