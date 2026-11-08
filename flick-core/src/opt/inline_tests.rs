//! Inlining across modules: the small functions of std and core, and the
//! queries of the plugin of std, whose text their methods build.

use super::tests::release;

#[test]
fn a_small_function_of_another_module_is_inlined() {
    let js = release(
        "module App exposing (pick, empty)\n\n\
         pick : Int\nlet pick = Int.min 3 5\n\n\
         empty : String -> Bool\nlet empty s = String.is_empty s\n",
    );
    // the bodies of `Int.min` and `String.is_empty`, which name only what
    // their modules export
    assert!(js.contains("export const pick = 3;"), "{js}");
    assert!(
        js.contains("export const empty = (s) => $m_String.length(s) === 0;"),
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
