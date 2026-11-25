use super::fmt;

/// An accessor keeps its steps together and stands apart from what comes
/// before it; a long call breaks around it, never inside it.
#[test]
fn keeps_the_steps_of_an_accessor_together() {
    let src = "let a = List.map   .address.city users\nlet b = user|>.name\nlet c = [ .name , (.address.city) ]\nlet d = .a .b\nlet e = { get = .name }\nlet f = .name?\nlet g = \"\\{.name user}\"\nlet h = List.map .address.street_name_of_the_place_where_the_user_lives some_long_list_of_all_the_registered_users\n";
    insta::assert_snapshot!(fmt(src), @r#"
    let a = List.map .address.city users
    let b = user |> .name
    let c = [.name, (.address.city)]
    let d = .a .b
    let e = { get = .name }
    let f = .name?
    let g = "\{.name user}"
    let h =
      List.map
        .address.street_name_of_the_place_where_the_user_lives
        some_long_list_of_all_the_registered_users
    "#);
}

/// An optional step `.?label` stays whole, and the paths it is in keep
/// their boundaries: parentheses, a propagation `?` and a postfix `?`.
#[test]
fn keeps_optional_steps_whole() {
    let src = "let a = user .address.?city\nlet b = List.map .address.?city.name users\nlet c = (x.?a).?b\nlet d = x?.a\nlet e = .a?\nlet f = x.?a?\nlet g = maybe|>.?address.?city\nlet h = [0..5]\nlet i = 1.5\nlet j = (\"Новосибирск\",x.?city)\nlet k =\n  // the user\n  user.?address\n";
    let formatted = fmt(src);
    assert_eq!(fmt(&formatted), formatted, "a fixed point");
    insta::assert_snapshot!(formatted, @r#"
    let a = user .address.?city
    let b = List.map .address.?city.name users
    let c = (x.?a).?b
    let d = x?.a
    let e = .a?
    let f = x.?a?
    let g = maybe |> .?address.?city
    let h = [0..5]
    let i = 1.5
    let j = ("Новосибирск", x.?city)
    let k =
      // the user
      user.?address
    "#);
}
