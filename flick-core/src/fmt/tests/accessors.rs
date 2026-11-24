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
