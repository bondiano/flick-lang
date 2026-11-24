//! Field paths in the editor: what each step of an accessor reads, the
//! fields completed after a `.` of finished and unfinished paths, hover on
//! labels and on a whole accessor, and an accessor's hidden parameter,
//! which no query shows.

use super::{Ide, Symbol};
use super::tests::{Fixture, fixture_of};
use crate::diagnostics::Severity;
use crate::source::FileId;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::show_type;

const MAIN: &str = "module App.Main exposing (..)

/// Where someone lives.
type alias Address = {
  /// The city, as the post writes it.
  city : String;
  zip : String;
}

type alias User = { name : String; address : Address }

get_city : User -> String
let get_city = .address.city

cities : User -> List User -> List String
let cities first users =
  let get = .name
  let names = users |> List.map get
  List.map .address.city users ++ [.address.city first, first |> .name] ++ names
";

fn main(text: &str) -> Fixture {
    fixture_of(&[("src/App/Main.fl", text)])
}

/// The steps of the accessor at `offset`: each receiver and the field it
/// reads, shown, and the accessor's type.
fn steps(ide: &Ide<'_>, file: FileId, offset: u32) -> (Vec<(String, String)>, String) {
    let m = ide.module(file).expect("module");
    let path = super::tree::path_to(&m.tree, offset);
    let accessor = path
        .iter()
        .rev()
        .find(|n| n.kind == K::AccessorExpr)
        .expect("an accessor");
    let text = ide.text(file);
    let labels = accessor.nodes().map(|s| {
        s.child_tokens()
            .find(|t| t.kind == K::Ident)
            .map(|t| t.text(text))
    });
    let read = (ide.step_receivers(m, accessor).into_iter())
        .zip(labels)
        .map(|(receiver, label)| {
            let receiver = receiver.expect("a receiver");
            let field = label
                .and_then(|l| ide.field_type(file, &receiver, l))
                .map_or_else(|| "-".to_owned(), |t| show_type(&t));
            (show_type(&receiver), field)
        })
        .collect();
    let whole = m.typing.types.get(&accessor.range()).expect("its type");
    (read, show_type(whole))
}

fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
    (items.iter())
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

#[test]
fn each_step_reads_the_field_of_the_record_before_it() {
    let f = main(MAIN);
    let ide = f.ide();
    let file = f.file("App.Main");
    let signed = f.at("App.Main", ".address.city", 0);
    assert_eq!(
        steps(&ide, file, signed),
        (
            pairs(&[("User", "Address"), ("Address", "String")]),
            "User -> String".to_owned()
        )
    );
    // the record a later argument gives, once the call is solved
    let mapped = f.at("App.Main", ".address.city users", 0);
    let record = "{ address : Address; name : String }";
    assert_eq!(
        steps(&ide, file, mapped),
        (
            pairs(&[(record, "Address"), ("Address", "String")]),
            format!("{record} -> String")
        )
    );
    // generalized, a step reads a field of any record that has it
    let general = f.at("App.Main", ".name", 0);
    assert_eq!(
        steps(&ide, file, general),
        (
            pairs(&[("{ name : a | b }", "a")]),
            "{ name : a | b } -> a".to_owned()
        )
    );
}

#[test]
fn a_step_without_its_label_reads_the_field_before_it() {
    let text = MAIN.replace(
        "List.map .address.city users ++",
        "List.map .address. users ++",
    );
    let f = main(&text);
    let ide = f.ide();
    let at = f.at("App.Main", ".address. users", 0);
    let (read, _) = steps(&ide, f.file("App.Main"), at);
    let record = "{ address : Address; name : String }";
    assert_eq!(read, pairs(&[(record, "Address"), ("Address", "-")]));
}

/// The labels and types completed after the `.` at `needle`'s `nth`
/// occurrence plus `shift`, in `text`.
fn completed(text: &str, needle: &str, shift: u32) -> Vec<(String, String)> {
    let f = main(text);
    let at = f.at("App.Main", needle, 0) + shift;
    assert_eq!(&text[at as usize..=at as usize], ".", "{needle}");
    (f.ide().complete(f.file("App.Main"), at, true).into_iter())
        .map(|c| (c.label, c.detail.unwrap_or_default()))
        .collect()
}

const ADDRESS: [(&str, &str); 2] = [("city", "String"), ("zip", "String")];
const USER: [(&str, &str); 2] = [("address", "Address"), ("name", "String")];

#[test]
fn a_later_argument_gives_the_record_of_an_unfinished_path() {
    let text = MAIN.replace(
        "List.map .address.city users ++",
        "List.map .address. users ++",
    );
    assert_eq!(completed(&text, ".address. users", 8), pairs(&ADDRESS));
}

#[test]
fn a_pipeline_gives_the_record_of_its_last_step() {
    let text = MAIN.replace("users |> List.map get", "users |> List.map .address.");
    assert_eq!(completed(&text, ".address.\n", 8), pairs(&ADDRESS));
}

#[test]
fn a_label_being_written_completes_the_fields_of_its_receiver() {
    let text = MAIN.replace("List.map .address.city users", "List.map .addr users");
    assert_eq!(completed(&text, ".addr users", 0), pairs(&USER));
    let text = MAIN.replace("List.map .address.city users", "List.map .address.ci users");
    assert_eq!(completed(&text, ".address.ci", 8), pairs(&ADDRESS));
}

#[test]
fn an_annotation_gives_the_record_of_a_bare_dot() {
    let text = format!("{MAIN}\npick : User -> String\nlet pick = .\n");
    assert_eq!(completed(&text, "pick = .", 7), pairs(&USER));
}

#[test]
fn a_projection_completes_the_fields_of_its_receiver() {
    let text = MAIN.replace("let get = .name", "let get = first.address.");
    assert_eq!(completed(&text, "first.address.", 13), pairs(&ADDRESS));
    let text = MAIN.replace("let get = .name", "let get = first.");
    assert_eq!(completed(&text, "first.", 5), pairs(&USER));
}

#[test]
fn a_label_shows_its_field_and_an_accessor_its_function() {
    let f = main(MAIN);
    let ide = f.ide();
    let file = f.file("App.Main");
    let city = f.at("App.Main", "city first", 0) + 1;
    assert_eq!(
        ide.hover(file, city).expect("hover").0,
        "```flick\ncity : String\n```\n\nThe city, as the post writes it.\n\nfield of `Address` from `App.Main`"
    );
    let dot = f.at("App.Main", ".address.city", 0);
    let (shown, range) = ide.hover(file, dot).expect("hover");
    assert_eq!(
        shown,
        "```flick\n.address.city : User -> String\n```\n\nA function reading `.address.city` of its argument: a `String`."
    );
    assert_eq!(range, (dot, dot + 13));
    let general = f.at("App.Main", ".name", 0);
    assert!(
        (ide.hover(file, general).expect("hover").0)
            .starts_with("```flick\n.name : { name : a | b } -> a\n```"),
    );
}

/// No query shows the parameter an accessor hides, by any name: it is
/// neither a local in scope nor a symbol.
#[test]
fn the_hidden_parameter_is_no_symbol() {
    let f = main(MAIN);
    let ide = f.ide();
    let file = f.file("App.Main");
    let start = f.at("App.Main", "List.map .address.city users", 0);
    for offset in start..start + 40 {
        let names: Vec<String> = (ide.complete(file, offset, false).into_iter())
            .map(|c| c.label)
            .filter(|l| !l.starts_with(char::is_alphabetic))
            .collect();
        assert!(names.is_empty(), "{offset}: {names:?}");
        if let Some((shown, _)) = ide.hover(file, offset) {
            assert!(!shown.contains('$'), "{offset}: {shown}");
        }
        if let Some((symbol, _)) = ide.symbol_at(file, offset) {
            assert!(
                !matches!(symbol, Symbol::Local { ref name, .. } if name.contains('$')),
                "{offset}: {symbol:?}"
            );
        }
    }
    let outline = ide.document_symbols(file);
    let names: Vec<&str> = outline.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, ["Address", "User", "get_city", "cities"]);
    assert!(ide.inlay_hints(file).iter().all(|(_, h)| !h.contains('$')));
}

/// An unfinished step is reported at its `.`, and what follows it is
/// checked: the editor still knows the statements after it.
#[test]
fn an_unfinished_step_keeps_the_rest_of_the_module() {
    let text = MAIN.replace(
        "List.map .address.city users ++",
        "List.map .address. users ++",
    );
    let f = main(&text);
    let ide = f.ide();
    let dot = f.at("App.Main", ". users", 0);
    let errors: Vec<_> = (ide.analysis.diagnostics.iter())
        .filter(|d| d.severity == Severity::Error)
        .map(|d| (d.code.to_string(), d.span.map(|s| (s.start, s.end))))
        .collect();
    assert_eq!(errors, [("FLICK-S007".to_owned(), Some((dot, dot + 1)))]);
    let names = f.at("App.Main", "++ names", 0) + 3;
    assert_eq!(
        ide.hover(f.file("App.Main"), names).expect("hover").0,
        "```flick\nnames : List String\n```"
    );
}
