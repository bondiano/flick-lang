use super::super::tests::release;

/// `Default` of `derive_default.fl`, with `GDefault` over `Generic` and the
/// instance of `Float`, and `Point` after it.
const DEFAULT: &str = "module App exposing (..)\n\nimport Generic exposing (Generic(..), Sum(..), Con(..), Field(..))\n\n/// A value of each type: the first constructor, each field its default.\nclass Default a =\n  default : a\n  default : (Generic a rep, GDefault rep) => a\n  let default = from_rep gdefault\n\nclass GDefault rep =\n  gdefault : rep\n\ninstance GDefault Unit =\n  let gdefault = ()\n\ninstance (Default a, GDefault rest) => GDefault (Field l a, rest) =\n  let gdefault = (Field default, gdefault)\n\ninstance GDefault fields => GDefault (Con n fields) =\n  let gdefault = Con gdefault\n\ninstance GDefault a => GDefault (Sum a b) =\n  let gdefault = Inl gdefault\n\ninstance Default Float =\n  let default = 0\n\ntype Point = Point Float Float\n";

/// The line of `js` that defines `name`.
fn line<'a>(js: &'a str, name: &str) -> &'a str {
    let start = format!("export const {name} = ");
    js.lines()
        .find(|l| l.starts_with(&start))
        .unwrap_or_else(|| panic!("no {name} in\n{js}"))
}

#[test]
fn a_default_through_generic_is_the_method_written_by_hand() {
    let derived = release(&format!("{DEFAULT}  deriving Default\n"));
    let hand = release(&format!(
        "{DEFAULT}\ninstance Default Point =\n  let default = Point 0 0\n"
    ));
    let method = "$i_Default_Point$default";
    assert_eq!(line(&derived, method), line(&hand, method));
    assert!(!derived.contains("get default()"), "{derived}");
}

#[test]
fn a_match_of_a_known_constructor_is_the_arm_it_takes() {
    let js = release(
        "module App exposing (pick, pair)\n\n\
         pick : Float\nlet pick =\n  match Some 2.0 with\n  | None -> 0\n  | Some x -> x\n\n\
         pair : Float\nlet pair =\n  match (1.0, Some 3.0) with\n  | (a, Some b) -> a + b\n  | _ -> 0\n",
    );
    assert_eq!(line(&js, "pick"), "export const pick = 2;");
    assert_eq!(line(&js, "pair"), "export const pair = 4;");
}

#[test]
fn a_known_default_is_taken_apart_where_it_is_and_named_elsewhere() {
    let js = release(&format!(
        "{DEFAULT}  deriving Default\n\n\
         type Shape = Rect Point Point | Circle Float\n  deriving Default\n\n\
         sum : Float\nlet sum =\n  let Point x y = (default : Point)\n  x + y\n\n\
         whole : Point\nlet whole = default\n\n\
         kind : String\nlet kind =\n  match (default : Shape) with\n  | Circle _ -> \"circle\"\n  | Rect _ _ -> \"rect\"\n"
    ));
    assert_eq!(line(&js, "sum"), "export const sum = 0;");
    // the shared value, not a copy of it
    assert_eq!(
        line(&js, "whole"),
        "export const whole = $i_Default_Point$default;"
    );
    assert_eq!(line(&js, "kind"), "export const kind = \"rect\";");
    // a default that uses another default is a value too
    assert!(
        line(&js, "$i_Default_Shape$default").contains("Rect#"),
        "{js}"
    );
}

#[test]
fn settling_data_does_not_expand_a_recursive_method_function() {
    use std::cell::Cell;

    use super::{Lookup, methods};
    use crate::ir::tests::check_text;
    use crate::ir::{Expr, ExprKind, ItemKind};
    use flick_ir::Dict;

    let (mut module, _) = check_text(
        "Recursive",
        "module Recursive exposing (..)\ntype Tree = Leaf | Node Tree Tree\n  deriving Eq\n",
    )
    .expect("a recursive derived instance");
    let (name, method, at) = module
        .items
        .iter()
        .find_map(|item| match &item.kind {
            ItemKind::Instance { info, methods } => methods
                .iter()
                .find(|(name, _)| name == "eq")
                .map(|(_, body)| (info.name.clone(), body.clone(), item.span)),
            _ => None,
        })
        .expect("Eq Tree method");
    let dict = Dict::Instance {
        module: module.name.clone(),
        name: name.clone(),
        args: Vec::new(),
    };
    let mut value = Expr::new(
        ExprKind::Method {
            dict,
            name: "eq".to_owned(),
        },
        at,
    );
    let original = value.clone();
    // Model a known method imported from a different module. Looking up its
    // function is enough to know it cannot fold to a data value; following
    // recursive calls inside it would branch once per recursive field.
    module.name = "Client".to_owned();
    module.items.clear();
    let lookups = Cell::new(0);
    let generic = |owner: &str, member: &str| {
        assert_eq!(owner, "Recursive");
        assert_eq!(member, format!("{name}#eq"));
        lookups.set(lookups.get() + 1);
        assert_eq!(
            lookups.get(),
            1,
            "data settling must not follow a method function's body"
        );
        Some((Vec::new(), method.clone()))
    };
    let lookup = Lookup::new(&module, &generic);
    assert!(!methods(&mut value, &lookup, &mut 0, 0));
    assert_eq!(value, original);
    assert_eq!(lookups.get(), 1);
}
