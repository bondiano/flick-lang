use super::fmt;

/// Expression holes stay in argument position, never expand to `fun`, and stay idempotent.
#[test]
fn keeps_expression_holes_unchanged_and_idempotent() {
    let src = "let a = f a _ c\nlet b = x |> f a _ c\nlet c = f a _ c <| x\nlet d = f (g _) _\nlet e = very_long_function_name_for_wrapping first_long_argument _ second_long_argument third_long_argument\n";
    let formatted = fmt(src);
    insta::assert_snapshot!(formatted, @r#"
    let a = f a _ c
    let b = x |> f a _ c
    let c = f a _ c <| x
    let d = f (g _) _
    let e =
      very_long_function_name_for_wrapping
        first_long_argument
        _
        second_long_argument
        third_long_argument
    "#);
    assert_eq!(fmt(&formatted), formatted);
}
