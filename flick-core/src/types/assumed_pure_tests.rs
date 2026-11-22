use std::collections::HashMap;

use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::resolve::{self, Module, NO_GENERATED};
use crate::source::SourceMap;
use crate::syntax;
use crate::types::lint_tests::{HEAD, assert_fix, check, fixed};
use crate::types::{self, Limits, Options, Thresholds, Watched};

/// The `FLICK-T003` message for `text`, panicking unless there is one.
fn t003_message(text: &str) -> String {
    let diagnostics = check(text);
    let messages: Vec<&str> = diagnostics
        .iter()
        .filter(|d| d.code == codes::T003)
        .map(|d| d.message.as_str())
        .collect();
    match messages[..] {
        [message] => message.to_owned(),
        _ => panic!("expected one FLICK-T003, got {diagnostics:?}"),
    }
}

/// Checks a chain of modules in dependency order, returning the last one's
/// diagnostics.
fn check_modules(modules: &[(&str, &str, &str)]) -> Vec<Diagnostic> {
    let mut sources = SourceMap::default();
    let files: Vec<_> = modules
        .iter()
        .map(|&(name, path, text)| (name, sources.add(path, text.to_owned()), text))
        .collect();
    let mut interfaces = HashMap::new();
    let mut last_diags = Vec::new();
    for &(name, file, text) in &files {
        let parsed = syntax::parse(text, file);
        assert!(
            parsed.diagnostics.is_empty(),
            "{name}: {:?}",
            parsed.diagnostics
        );
        let module_syntax = resolve::module_syntax(&parsed.tree, text, file);
        let module = Module {
            name,
            file,
            text,
            tree: &parsed.tree,
            syntax: &module_syntax,
            generated: NO_GENERATED,
        };
        let lookup = |m: &str| {
            interfaces
                .get(m)
                .cloned()
                .or_else(|| types::builtins().iter().find(|i| i.module == m).cloned())
        };
        let (mut iface, mut diags) = resolve::resolve(&module, &lookup);
        let local = |_: &str| true;
        let options = Options {
            local: &local,
            limits: Limits::default(),
            no_let_assert: false,
            thresholds: Thresholds::default(),
            watched: Watched::none(),
            stages_by_class: false,
        };
        diags.extend(types::check(
            &module,
            &lookup,
            types::builtins(),
            options,
            &mut iface,
        ));
        last_diags = diags;
        interfaces.insert(name.to_owned(), iface);
    }
    last_diags
}

/// Annotated with the runtime arrow it is given, the callback fits it.
#[test]
fn a_callback_taken_for_pure_is_annotated() {
    let head = "service Log =\n  info : String ->{} Unit\n\nsay : Unit ->{Log} Unit\nlet say () = Log.info \"hi\"\n\napply : Unit ->{Log} Unit\n";
    let before = format!("{head}let apply () = (fun h -> h ()) say\n");
    assert_eq!(
        t003_message(&format!("{HEAD}{before}")),
        "expected `Unit -> _`, found `Unit ->{Log} Unit`"
    );
    assert_fix(
        &before,
        "FLICK-T003",
        &format!("{head}let apply () = (fun (h : Unit ->{{Log}} Unit) -> h ()) say\n"),
    );
}

/// A local function is generalized, so its use instantiates the arrow
/// taken for its parameter afresh: the note and the fix still find it.
#[test]
fn a_callback_of_a_local_function_is_annotated() {
    let head = "service Log =\n  info : String ->{} Unit\n\nsay : Unit ->{Log} Unit\nlet say () = Log.info \"hi\"\n\napply : Unit ->{Log} Unit\n";
    let text = format!("{HEAD}{head}let apply () =\n  let apply_it h = h ()\n  apply_it say\n");
    let diagnostics = check(&text);
    let error = (diagnostics.iter())
        .find(|d| d.code.to_string() == "FLICK-T003")
        .unwrap_or_else(|| panic!("{diagnostics:?}"));
    assert!(
        (error.related.iter()).any(|r| r.message.starts_with("`h` has no type written")),
        "{error:?}"
    );
    let fixed = fixed(&text, "FLICK-T003").expect("the fix");
    assert!(
        fixed.contains("let apply_it (h : Unit ->{Log} Unit) = h ()"),
        "{fixed}"
    );
}

const SAY: &str = "service Log =\n  info : String ->{} Unit\n\nsay : Unit ->{Log} Unit\nlet say () = Log.info \"hi\"\n\napply : Unit ->{Log} Unit\nlet apply () =\n";

/// The `FLICK-T003` fix of `before` annotates `from` as `to` only. A `fun`
/// annotated so leaves no errors; a parameter of a `let` header takes its
/// type in a signature instead (`FLICK-T005`), so there the text is checked.
fn assert_annotated_in(before: &str, from: &str, to: &str) {
    let before = format!("{HEAD}{before}");
    assert_eq!(before.matches(from).count(), 1, "{from}");
    let after = before.replacen(from, to, 1);
    assert_eq!(fixed(&before, "FLICK-T003").as_deref(), Some(&*after));
    if to.starts_with("fun ") {
        let errors: Vec<_> = (check(&after).into_iter())
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "{errors:?}");
    }
}

/// As `assert_annotated_in`, for the body of `apply`.
fn assert_annotated(body: &str, from: &str, to: &str) {
    assert_annotated_in(&format!("{SAY}{body}"), from, to);
}

/// The note and the fix are at the parameter of the function applied, not
/// at a local before it whose arrow has the same shape.
#[test]
fn a_callback_is_annotated_at_the_function_applied() {
    // a local function with a callback of its own
    assert_annotated(
        "  let call f x = f x\n  let apply_it h = h ()\n  call (fun x -> x) ()\n  apply_it say\n",
        "apply_it h =",
        "apply_it (h : Unit ->{Log} Unit) =",
    );
    // two of the same shape, the second applied, also through a pipe
    for applied in ["run_it say", "say |> run_it"] {
        assert_annotated(
            &format!(
                "  let apply_it h = h ()\n  let run_it g = g ()\n  apply_it (fun () -> ())\n  {applied}\n"
            ),
            "run_it g =",
            "run_it (g : Unit ->{Log} Unit) =",
        );
    }
    let first = "  let first k = k ()\n  first (fun () -> ())\n";
    assert_annotated(
        &format!("{first}  let run = fun h -> h ()\n  run say\n"),
        "fun h ->",
        "fun (h : Unit ->{Log} Unit) ->",
    );
    // the callback as the second parameter
    assert_annotated(
        &format!("{first}  let both x h = h x\n  both () say\n"),
        "both x h =",
        "both x (h : Unit ->{Log} Unit) =",
    );
}

/// An arrow taking a record is matched as a whole, not by its fields.
#[test]
fn a_callback_taking_a_record_is_annotated_at_the_function_applied() {
    let text = SAY.replace(
        "apply : ",
        "log_record : { a : Int } ->{Log} Unit\nlet log_record r = Log.info (show r.a)\n\napply : ",
    );
    let body = "  let first k = k { a = 2 }\n  first (fun _ -> ())\n  let with_record h = h { a = 1 }\n  with_record log_record\n";
    assert_annotated_in(
        &format!("{text}{body}"),
        "with_record h =",
        "with_record (h : { a : Int } ->{Log} Unit) =",
    );
}

/// Applied through a function that is not a local, the arrow taken for a
/// callback is still found when it is the one expected; a local whose
/// arrow is generic since is not.
#[test]
fn a_callback_applied_through_another_function_is_annotated() {
    let unrelated = "  let unrelated h = h ()\n  unrelated (fun () -> ())\n";
    assert_annotated(
        &format!("{unrelated}  List.each_effect (fun k -> k ()) [say]\n"),
        "fun k ->",
        "fun (k : Unit ->{Log} Unit) ->",
    );
    // `List.map` takes a pure function, so only the text of its fix is checked
    let mapped = format!("{HEAD}{SAY}{unrelated}  ignore (List.map (fun k -> k ()) [say])\n");
    assert_eq!(
        fixed(&mapped, "FLICK-T003"),
        Some(mapped.replace("fun k ->", "fun (k : Unit ->{Log} Unit) ->"))
    );
    assert_annotated(
        &format!("{unrelated}  (fun k -> k ()) say\n"),
        "fun k ->",
        "fun (k : Unit ->{Log} Unit) ->",
    );
}

/// Repro for #707: a callback needing a service from another module such as
/// `Host.Console` spells it the way this module can name it in both the
/// T003 message and the inserted annotation, so applying the fix leaves no
/// errors.
#[test]
fn a_callback_using_a_host_service_names_the_module_in_fix_and_message() {
    let before = "module Main exposing (main)\n\n\
        say : Unit ->{Host.Console} Unit\n\
        let say () = Platform.stdout_line \"hi\"\n\n\
        main : Unit ->{Host.Console} Unit\n\
        let main () =\n  \
          let run = fun action -> action ()\n  \
          run say\n";
    assert_eq!(
        t003_message(before),
        "expected `Unit -> _`, found `Unit ->{Host.Console} Unit`"
    );
    let after = fixed(before, "FLICK-T003").expect("T003 fix");
    assert_eq!(
        after,
        "module Main exposing (main)\n\n\
        say : Unit ->{Host.Console} Unit\n\
        let say () = Platform.stdout_line \"hi\"\n\n\
        main : Unit ->{Host.Console} Unit\n\
        let main () =\n  \
          let run = fun (action : Unit ->{Host.Console} Unit) -> action ()\n  \
          run say\n"
    );
    let errors: Vec<_> = check(&after)
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
}

/// Exposed, aliased, Prelude and shadowed services each spell the service
/// the way the current module can write it.
#[test]
fn a_callback_names_services_as_the_module_imports_them() {
    // Exposed from another module: uses its short name.
    let exposed = "import Host exposing (Console)\n\n\
        say : Unit ->{Console} Unit\n\
        let say () = Platform.stdout_line \"hi\"\n\n\
        apply : Unit ->{Console} Unit\n\
        let apply () =\n  \
          let run = fun action -> action ()\n  \
          run say\n";
    assert_eq!(
        t003_message(&format!("{HEAD}{exposed}")),
        "expected `Unit -> _`, found `Unit ->{Console} Unit`"
    );
    assert_fix(
        exposed,
        "FLICK-T003",
        &exposed.replace(
            "fun action -> action ()",
            "fun (action : Unit ->{Console} Unit) -> action ()",
        ),
    );

    // Aliased import: uses the module's alias.
    let aliased = "import Host as H\n\n\
        say : Unit ->{H.Console} Unit\n\
        let say () = Platform.stdout_line \"hi\"\n\n\
        apply : Unit ->{H.Console} Unit\n\
        let apply () =\n  \
          let run = fun action -> action ()\n  \
          run say\n";
    assert_eq!(
        t003_message(&format!("{HEAD}{aliased}")),
        "expected `Unit -> _`, found `Unit ->{H.Console} Unit`"
    );
    assert_fix(
        aliased,
        "FLICK-T003",
        &aliased.replace(
            "fun action -> action ()",
            "fun (action : Unit ->{H.Console} Unit) -> action ()",
        ),
    );

    // Local service of the same name shadows `Console`, so `Host.Console`
    // must stay qualified.
    let shadowed = "service Console =\n  write : String ->{} Unit\n\n\
        say : Unit ->{Host.Console} Unit\n\
        let say () = Platform.stdout_line \"hi\"\n\n\
        apply : Unit ->{Host.Console} Unit\n\
        let apply () =\n  \
          let run = fun action -> action ()\n  \
          run say\n";
    assert_eq!(
        t003_message(&format!("{HEAD}{shadowed}")),
        "expected `Unit -> _`, found `Unit ->{Host.Console} Unit`"
    );
    assert_fix(
        shadowed,
        "FLICK-T003",
        &shadowed.replace(
            "fun action -> action ()",
            "fun (action : Unit ->{Host.Console} Unit) -> action ()",
        ),
    );

    // Prelude service (`Async`) keeps its short name.
    let prelude = "wait : Unit ->{Async} Unit\n\
        foreign effect wait from \"./wait.mjs\" \"wait\"\n\n\
        apply : Unit ->{Async} Unit\n\
        let apply () =\n  \
          let run = fun action -> action ()\n  \
          run wait\n";
    assert_eq!(
        t003_message(&format!("{HEAD}{prelude}")),
        "expected `Unit -> _`, found `Unit ->{Async} Unit`"
    );
    assert_fix(
        prelude,
        "FLICK-T003",
        &prelude.replace(
            "fun action -> action ()",
            "fun (action : Unit ->{Async} Unit) -> action ()",
        ),
    );
}

/// Foreign parameter, return, error and `Service.Impl` types alongside a
/// foreign service are qualified in both the message and inserted fix.
#[test]
fn a_callback_names_foreign_param_return_error_and_impl_types() {
    let foreign_types = "inspect : Duration.Duration ->{Host.Console; PlatformError.PlatformError} Duration.Duration\n\
        let inspect d =\n  \
          Platform.stdout_line \"hi\"\n  \
          d\n\n\
        apply : Duration.Duration ->{Host.Console; PlatformError.PlatformError} Duration.Duration\n\
        let apply d =\n  \
          let run = fun action -> action d\n  \
          run inspect\n";
    assert_eq!(
        t003_message(&format!("{HEAD}{foreign_types}")),
        "expected `Duration.Duration -> _`, found `Duration.Duration ->{Host.Console; PlatformError.PlatformError} Duration.Duration`"
    );
    assert_fix(
        foreign_types,
        "FLICK-T003",
        &foreign_types.replace(
            "fun action -> action d",
            "fun (action : Duration.Duration ->{Host.Console; PlatformError.PlatformError} Duration.Duration) -> action d",
        ),
    );

    let service_impl = "use_impl : Host.Console.Impl ->{Host.Console} Unit\n\
        let use_impl _ = Platform.stdout_line \"hi\"\n\n\
        apply : Host.Console.Impl ->{Host.Console} Unit\n\
        let apply impl =\n  \
          let run = fun action -> action impl\n  \
          run use_impl\n";
    let impl_msg = t003_message(&format!("{HEAD}{service_impl}"));
    assert!(
        impl_msg.ends_with(", found `Host.Console.Impl ->{Host.Console} Unit`"),
        "{impl_msg}"
    );
    assert_fix(
        service_impl,
        "FLICK-T003",
        &service_impl.replace(
            "fun action -> action impl",
            "fun (action : Host.Console.Impl ->{Host.Console} Unit) -> action impl",
        ),
    );
}

/// Even without an assumed-pure callback, a T003 mismatch involving a
/// foreign service row names the service as this module can name it.
#[test]
fn t003_message_qualifies_foreign_service_in_annotated_pure_mismatch() {
    let text = "module Main exposing (main)\n\n\
        say : Unit ->{Host.Console} Unit\n\
        let say () = Platform.stdout_line \"hi\"\n\n\
        pure_only : (Unit -> Unit) -> Unit\n\
        let pure_only f = f ()\n\n\
        main : Unit ->{Host.Console} Unit\n\
        let main () = pure_only say\n";
    assert_eq!(
        t003_message(text),
        "expected `Unit -> Unit`, found `Unit ->{Host.Console} Unit`"
    );

    // An omitted foreign service (`-Host.Console`) in a row also keeps its qualifier.
    let without = "module Main exposing (main)\n\n\
        need_without : Ref (Unit ->{-Host.Console | r} Unit) -> Unit\n\
        let need_without _ = ()\n\n\
        say : Unit ->{Host.Console} Unit\n\
        let say () = Platform.stdout_line \"hi\"\n\n\
        main : Unit ->{} Unit\n\
        let main () = need_without (Ref.make say)\n";
    assert_eq!(
        t003_message(without),
        "expected `Ref (Unit ->{-Host.Console | _} Unit)`, found `Ref (Unit ->{Host.Console} Unit)`"
    );
}

/// Multi-module project: qualified import, aliased import, exposing import,
/// and re-export through a facade module all resolve after applying the fix.
#[test]
fn a_callback_using_a_project_module_service_names_it_as_imported() {
    let logging = "module App.Logging exposing (Log, say)\n\n\
        service Log =\n  info : String ->{} Unit\n\n\
        say : Unit ->{Log} Unit\n\
        let say () = Log.info \"hi\"\n";
    let facade = "module App.Facade exposing (Log, say)\n\n\
        import App.Logging exposing (Log, say)\n";

    let cases = [
        ("import App.Logging\n", "App.Logging.Log", "App.Logging.say"),
        ("import App.Logging as L\n", "L.Log", "L.say"),
        ("import App.Logging exposing (Log, say)\n", "Log", "say"),
        ("import App.Facade\n", "App.Facade.Log", "App.Facade.say"),
    ];

    for (import_line, service_name, callee) in cases {
        let main = format!(
            "module App.Main exposing (main)\n\n\
             {import_line}\n\
             main : Unit ->{{{service_name}}} Unit\n\
             let main () =\n  \
               let run = fun action -> action ()\n  \
               run {callee}\n"
        );
        let diags = check_modules(&[
            ("App.Logging", "src/App/Logging.fl", logging),
            ("App.Facade", "src/App/Facade.fl", facade),
            ("App.Main", "src/App/Main.fl", &main),
        ]);
        let t003: Vec<_> = diags.iter().filter(|d| d.code == codes::T003).collect();
        assert_eq!(t003.len(), 1, "{import_line}: {diags:?}");
        assert_eq!(
            t003[0].message,
            format!("expected `Unit -> _`, found `Unit ->{{{service_name}}} Unit`"),
            "{import_line}"
        );
        let edits: Vec<_> = t003[0].fixes.iter().flat_map(|f| &f.edits).collect();
        assert_eq!(edits.len(), 1, "{import_line}: {:?}", t003[0].fixes);
        let mut fixed_main = main.clone();
        fixed_main.replace_range(edits[0].span.range(), &edits[0].replacement);
        assert!(
            fixed_main.contains(&format!(
                "fun (action : Unit ->{{{service_name}}} Unit) -> action ()"
            )),
            "{import_line}: {fixed_main}"
        );
        let after_diags: Vec<_> = check_modules(&[
            ("App.Logging", "src/App/Logging.fl", logging),
            ("App.Facade", "src/App/Facade.fl", facade),
            ("App.Main", "src/App/Main.fl", &fixed_main),
        ])
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
        assert!(
            after_diags.is_empty(),
            "{import_line}: fixed code had errors: {after_diags:?}\n{fixed_main}"
        );
    }
}
