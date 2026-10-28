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
      | B -> { a = 1;
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

#[test]
fn hugging_parentheses_do_not_push_a_piece_past_the_width() {
    // the `)` after the last operator piece used to make a 101-column line
    let src = "let main () =\n  Platform.stdout_line\n    (\"max: \" ++ show (count (Schedule.max (NonEmpty.consxxxxxxxx (Schedule.recurs 1) [Schedule.recurs 3]))))\n";
    let out = fmt(src);
    assert_eq!(fmt(&out), out, "wrapping is idempotent");
    assert!(
        out.lines().all(|l| l.chars().count() <= wrap::WIDTH),
        "{out}"
    );
}

#[test]
fn keeps_a_line_between_its_parent_and_a_deeper_line_apart() {
    let src = "let main () =\n  let x = try\n      get ()\n    with\n    | Boom n -> \"b\"\n  x\n";
    insta::assert_snapshot!(fmt(src), @r#"
    let main () =
      let x = try
          get ()
        with
        | Boom n -> "b"
      x
    "#);
}

#[test]
fn items_ending_a_line_end_with_their_separator() {
    let src = "type alias T = {\n  a : Float\n  b : String\n  | r\n}\nlet xs = [1,\n  2 // two\n]\nlet t = (1, 2)\nlet one = (\n  1\n)\nlet r = { a = 1; b =\n            2\n  }\nlet long = [ \"aaaaaaaaaaaaaaaaaaaaaaaa\", \"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\", \"cccccccccccccccccccccccc\" ]\n";
    let out = fmt(src);
    insta::assert_snapshot!(out, @r#"
    type alias T = {
      a : Float;
      b : String;
      | r
    }
    let xs = [1,
      2, // two
    ]
    let t = (1, 2)
    let one = (
      1
    )
    let r = { a = 1; b =
                2;
      }
    let long =
      [
        "aaaaaaaaaaaaaaaaaaaaaaaa",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "cccccccccccccccccccccccc",
      ]
    "#);
    assert_eq!(fmt(&out), out, "trailing separators are idempotent");
}

#[test]
fn keeps_a_rest_pattern_tight() {
    assert_eq!(
        fmt("let [ x ,  ...  rest ] = xs\n"),
        "let [x, ...rest] = xs\n"
    );
}
