//! A field read of a record literal, written there or given by an inlined
//! helper, is that field in release: each other field that is not data
//! still runs once, in order, whether it is selected or not.

use super::list_shape_tests::{decl, validated};
use super::tests::release;

/// The module `App` exposing `exposed`, then `decls`: `note` writes its
/// argument when it is evaluated, `boom` fails, `spin` never returns, and
/// `summarize` is a private helper that builds a record.
fn app(exposed: &str, decls: &str) -> String {
    format!(
        "module App exposing ({exposed})\n\ntype error Boom = Boom String\n\n\
         note : Int ->{{Host.Console}} Int\nlet note n =\n  Platform.stdout_line (show n)\n  n\n\n\
         boom : String ->{{; Boom}} Int\nlet boom label = Fail.fail (Boom label)\n\n\
         spin : Int -> Int\nlet rec spin n = spin n\n\n\
         type alias Info = {{ name : String }}\n\ntype alias Slot = {{ info : Info }}\n\n\
         type alias Pokemon = {{ id : Int; name : String; types : List Slot }}\n\n\
         type alias Summary = {{ id : Int; name : String; types : List String }}\n\n\
         summarize : Pokemon -> Summary\nlet summarize pokemon = {{ id = pokemon.id; name = pokemon.name; \
         types = List.map (fun item -> item.info.name) pokemon.types }}\n\n{decls}"
    )
}

/// The printed optimized definition of `name` in `printed`.
fn line<'a>(printed: &'a str, name: &str) -> &'a str {
    let start = format!("let {name} = ");
    let found = printed.lines().find(|l| l.starts_with(&start));
    found.unwrap_or_else(|| panic!("no {name} in\n{printed}"))
}

const PURE: &str = "\
picked : Int -> String -> String\nlet picked x s = { a = x; b = s; c = [x] }.b\n\n\
nested : Int -> Int\nlet nested x = { a = { b = x + 1; c = x }; d = 2 }.a.c\n";

#[test]
fn a_field_of_a_known_record_is_that_field() {
    let text = app(
        "picked, nested, types_of",
        &format!(
            "{PURE}\ntypes_of : Pokemon -> List String\nlet types_of pokemon = (summarize pokemon).types\n"
        ),
    );
    let js = release(&text);
    assert_eq!(
        decl(&js, "picked"),
        "export const picked = (x) => (s) => s;"
    );
    assert_eq!(decl(&js, "nested"), "export const nested = (x) => x;");
    // no record is built, nor its `id` and `name` read
    assert_eq!(
        decl(&js, "types_of"),
        "export const types_of = (pokemon) => {\n  const $t1 = [];\n  \
         for (const $elem4 of pokemon.types) {\n    $t1.push($elem4.info.name);\n  }\n  return $t1;\n};"
    );
    let ir = validated(&text);
    assert_eq!(line(&ir, "picked"), "let picked = (fun x s -> s)");
    assert_eq!(line(&ir, "nested"), "let nested = (fun x -> x)");
}

const OBSERVED: &str = "\
traced : Int ->{Host.Console} Int\nlet traced x = { a = note 1; b = x; c = note 3 }.b\n\n\
chosen : Int ->{Host.Console} Int\nlet chosen x = { a = note 4; b = note 5; c = note 6 }.b\n\n\
deep : Int ->{Host.Console} Int\nlet deep x = { a = { b = note 7; c = x }; d = note 8 }.a.c\n\n\
helped : Int ->{Host.Console} List String\n\
let helped x = (summarize { id = note 9; name = show x; types = [] }).types\n\n\
thrown : Int ->{; Boom} Int\nlet thrown x = { a = x; b = boom \"late\" }.a\n\n\
between : Int ->{Host.Console; Boom} Int\n\
let between x = { a = x; b = note 10; c = boom \"mid\"; d = note 11 }.a\n\n\
divided : Int -> Int -> Int\nlet divided x y = { a = x; b = Int.unsafe_div 1 y }.a\n\n\
spun : Int -> Int\nlet spun x = { a = x; b = spin 0 }.a\n";

const OBSERVERS: &str = "traced, chosen, deep, helped, thrown, between, divided, spun";

#[test]
fn every_field_runs_once_in_order_selected_or_not() {
    let text = app(OBSERVERS, OBSERVED);
    let js = release(&text);
    for (name, notes) in [
        ("traced", &["note(1)", "note(3)"][..]),
        ("chosen", &["note(4)", "note(5)", "note(6)"]),
        ("deep", &["note(7)", "note(8)"]),
        ("helped", &["note(9)"]),
        ("between", &["note(10)", "boom(\"mid\")", "note(11)"]),
    ] {
        let d = decl(&js, name);
        let at: Vec<_> = notes.iter().map(|n| d.find(n)).collect();
        assert!(at.iter().all(Option::is_some) && at.is_sorted(), "{d}");
        assert_eq!(d.matches("yield*").count(), notes.len(), "{d}");
        assert!(
            !d.contains(": yield*") && !d.contains("}).") && !d.contains("id:"),
            "{d}"
        );
    }
    assert!(decl(&js, "traced").ends_with("return x;\n});"), "{js}");
    for (name, kept) in [
        ("thrown", "boom(\"late\")"),
        ("divided", "unsafe_div"),
        ("spun", "spin(0)"),
    ] {
        let d = decl(&js, name);
        assert_eq!(d.matches(kept).count(), 1, "{d}");
        assert!(!d.contains("a: x") && d.contains("return x;"), "{d}");
    }
    let ir = validated(&text);
    assert_eq!(
        line(&ir, "chosen"),
        "let chosen = (fun x! -> { let $p4 = (App.note 4)!; let $p5 = (App.note 5)!; let $p6 = (App.note 6)!; $p5 })"
    );
    assert_eq!(
        line(&ir, "traced"),
        "let traced = (fun x! -> { let $p2 = (App.note 1)!; let $p3 = (App.note 3)!; x })"
    );
}

#[test]
fn a_consumer_of_the_whole_record_gets_every_field() {
    let text = app(
        "both",
        "both : Pokemon -> (List String, Summary)\nlet both pokemon =\n  let s = summarize pokemon\n  (s.types, s)\n",
    );
    let both = decl(&release(&text), "both").to_owned();
    for field in ["id: pokemon.id", "name: pokemon.name", "types: "] {
        assert!(both.contains(field), "{both}");
    }
    validated(&text);
}

#[cfg(feature = "native-project")]
#[test]
fn go_reads_the_field_without_building_the_record() {
    use super::list_shape_tests::{go_fn, go_of};

    let go = go_of(&app(
        &format!("main, picked, nested, {OBSERVERS}"),
        &format!(
            "{PURE}\n{OBSERVED}\nmain : Unit ->{{Host.Console; AppError}} Unit\nlet main () =\n  \
             Platform.stdout_line (show (picked 1 \"s\", nested 2, traced 3, chosen 4, deep 5, helped 6, spun 7))\n"
        ),
    ));
    // the pure ones are inlined into `main`
    let main = go_fn(&go, "main");
    assert!(!main.contains("R_") && main.contains("= \"s\"\n"), "{main}");
    assert_eq!(
        main.matches("F_App__spin(fb, int32(0))").count(),
        1,
        "{main}"
    );
    for (name, notes) in [("traced", 2), ("chosen", 3), ("deep", 2), ("helped", 1)] {
        let f = go_fn(&go, name);
        assert!(!f.contains("R_"), "{f}");
        assert_eq!(f.matches("F_App__note(").count(), notes, "{f}");
    }
}
