use super::*;
use crate::source::SourceMap;

fn fmt(text: &str) -> String {
    let mut map = SourceMap::default();
    let file = map.add("t.fl", text);
    format(text, file).unwrap_or_else(|d| panic!("{d:?}"))
}

#[test]
fn normalizes_spacing_indentation_and_blank_lines() {
    let src = "\n\nload  : UserId->{Users,Log;NotFound}  String\nlet load id   =\n    let user = Users.get(id)   \n\n\n    let r = { name=user.name;age = 1 }\n    f ( a , b ) [1,2] -x\n";
    insta::assert_snapshot!(fmt(src), @r"
    load : UserId ->{Users, Log; NotFound} String
    let load id =
      let user = Users.get (id)

      let r = { name = user.name; age = 1 }
      f (a, b) [1, 2] - x
    ");
}

#[test]
fn keeps_comments_layout_and_mid_line_blocks() {
    let src = "// lead\nlet f x =  // trailing\n      match x with\n      | A ->\n          foo\n      // between arms\n      | B -> { a = 1\n               b = 2 }\n";
    insta::assert_snapshot!(fmt(src), @r"
    // lead
    let f x = // trailing
      match x with
      | A ->
        foo
      // between arms
      | B -> { a = 1
               b = 2 }
    ");
}

#[test]
fn is_idempotent_on_its_output() {
    let src = "type alias T a = {\n    x : a\n} deriving Eq\nlet g = fun () -> task { 1 }\n";
    let once = fmt(src);
    assert_eq!(fmt(&once), once);
}

#[test]
fn refuses_files_with_syntax_errors() {
    let mut map = SourceMap::default();
    let file = map.add("t.fl", "let = 1\n");
    let errors = format("let = 1\n", file).expect_err("syntax error");
    assert_eq!(errors[0].code, codes::S007);
}

#[test]
fn wraps_long_lines_at_their_outermost_structure() {
    let long_signature = "load_everything : UserId -> SessionToken -> RequestContext ->{Users, Sessions, Log; NotFound, Expired} Profile\n";
    let long_let = "let summary = describe_user_profile first_argument second_argument third_argument fourth_argument fifth\n";
    let long_pipe = "let names = users |> List.filter is_active_and_verified |> List.map display_name_of_user |> List.sort_by_length\n";
    let long_record = "let config = { host = \"localhost\"; port = 8080; timeout_seconds = 30; retries = 5; verbose_logging = true }\n";
    let long_if = "let pick = if is_enabled_for_this_particular_user user then load_the_premium_dashboard user else fallback_view\n";
    let out = fmt(&[long_signature, long_let, long_pipe, long_record, long_if].concat());
    insta::assert_snapshot!(out, @r#"
    load_everything : UserId ->
      SessionToken ->
      RequestContext ->{Users, Sessions, Log; NotFound, Expired}
      Profile
    let summary =
      describe_user_profile first_argument second_argument third_argument fourth_argument fifth
    let names =
      users
      |> List.filter is_active_and_verified
      |> List.map display_name_of_user
      |> List.sort_by_length
    let config =
      { host = "localhost"; port = 8080; timeout_seconds = 30; retries = 5; verbose_logging = true }
    let pick =
      if is_enabled_for_this_particular_user user then
        load_the_premium_dashboard user
      else
        fallback_view
    "#);
    assert_eq!(fmt(&out), out, "wrapping is idempotent");
    assert!(out.lines().all(|l| l.chars().count() <= wrap::WIDTH));
}

#[test]
fn wraps_nested_pieces_further_and_keeps_unbreakable_lines() {
    let src = "let report = render_everything (List.map (fun user -> format_user_line user.name user.email user.created_at) all_users_from_database) header\n";
    let out = fmt(src);
    insta::assert_snapshot!(out, @r"
    let report =
      render_everything
        (List.map
          (fun user -> format_user_line user.name user.email user.created_at)
          all_users_from_database)
        header
    ");
    assert_eq!(fmt(&out), out);
    let literal = format!("\"{}\"", "x".repeat(120));
    let once = fmt(&format!("let s = {literal}\n"));
    assert_eq!(
        once,
        format!("let s =\n  {literal}\n"),
        "a single token is never split"
    );
    assert_eq!(fmt(&once), once);
}

#[test]
fn drops_empty_arrow_rows() {
    let src = "f : A ->{none; E} B\ng : A ->{none /* why */; E} B\nh : A ->{Log; none} B\ni : Eff none none A\nj : A ->{none; none} B\nk : A ->{; none} B\n";
    insta::assert_snapshot!(fmt(src), @r"
    f : A ->{; E} B
    g : A ->{ /* why */ ; E} B
    h : A ->{Log; none} B
    i : Eff none none A
    j : A ->{} B
    k : A ->{} B
    ");
}
