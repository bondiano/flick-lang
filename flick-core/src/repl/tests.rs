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

#[test]
fn vm_target_session_metadata_rollback_and_commands() {
    use super::HandlerStyle;
    use crate::types::show_type;

    let mut session = Session::with_target("vm");
    let decl = accept(&mut session, "let n = 41");
    assert!(!decl.expression);
    assert!(!decl.is_task);
    assert!(decl.value_type.is_none());

    let expr = accept(&mut session, "n + 1");
    assert!(expr.expression);
    assert!(!expr.is_task);
    assert_eq!(
        expr.value_type.as_ref().map(show_type).as_deref(),
        Some("Int")
    );
    assert!(expr.effects.is_empty());

    // Direct host-effect call auto-wraps in a task and resolves Host.Console ambiently
    let printed = accept(&mut session, "Platform.stdout_line \"hi\"");
    assert!(printed.expression);
    assert!(printed.is_task);
    assert_eq!(
        printed.ok_type.as_ref().map(show_type).as_deref(),
        Some("Unit")
    );
    assert!(printed.effects.is_empty(), "{:?}", printed.effects);

    // Rollback restores phrase list and scope cleanly
    assert!(session.rollback_last());
    assert_eq!(session.phrases().len(), 2);
    assert_eq!(type_of(&session, "it"), "Int");

    // Task expression reports ok_type and err_type
    accept(&mut session, "type error Boom = Boom String");
    let ok_task = accept(&mut session, "task { 7 }");
    assert!(ok_task.is_task);
    assert_eq!(
        ok_task.ok_type.as_ref().map(show_type).as_deref(),
        Some("Int")
    );
    assert!(ok_task.err_type.is_none());

    let fail_task = accept(&mut session, "task { Fail.fail (Boom \"oops\") }");
    assert!(fail_task.is_task);
    assert!(fail_task.err_type.is_some());

    // User effect is reported as unresolved until `:handle` installs a handler
    accept(&mut session, "effect Ask =\n  ask : Unit ->{} Int");
    let unhandled = accept(&mut session, "Ask.ask ()");
    assert!(unhandled.is_task);
    assert_eq!(unhandled.effects.len(), 1);
    assert!(
        unhandled.effects[0].ends_with(".Ask"),
        "{:?}",
        unhandled.effects
    );
    assert!(session.rollback_last());

    let handle = session
        .enter_handle("fun body -> try body () with | Ask.ask () -> 42")
        .unwrap_or_else(|d| panic!("handle rejected: {}", shown(&d)));
    assert_eq!(
        handle.style,
        HandlerStyle::Thunk {
            returns_task: false
        }
    );
    assert_eq!(handle.handled_effects.len(), 1);
    assert!(handle.handled_effects[0].ends_with(".Ask"));

    let handled = accept(&mut session, "Ask.ask ()");
    assert!(handled.is_task);
    assert!(handled.effects.is_empty(), "{:?}", handled.effects);

    // `:provide` checks a layer and records its provided service
    accept(&mut session, "service Cfg =\n  port : Unit ->{} Int");
    let provide = session
        .enter_provide("Layer.succeed Cfg { port = fun () -> 8080 }")
        .unwrap_or_else(|d| panic!("provide rejected: {}", shown(&d)));
    assert_eq!(provide.provided_services.len(), 1);
    assert!(provide.provided_services[0].ends_with(".Cfg"));
    let used_cfg = accept(&mut session, "Cfg.port ()");
    assert!(used_cfg.effects.is_empty(), "{:?}", used_cfg.effects);

    // `:bg` accepts tasks without overwriting `it` and rejects non-tasks
    accept(&mut session, "1 + 2");
    assert_eq!(type_of(&session, "it"), "Int");
    let bg = session
        .enter_bg("task { 99 }")
        .unwrap_or_else(|d| panic!("bg rejected: {}", shown(&d)));
    assert_eq!(bg.ok_type.as_ref().map(show_type).as_deref(), Some("Int"));
    assert_eq!(type_of(&session, "it"), "Int");
    assert!(session.enter_bg("123").is_err());
}
