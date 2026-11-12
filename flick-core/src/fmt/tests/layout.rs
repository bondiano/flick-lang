//! Layout of expressions, blocks, brackets and comments.

use super::fmt;

/// A short body joins its `=`; spacing and blank lines are normalized.
#[test]
fn joins_short_bodies_and_normalizes_spacing() {
    let src = "\n\nload  : UserId->{Users,Log;NotFound}  String\nlet  short x   =\n    x+1\nlet load id   =\n    let user = Users.get(id)   \n\n\n    let r = { name=user.name;age = 1 }\n    f ( a , b ) [1,2] -x\n";
    insta::assert_snapshot!(fmt(src), @r#"
    load : UserId ->{Users, Log; NotFound} String
    let short x = x + 1
    let load id =
      let user = Users.get (id)

      let r = { name = user.name; age = 1 }
      f (a, b) [1, 2] - x
    "#);
}

/// A body that does not fit goes to the next line; records, lists and blocks hug the `=`.
#[test]
fn breaks_a_long_body_after_its_equals() {
    let src = "let summary = describe_user_profile first_argument second_argument third_argument fourth_argument fifth\nlet config = { host = \"localhost\"; port = 8080; timeout_seconds = 30; retries = 5; verbose_logging = true }\nlet t = task { Users.get id }\nlet run id = provide (Layer.merge UsersLive LogLive) { load_name id; \"done\" }\nlet names = [ \"aaaaaaaaaaaaaaaaaaaaaaaa\", \"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\", \"cccccccccccccccccccccccc\" ]\n";
    insta::assert_snapshot!(fmt(src), @r#"
    let summary =
      describe_user_profile first_argument second_argument third_argument fourth_argument fifth
    let config = {
      host = "localhost";
      port = 8080;
      timeout_seconds = 30;
      retries = 5;
      verbose_logging = true;
    }
    let t = task { Users.get id }
    let run id = provide (Layer.merge UsersLive LogLive) {
      load_name id
      "done"
    }
    let names = [
      "aaaaaaaaaaaaaaaaaaaaaaaa",
      "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      "cccccccccccccccccccccccc",
    ]
    "#);
}

/// A function as the last argument keeps the call on its line; its `)` closes on its own line.
#[test]
fn hugs_a_last_function_argument() {
    let src = "let callback counter = Ref.modify counter (fun current ->\n    let next = current + 1\n    (next, next))\nlet view = Ui.view (fun () -> match Atom.get user with\n  | Atom.Loading -> \"loading\"\n  | Atom.Loaded name -> name)\nlet short = List.map (fun x -> x + 1) xs\n";
    insta::assert_snapshot!(fmt(src), @r#"
    let callback counter =
      Ref.modify counter (fun current ->
        let next = current + 1
        (next, next)
      )
    let view =
      Ui.view (fun () ->
        match Atom.get user with
        | Atom.Loading -> "loading"
        | Atom.Loaded name -> name
      )
    let short = List.map (fun x -> x + 1) xs
    "#);
}

/// Arguments go one per line; a parenthesized call stays hugged by its parentheses.
#[test]
fn breaks_arguments_one_per_line() {
    let src = "let report = render_everything (List.map (fun user -> format_user_line user.name user.email user.created_at) all_users_from_database) header\n";
    insta::assert_snapshot!(fmt(src), @r#"
    let report =
      render_everything
        (List.map
          (fun user -> format_user_line user.name user.email user.created_at)
          all_users_from_database)
        header
    "#);
}

/// Operators break before the weakest precedence; pipelines one step per line, kept broken when written so.
#[test]
fn breaks_operators_and_pipelines() {
    let src = "let names = users |> List.filter is_active_and_verified |> List.map display_name_of_user |> List.sort_by_length\nlet kept =\n  xs\n    |> List.map f\nlet nested = f (xs |> List.filter is_active_and_verified_and_more |> List.map display_name_of_user_long_name)\nlet logic = first_condition_is_true && second_condition_holds || third_condition_is_met && the_fourth_one_too\n";
    insta::assert_snapshot!(fmt(src), @r#"
    let names =
      users
      |> List.filter is_active_and_verified
      |> List.map display_name_of_user
      |> List.sort_by_length
    let kept =
      xs
      |> List.map f
    let nested =
      f (xs |> List.filter is_active_and_verified_and_more |> List.map display_name_of_user_long_name)
    let logic =
      first_condition_is_true && second_condition_holds || third_condition_is_met && the_fourth_one_too
    "#);
}

/// `<|` breaks as `|>` does, one step per line; a composition as other operators.
#[test]
fn breaks_left_pipes_and_compositions() {
    let src = "let shown = Platform.stdout_line <| show <| List.length <| List.filter is_active_and_verified_user <| users_of_the_system\nlet kept =\n  f\n    <| g x\nlet composed = List.map normalize_the_name >> List.filter is_active_and_verified >> List.sort_by_length >> List.take 10\nlet tight = (f>>g) 1\nlet back = f<<g<<h\nlet trailing =\n  f <|\n    g x\nlet piped =\n  xs |>\n    List.map f\n";
    insta::assert_snapshot!(fmt(src), @r"
    let shown =
      Platform.stdout_line
      <| show
      <| List.length
      <| List.filter is_active_and_verified_user
      <| users_of_the_system
    let kept =
      f
      <| g x
    let composed =
      List.map normalize_the_name
        >> List.filter is_active_and_verified
        >> List.sort_by_length
        >> List.take 10
    let tight = (f >> g) 1
    let back = f << g << h
    let trailing =
      f
      <| g x
    let piped =
      xs
      |> List.map f
    ");
}

/// `if` stays on one line or breaks every branch; `match` and `try` put arms one per line.
#[test]
fn lays_out_if_match_and_try() {
    let src = "let classify n = if n < 0 then \"negative\" else if n == 0 then \"zero\" else \"positive\"\nlet long n = if n < 0 then describe_negative_number_with_a_long_name n else describe_the_positive_case_too n\nlet nested n =\n  if n > 0 then \"+\"\n  else\n    if n < 0 then \"-\" else \"0\"\nlet area shape =\n  match shape with | Square side | Circle side -> side * side\n  | Rect w h when w == h -> w * w\n  | Line\n  | Point -> 0\nlet safe id = try Some (Users.get id) with | NotFound _ -> None\nlet both () =\n  try\n    let first = load \"a\"\n    first.title\n  with\n  | NotFound id -> \"not found\"\n";
    insta::assert_snapshot!(fmt(src), @r#"
    let classify n = if n < 0 then "negative" else if n == 0 then "zero" else "positive"
    let long n =
      if n < 0 then describe_negative_number_with_a_long_name n else describe_the_positive_case_too n
    let nested n =
      if n > 0 then
        "+"
      else
        if n < 0 then "-" else "0"
    let area shape =
      match shape with
      | Square side | Circle side -> side * side
      | Rect w h when w == h -> w * w
      | Line | Point -> 0
    let safe id =
      try Some (Users.get id) with
      | NotFound _ -> None
    let both () =
      try
        let first = load "a"
        first.title
      with
      | NotFound id -> "not found"
    "#);
}

/// No `,` may follow a block on its own lines, so the last branch of an item stays on its line.
#[test]
fn keeps_a_last_branch_on_its_line_before_a_comma() {
    let src = "let items = [if a then first_value_of_the_list_here else other_value, if b then second_value_long_name else third_one_here]\n";
    insta::assert_snapshot!(fmt(src), @r#"
    let items = [
      if a then first_value_of_the_list_here else other_value,
      if b then second_value_long_name else third_one_here,
    ]
    "#);
}

/// A trailing separator or a blank line keeps a bracket broken; constants written several per line fill lines.
#[test]
fn follows_trailing_separators_and_blank_lines() {
    let src = "let xs = [1,\n  2]\nlet ys = [1, 2,]\nlet filled = [\n  \"one\", \"two\", \"three\", \"four\", \"five\", \"six\", \"seven\", \"eight\", \"nine\", \"ten\", \"eleven\",\n  \"twelve\", \"thirteen\",\n]\nlet spaced = [\n  1,\n\n  2,\n]\nlet r = { a = 1; b = 2; }\nlet u = { base with a = 1; b = 2 }\nlet long = { base_record_with_a_long_name with first_field = 1; second_field = \"two\"; third = 3 }\nlet one = (1,)\nlet [ x ,  ...  rest ] = xs\n";
    insta::assert_snapshot!(fmt(src), @r#"
    let xs = [1, 2]
    let ys = [1, 2]
    let filled = [
      "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve",
      "thirteen",
    ]
    let spaced = [
      1,

      2,
    ]
    let r = {
      a = 1;
      b = 2;
    }
    let u = { base with a = 1; b = 2 }
    let long = { base_record_with_a_long_name with first_field = 1; second_field = "two"; third = 3 }
    let one = (1,)
    let [x, ...rest] = xs
    "#);
}

/// Comments keep their lines: own-line comments before what follows, trailing ones at the end of theirs.
#[test]
fn keeps_comments_in_place() {
    let src = "// lead\nlet f x =  // trailing\n      match x with\n      | A ->\n          foo\n      // between arms\n      | B -> 2\n\nlet xs = [1, // one\n  2\n  // before the end\n]\nlet g () =\n  task {\n    run () // runs\n    // done\n  }\n/// Documented.\nlet h = 1 // end\n";
    insta::assert_snapshot!(fmt(src), @r#"
    // lead
    let f x = // trailing
      match x with
      | A -> foo
      // between arms
      | B -> 2

    let xs = [
      1, // one
      2,
      // before the end
    ]
    let g () = task {
      run () // runs
      // done
    }
    /// Documented.
    let h = 1 // end
    "#);
}

/// A block comment with code after it on its line stays there, spaced as a
/// word; before a closer or a separator it sticks to the token on its left.
#[test]
fn keeps_block_comments_inline() {
    let src = "let a = f /* b */ x\nlet c = (x /* c */)\nlet d = (/* d */ x)\nlet e = x /* g */ + y\nlet h = [1 /* h */, 2]\nlet i = [1, 2 /* i */]\nlet j = (1 /* j */, 2)\nlet k = { a = 1 /* k */ }\nlet l = { a = 1 /* l */; b = 2 }\nlet n = if x /* n */ then y else z\nlet o = fun x /* o */ -> x\nlet u = (/* u */)\nlet t = x /* a */ // b\nlet after = [1, /* s */ 2]\nlet last = { a = 1; /* e */ }\nv : List /* v */ A\nw : { a : Int /* w */ }\n";
    insta::assert_snapshot!(fmt(src), @r#"
    let a = f /* b */ x
    let c = (x /* c */)
    let d = (/* d */ x)
    let e = x /* g */ + y
    let h = [1 /* h */, 2]
    let i = [1, 2 /* i */]
    let j = (1 /* j */, 2)
    let k = { a = 1 /* k */ }
    let l = { a = 1 /* l */; b = 2 }
    let n = if x /* n */ then y else z
    let o = fun x /* o */ -> x
    let u = (/* u */)
    let t = x /* a */ // b
    let after = [1, /* s */ 2]
    let last = {
      a = 1; /* e */
    }
    v : List /* v */ A
    w : { a : Int /* w */ }
    "#);
}

/// `sql`, `cli`, `do` and `for` blocks put one clause or statement per line.
#[test]
fn lays_out_blocks_in_braces() {
    let src = "let query =\n  sql params {\n    from user in Users\n    left_join account in Accounts\n      on account.user_id == user.id\n    where user.id == params.user_id\n    select { name = user.name; balance = account.balance }\n  }\nlet command = cli \"greet\" {\n  description \"Greets\"\n  /// Who to greet\n  arg name : String = \"world\"\n  handle fun { name } -> Platform.stdout_line (\"Hello, \" ++ name) }\nlet workflow = do { x <- ask; let y = x + 1; pure y }\nlet evens = [for x <- xs if x > 0 yield x * 2]\n";
    insta::assert_snapshot!(fmt(src), @r#"
    let query = sql params {
      from user in Users
      left_join account in Accounts on account.user_id == user.id
      where user.id == params.user_id
      select { name = user.name; balance = account.balance }
    }
    let command = cli "greet" {
      description "Greets"
      /// Who to greet
      arg name : String = "world"
      handle fun { name } -> Platform.stdout_line ("Hello, " ++ name)
    }
    let workflow = do {
      x <- ask
      let y = x + 1
      pure y
    }
    let evens = [for x <- xs if x > 0 yield x * 2]
    "#);
}

/// A block its plugin does not lay out keeps its lines, which move with
/// its head; the declaration around it is laid out.
#[test]
fn moves_a_block_without_a_layout_with_its_head() {
    let src = "import Pairs exposing (plugin pairs)\n\nlet x =\n      pairs {\n        a = 1\n          + 2\n      }\n";
    insta::assert_snapshot!(fmt(src), @r#"
    import Pairs exposing (plugin pairs)

    let x = pairs {
      a = 1
        + 2
    }
    "#);
}

/// Braces after `then` and `else` hold statements: a line for each, the
/// `else` on the line of the closing brace, one statement on one line; a
/// record there keeps its parentheses.
#[test]
fn keeps_the_braces_of_an_if_branch() {
    let src = "let f n =\n  if n > 0 then {\n        a = 1\n        b = 2\n     }\n  else {   a = 3;  b = 4 }\n  if n > 9 then {  a = a + b } else    {\n      b = a\n  }\n  a\nlet g r = if r.x > 0 then ( { r with x = 0 } ) else r\n";
    insta::assert_snapshot!(fmt(src), @r#"
    let f n =
      if n > 0 then {
        a = 1
        b = 2
      } else {
        a = 3
        b = 4
      }
      if n > 9 then { a = a + b } else { b = a }
      a
    let g r = if r.x > 0 then ({ r with x = 0 }) else r
    "#);
}
