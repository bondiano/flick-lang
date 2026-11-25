use super::{Accepted, Session};
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::types::show_signature;

/// Enters `input`, which must check.
fn accept<'s>(session: &'s mut Session, input: &str) -> Accepted<'s> {
    match session.enter(input) {
        Ok(accepted) => accepted,
        Err(diagnostics) => panic!("`{input}` was rejected: {}", shown(&diagnostics)),
    }
}

/// Enters `input`, which must fail.
fn reject(session: &mut Session, input: &str) -> Vec<Diagnostic> {
    match session.enter(input) {
        Ok(accepted) => panic!("`{input}` was accepted as {}", accepted.phrase.module),
        Err(diagnostics) => diagnostics,
    }
}

fn shown(diagnostics: &[Diagnostic]) -> String {
    (diagnostics.iter())
        .map(|d| format!("{} {}", d.code, d.message))
        .collect::<Vec<_>>()
        .join("; ")
}

/// The type of the value `name` as the session sees it now.
fn type_of(session: &Session, name: &str) -> String {
    let scheme = session
        .value(name)
        .unwrap_or_else(|| panic!("no value `{name}`"));
    show_signature(scheme)
}

/// The names a phrase brought in, each with its type when it has one.
fn names(accepted: &Accepted<'_>) -> Vec<String> {
    (accepted.phrase.names.iter())
        .map(|e| match &e.scheme {
            Some(t) => format!("{} : {}", e.entity.name, show_signature(t)),
            None => e.entity.name.clone(),
        })
        .collect()
}

#[test]
fn each_phrase_kind_checks_as_a_module_of_its_own() {
    let mut session = Session::new();
    let x = accept(&mut session, "let x = 1");
    assert_eq!(x.phrase.module, "Repl.P1");
    assert_eq!(names(&x), ["x : Int"]);
    let f = accept(&mut session, "let f y = y + x");
    assert_eq!(f.phrase.module, "Repl.P2");
    assert_eq!(names(&f), ["f : Int -> Int"]);
    let shape = accept(&mut session, "type Shape = Circle Float | Square Float");
    assert_eq!(shape.phrase.module, "Repl.P3");
    assert_eq!(
        names(&shape),
        [
            "Shape",
            "Circle : Float -> Shape",
            "Square : Float -> Shape"
        ]
    );
    let class = accept(
        &mut session,
        "class Area a =\n  area : a -> Float\n\ninstance Area Shape =\n  let area s =\n    match s with\n    | Circle r -> 3.0 * r * r\n    | Square w -> w * w\n",
    );
    assert_eq!(class.phrase.module, "Repl.P4");
    assert_eq!(names(&class), ["Area", "area : Area a => a -> Float"]);
    accept(&mut session, "area (Square 2.0)");
    let import = accept(&mut session, "import List exposing (reverse)");
    assert_eq!(import.phrase.module, "Repl.P6");
    assert_eq!(names(&import), ["reverse : List a -> List a"]);
    accept(&mut session, "reverse [1, 2]");
    assert_eq!(type_of(&session, "it"), "List Int");
}

#[test]
fn a_later_phrase_shadows_and_earlier_ones_keep_what_they_saw() {
    let mut session = Session::new();
    accept(&mut session, "let x = 1");
    accept(&mut session, "let f () = x");
    accept(&mut session, "let x = \"a\"");
    assert_eq!(type_of(&session, "f"), "Unit -> Int");
    assert_eq!(type_of(&session, "x"), "String");
    // using the newest `x` next to `f` raises no ambiguity
    accept(&mut session, "let both = (f (), x)");
    assert_eq!(type_of(&session, "both"), "(Int, String)");
}

#[test]
fn an_expression_binds_it() {
    let mut session = Session::new();
    accept(&mut session, "1 + 2");
    assert_eq!(type_of(&session, "it"), "Int");
    accept(&mut session, "it * 2");
    assert_eq!(type_of(&session, "it"), "Int");
    accept(&mut session, "fun y -> y");
    assert_eq!(type_of(&session, "it"), "a -> a");
}

#[test]
fn an_error_points_into_the_phrase_and_leaves_the_session_as_it_was() {
    let mut session = Session::new();
    accept(&mut session, "let x = 1");
    let input = "let y = x + \"a\"";
    let diagnostics = reject(&mut session, input);
    let error = (diagnostics.iter())
        .find(|d| d.severity == Severity::Error)
        .expect("an error");
    let span = error.span.expect("a span");
    let file = session.sources().get(span.file);
    assert_eq!(file.text, input);
    assert_eq!(&input[span.start as usize..span.end as usize], "\"a\"");
    // the failed phrase took no name and no number
    assert!(session.value("y").is_none());
    let next = accept(&mut session, "let y = x + 1");
    assert_eq!(next.phrase.module, "Repl.P2");
    // an error in an expression is placed in its text too
    let diagnostics = reject(&mut session, "y +\n  \"b\"");
    let span = (diagnostics.iter().find_map(|d| d.span)).expect("a span");
    let text = &session.sources().get(span.file).text;
    assert_eq!(&text[span.start as usize..span.end as usize], "\"b\"");
}

#[test]
fn shadowed_constructors_imports_and_classes_raise_no_ambiguity() {
    let mut session = Session::new();
    accept(&mut session, "type Light = Red | Green");
    accept(&mut session, "type Signal = Red | Off");
    accept(&mut session, "let old = Green");
    assert_eq!(type_of(&session, "old"), "Light");
    accept(&mut session, "let new = Red");
    assert_eq!(type_of(&session, "new"), "Signal");
    accept(&mut session, "import List exposing (reverse)");
    accept(&mut session, "let reverse x = x + 1");
    assert_eq!(type_of(&session, "reverse"), "Int -> Int");
    accept(&mut session, "import List exposing (..)");
    assert_eq!(type_of(&session, "reverse"), "List a -> List a");
    accept(&mut session, "let mine = List.length [1]");
    for _ in 0..2 {
        let accepted = accept(&mut session, "class Size a =\n  size : a -> Int");
        let warnings = (accepted.diagnostics.iter())
            .filter(|d| [codes::R009, codes::R010].contains(&d.code))
            .count();
        assert_eq!(warnings, 0, "{}", shown(&accepted.diagnostics));
    }
}

#[test]
fn an_instance_of_an_earlier_phrase_stays_visible() {
    let mut session = Session::new();
    accept(&mut session, "class Size a =\n  size : a -> Int");
    accept(
        &mut session,
        "instance Size Bool =\n  let size b = if b then 1 else 0",
    );
    accept(&mut session, "let unrelated = 1");
    accept(&mut session, "let unrelated = 2");
    accept(&mut session, "size true");
    assert_eq!(type_of(&session, "it"), "Int");
}

/// Phrase 200 reuses what phrases 1–199 checked: only it is checked.
#[test]
fn a_new_phrase_checks_no_earlier_one_again() {
    let mut session = Session::new();
    for n in 1..200 {
        accept(&mut session, &format!("let v{n} = {n}"));
    }
    session.cache.explain(true);
    accept(&mut session, "let last = v1 + v199");
    let reasons = session.cache.reasons();
    let checked: Vec<&String> = (reasons.iter())
        .filter(|(_, reason)| reason != "reused")
        .map(|(module, _)| module)
        .collect();
    assert_eq!(checked, ["Repl.P200"], "{reasons:?}");
    assert_eq!(reasons.len(), 200);
}

#[test]
fn a_layout_places_spans_of_the_input_and_moves_them_past_insertions() {
    use super::render::Layout;
    use crate::source::{FileId, Span};

    let (module, input) = (FileId::new(0), FileId::new(1));
    let at =
        |layout: &Layout, start: u32, end: u32| layout.place(Span::new(module, start, end), input);
    // "head\n" is generated; "ab\ncd" is the input from offset 0
    let layout = Layout::declarations("head\n", "ab\ncd");
    assert_eq!(layout.text, "head\nab\ncd\n");
    assert_eq!(at(&layout, 5, 7), Some(Span::new(input, 0, 2)));
    assert_eq!(at(&layout, 0, 4), None, "a span of the generated head");
    assert_eq!(
        at(&layout, 8, 11),
        Some(Span::new(input, 3, 5)),
        "an end past the run is cut"
    );
    // each line of an expression is a run of its own, indented
    let layout = Layout::expression("", "f\n  x", Some("Repl.P1"));
    assert_eq!(layout.text, "let it =\n  let it = Repl.P1.it\n  f\n    x\n");
    let x = u32::try_from(layout.text.rfind('x').expect("x")).expect("small");
    assert_eq!(at(&layout, x, x + 1), Some(Span::new(input, 4, 5)));
    // a line inserted inside a run splits it and moves what follows
    let layout = Layout::declarations("", "let a = 1\nlet f x = x\n");
    let signed = layout.inserted(&[(10, "f : a -> a\n".to_owned())]);
    assert_eq!(signed.text, "let a = 1\nf : a -> a\nlet f x = x\n\n");
    assert_eq!(at(&signed, 0, 3), Some(Span::new(input, 0, 3)));
    assert_eq!(
        at(&signed, 10, 11),
        None,
        "the inserted signature is generated"
    );
    assert_eq!(at(&signed, 21, 26), Some(Span::new(input, 10, 15)));
}
