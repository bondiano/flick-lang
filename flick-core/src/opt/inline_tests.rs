//! Inlining across modules: the small functions of std and core, and the
//! queries of the plugin of std, whose text their methods build.

use super::tests::release;

#[test]
fn a_small_function_of_another_module_is_inlined() {
    let js = release(
        "module App exposing (pick, empty)\n\n\
         pick : Int\nlet pick = Int.min 3 5\n\n\
         empty : List Int -> Bool\nlet empty xs = List.is_empty xs\n",
    );
    // the bodies of `Int.min` and `List.is_empty`, which name only what
    // their modules export
    assert!(js.contains("export const pick = 3;"), "{js}");
    assert!(
        js.contains("export const empty = (xs) => $m_List$Core.length(xs) === 0;"),
        "{js}"
    );
}

/// A query of the plugin of std: its text, cast as the types of its
/// captures say, is one literal, and its typing function is gone.
#[test]
fn a_query_of_std_folds_into_one_text() {
    let js = release(
        "module App exposing (named)\n\n\
         import Sql exposing (Pg)\n\n\
         sql table Users \"users\" {\n  id : bigint primary key generated always as BigInt\n  name : text not null as String\n}\n\n\
         named : Option String -> Sql.Template Pg {} { id : BigInt }\n\
         let named tag = sql {\n  from u in Users\n  where is_null $tag || u.name == $tag\n  limit 5\n  select { id = u.id }\n}\n",
    );
    let text = "SELECT \\\"u\\\".\\\"id\\\" AS \\\"id\\\" FROM \\\"users\\\" AS \\\"u\\\" WHERE ((CAST($1 AS text) IS NULL) OR (\\\"u\\\".\\\"name\\\" = CAST($2 AS text))) LIMIT $3";
    assert!(js.contains(&format!("text: \"{text}\"")), "{js}");
    assert!(!js.contains("Derive.query"), "{js}");
    assert!(!js.contains("cast_open"), "{js}");
}

const CHAIN: &str = "module App exposing (chain, kept, passed)\n\n\
chain : Option Int -> Int\n\
let chain value =\n  \
value\n  |> Option.filter (fun x -> x > 0)\n  |> Option.map (fun x -> x * 2)\n  |> Option.with_default 0\n\n\
kept : Option Int -> Option Int\nlet kept value = Option.map (fun x -> x + 1) value\n\n\
passed : (Int -> Int) -> Option Int -> Option Int\nlet passed f value = Option.map f value\n";

/// A chain of combinators over an option, each given a function written
/// where it is passed: no function is made, and no option between them.
#[test]
fn a_value_built_only_to_be_taken_apart_is_not_built() {
    let js = release(CHAIN);
    let chain = js.split("export const chain").nth(1).expect("chain");
    let chain = chain
        .split("export const kept")
        .next()
        .expect("what follows");
    // two tests and a product: nothing is called, made or allocated
    assert!(
        !chain.contains("$m_Option") && !chain.contains("some("),
        "{js}"
    );
    assert_eq!(chain.matches("=>").count(), 1, "{js}");
    assert!(
        chain.contains("return Math.imul(x$i1, 2);") && chain.contains("return 0;"),
        "{js}"
    );
}

/// A combinator given a function written there is the `match` it is; one
/// given a function it does not know stays a call, as inlining it would
/// spare nothing.
#[test]
fn a_combinator_is_inlined_only_where_that_spares_an_object() {
    let js = release(CHAIN);
    let kept = js.split("export const kept").nth(1).expect("kept");
    let (kept, passed) = kept.split_once("export const passed").expect("passed");
    assert!(!kept.contains("$m_Option.map"), "{js}");
    assert!(passed.contains("$m_Option.map"), "{js}");
}

/// An arm that names the value taken apart reads it by the name it has.
#[test]
fn an_arm_gives_no_second_name_to_what_is_taken_apart() {
    let js = release(
        "module App exposing (fizzbuzz)\n\n\
         fizzbuzz : Int -> String\nlet fizzbuzz n =\n  match n with\n  \
         | n when n % 15 == 0 -> \"FizzBuzz\"\n  | n when n % 3 == 0 -> \"Fizz\"\n  | n -> show n\n",
    );
    assert!(js.contains("if ($rt.remInt(n, 15) === 0) {"), "{js}");
    assert!(js.contains("if ($rt.remInt(n, 3) === 0) {"), "{js}");
    assert!(!js.contains("n$") && !js.contains("  const "), "{js}");
}
