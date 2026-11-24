//! A field rename that cannot show it accounts for every label that may
//! read the field is refused with the place that stops it, and edits
//! nothing: equal labels and equal shapes are never evidence.

use super::field_references_tests::{MAIN, applied, project};

/// Renaming `User`'s `name`, at the projection of `greet`, in `main`.
fn rename_user_name(main: &str, new: &str) -> Result<usize, String> {
    let f = project(main);
    let at = f.at("App.Main", "user.name", 0) + 5;
    f.ide()
        .rename(f.file("App.Main"), at, new)
        .map(|edits| edits.len())
}

fn refused(main: &str, why: &str) {
    assert_eq!(
        rename_user_name(main, "full_name"),
        Err(format!("cannot rename the field `name` of `User`: {why}")),
        "{main}"
    );
}

/// The line of `needle` in `main`, 1-based.
fn line(main: &str, needle: &str) -> usize {
    main[..main.find(needle).expect("occurs")]
        .matches('\n')
        .count()
        + 1
}

#[test]
fn a_row_polymorphic_helper_refuses() {
    let main =
        format!("{MAIN}\nanyone : User -> String\nlet anyone u =\n  let get = .name\n  get u\n");
    refused(
        &main,
        &format!(
            "`name` at src/App/Main.fl:{} may read it: its record is open: a row-polymorphic function reads the field of other records too",
            line(&main, "get = .name")
        ),
    );
    let main = format!("{MAIN}\nnamed : {{ name : String | r }} -> String\nlet named r = r.name\n");
    refused(
        &main,
        &format!(
            "`name` at src/App/Main.fl:{} may read it: its record is open: a row-polymorphic function reads the field of other records too",
            line(&main, "= r.name")
        ),
    );
}

#[test]
fn an_equal_record_of_another_declaration_refuses() {
    let main = format!("{MAIN}\ntype alias Person = {{ name : String; address : Address }}\n");
    refused(
        &main,
        &format!(
            "the record type at src/App/Main.fl:{} has its fields, so a value of either is a value of both",
            line(&main, "type alias Person")
        ),
    );
}

/// The same label in an unrelated record neither joins nor stops the
/// rename: `Product` keeps its `name`.
#[test]
fn the_same_label_of_an_unrelated_record_stays() {
    let main = format!(
        "{MAIN}\ntitles : List Product -> List String\nlet titles ps = List.map .name ps\n"
    );
    let f = project(&main);
    let at = f.at("App.Main", "user.name", 0) + 5;
    let edits = f
        .ide()
        .rename(f.file("App.Main"), at, "full_name")
        .expect("renamed");
    let (domain, main) = applied(&f, &main, &edits, "full_name");
    assert!(main.contains("List.map .name ps"), "{main}");
    assert!(
        domain.contains("{ name : String; price : Float }"),
        "{domain}"
    );
}

#[test]
fn an_alias_in_another_module_refuses() {
    let main = format!(
        "{MAIN}\ntype alias Member = User\n\nmember : Member -> String\nlet member m = m.name\n"
    );
    refused(
        &main,
        &format!(
            "`name` at src/App/Main.fl:{} may read it: its record is an alias of a record another module declares",
            line(&main, "m.name")
        ),
    );
}

#[test]
fn a_record_through_a_generic_call_refuses() {
    let main =
        format!("{MAIN}\nnames : List User -> List String\nlet names us = List.map .name us\n");
    refused(
        &main,
        &format!(
            "`name` at src/App/Main.fl:{} may read it: its record is written out, and records of other declarations with these fields are of its type",
            line(&main, ".name us")
        ),
    );
    // nor is the label there a field to rename
    let f = project(&main);
    let at = f.at("App.Main", ".name us", 0) + 1;
    assert_eq!(
        f.ide().renamable(f.file("App.Main"), at).map(|_| ()),
        Err("cannot rename `name`: its record is written out, and records of other declarations with these fields are of its type; it may not be one declaration's field".to_owned())
    );
}

#[test]
fn the_foreign_boundary_refuses() {
    let main = format!(
        "{MAIN}\nfetch : Unit ->{{}} User\nforeign fetch from global \"fetchUser\"\n  arity 0\n"
    );
    refused(
        &main,
        &format!(
            "`fetch` at src/App/Main.fl:{} passes it to foreign code, which reads fields by their names",
            line(&main, "foreign fetch")
        ),
    );
}

#[test]
fn a_derived_encoding_refuses() {
    let main = format!(
        "{MAIN}\ntype alias Card = {{ title : String; note : String }} deriving Schema\n\ntitle : Card -> String\nlet title c = c.title\n"
    );
    let f = project(&main);
    let at = f.at("App.Main", "c.title", 0) + 2;
    assert_eq!(
        f.ide().rename(f.file("App.Main"), at, "topic"),
        Err("cannot rename the field `title` of `Card`: `Card` derives `Schema`, whose encoding names the fields of `Card`".to_owned())
    );
}

#[test]
fn a_pattern_that_binds_the_label_refuses() {
    let main = format!("{MAIN}\nfirst_name : User -> String\nlet first_name {{ name }} = name\n");
    refused(
        &main,
        &format!(
            "`{{ name }}` at src/App/Main.fl:{} also binds `name`: write `{{ name = name }}` first",
            line(&main, "{ name }")
        ),
    );
}

#[test]
fn a_new_place_among_the_labels_refuses() {
    assert_eq!(
        rename_user_name(MAIN, "a_name"),
        Err("cannot rename the field `name` of `User`: `a_name` takes another place among the fields of `User`, which orders its comparisons and its text".to_owned())
    );
}

#[test]
fn an_error_in_the_project_refuses() {
    let main = format!("{MAIN}\nbroken : User -> Int\nlet broken u = u.name\n");
    refused(
        &main,
        "the project has errors, and an unchecked label may read it",
    );
}
