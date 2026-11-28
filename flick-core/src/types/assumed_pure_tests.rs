use crate::types::ForeignBackendSpec;
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
            foreign_backend: ForeignBackendSpec::default_js(),
            validate_foreign_metadata: true,
            native_overrides: None,
            plugin_attributes: &[],
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
    let before = format!("{head}let apply () =\n  let run = fun h -> h ()\n  run say\n");
    assert_eq!(
        t003_message(&format!("{HEAD}{before}")),
        "expected `Unit -> _`, found `Unit ->{Log} Unit`"
    );
    assert_fix(
        &before,
        "FLICK-T003",
        &format!(
            "{head}let apply () =\n  let run = fun (h : Unit ->{{Log}} Unit) -> h ()\n  run say\n"
        ),
    );
}

const SAY: &str = "service Log =\n  info : String ->{} Unit\n\nsay : Unit ->{Log} Unit\nlet say () = Log.info \"hi\"\n\napply : Unit ->{Log} Unit\nlet apply () =\n";

/// The `FLICK-T003` edit annotates the specified lambda parameter only and
/// the annotated program checks again.
fn assert_annotated(body: &str, name: &str) {
    let before = format!("{SAY}{body}");
    let from = format!("fun {name} ->");
    assert_eq!(before.matches(&from).count(), 1, "{before}");
    let after = before.replace(&from, &format!("fun ({name} : Unit ->{{Log}} Unit) ->"));
    assert_fix(&before, "FLICK-T003", &after);
}

/// The note identifies the binder of the applied callback. A named
/// function's header cannot carry a parameter annotation (FLICK-T005).
fn assert_header_callback(body: &str, name: &str) {
    let text = format!("{HEAD}{body}");
    let diagnostics = check(&text);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == codes::T003)
        .collect();
    let [error] = &errors[..] else {
        panic!("{diagnostics:?}")
    };
    let note = error
        .related
        .iter()
        .find(|r| r.message.contains("took it for a pure function"))
        .unwrap_or_else(|| panic!("{error:?}"));
    assert_eq!(
        &text[note.span.start as usize..note.span.end as usize],
        name
    );
    assert!(
        error
            .fixes
            .iter()
            .all(|fix| !fix.message.starts_with("annotate")),
        "{error:?}"
    );
}

#[test]
fn a_callback_of_a_named_local_function_is_explained_at_its_header() {
    assert_header_callback(
        &format!(
            "{SAY}  let call f x = f x\n  let apply_it h = h ()\n  call (fun x -> x) ()\n  apply_it say\n"
        ),
        "h",
    );
    for applied in ["run_it say", "say |> run_it"] {
        assert_header_callback(
            &format!(
                "{SAY}  let apply_it h = h ()\n  let run_it g = g ()\n  apply_it (fun () -> ())\n  {applied}\n"
            ),
            "g",
        );
    }
}

/// Both a returned lambda and a local lambda literal may be parenthesized.
#[test]
fn a_parenthesized_callback_is_annotated_at_the_function_applied() {
    for body in [
        "  let run () = (fun h -> h ())\n  run () say\n",
        "  let run () = (fun h -> h ())\n  (run ()) say\n",
        "  let run () = (fun h -> h ())\n  say |> run ()\n",
        "  let run = (fun h -> h ())\n  run say\n",
        "  let run = fun () -> (fun h -> h ())\n  run () say\n",
        "  (fun () -> (fun h -> h ())) () say\n",
    ] {
        assert_annotated(body, "h");
    }
}

/// An arrow taking a record is matched as a whole, not by its fields.
#[test]
fn a_callback_taking_a_record_is_annotated_at_the_function_applied() {
    let text = SAY.replace(
        "apply : ",
        "log_record : { a : Int } ->{Log} Unit\nlet log_record r = Log.info (show r.a)\n\napply : ",
    );
    let body = "  let first k = k { a = 2 }\n  first (fun _ -> ())\n  let with_record = (fun h -> h { a = 1 })\n  with_record log_record\n";
    let before = format!("{text}{body}");
    let after = before.replace("fun h ->", "fun (h : { a : Int } ->{Log} Unit) ->");
    assert_fix(&before, "FLICK-T003", &after);
}

/// Applied through a function that is not a local, the arrow taken for a
/// callback is still found when it is the one expected; a local whose
/// arrow is generic since is not.
#[test]
fn a_callback_applied_through_another_function_is_annotated() {
    let unrelated = "  let unrelated h = h ()\n  unrelated (fun () -> ())\n";
    assert_annotated(
        &format!("{unrelated}  List.each_effect (fun k -> k ()) [say]\n"),
        "k",
    );
    // `List.map` takes a pure function, so only the text of its fix is checked
    let mapped = format!("{HEAD}{SAY}{unrelated}  ignore (List.map (fun k -> k ()) [say])\n");
    assert_eq!(
        fixed(&mapped, "FLICK-T003"),
        Some(mapped.replace("fun k ->", "fun (k : Unit ->{Log} Unit) ->"))
    );
}

/// A diagnostic inferred through an external callee leaves the assumption
/// available for a later direct call of the local.
#[test]
fn a_callback_note_can_be_reused_after_matching_its_arrow() {
    let text = format!(
        "{HEAD}{}  let run = fun h -> if h () then ()\n  List.each_effect run [say]\n  run say\n",
        SAY.replace("->{Log} Unit\nlet say", "->{Log} Bool\nlet say")
            .replace("info : String ->{} Unit", "info : String ->{} Bool")
    );
    let diagnostics = check(&text);
    assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    for diagnostic in &diagnostics {
        assert_eq!(diagnostic.code, codes::T003, "{diagnostic:?}");
        let fixing = diagnostic
            .fixes
            .iter()
            .find(|fix| fix.message.starts_with("annotate `h`"))
            .unwrap_or_else(|| panic!("{diagnostic:?}"));
        let [edit] = &fixing.edits[..] else {
            panic!("{fixing:?}");
        };
        let mut fixed = text.clone();
        fixed.replace_range(
            edit.span.start as usize..edit.span.end as usize,
            &edit.replacement,
        );
        assert_eq!(
            fixed,
            text.replace("fun h ->", "fun (h : Unit ->{Log} Bool) ->")
        );
        assert!(check(&fixed).is_empty(), "{fixed}");
    }
}

/// A later argument to a signed function has no inferred binder to annotate.
#[test]
fn a_callback_fix_does_not_reuse_an_unrelated_inferred_parameter() {
    let text = "service Log =
  info : String ->{} Bool

say : Unit ->{Log} Bool
let say () = Log.info \"hi\"

explicit : (Unit -> Bool) -> Unit
let explicit h = if h () then ()

apply : Unit ->{Log} Unit
let apply () =
  let inferred = (fun f -> if f () then ())
  inferred say
  explicit say
";
    let diagnostics = check(&format!("{HEAD}{text}"));
    let [inferred, explicit] = &diagnostics[..] else {
        panic!("{diagnostics:?}");
    };
    assert_eq!(inferred.code, codes::T003, "{inferred:?}");
    assert_eq!(explicit.code, codes::T003, "{explicit:?}");
    assert!(
        inferred
            .fixes
            .iter()
            .any(|fix| fix.message.starts_with("annotate `f`")),
        "{inferred:?}"
    );
    assert!(
        explicit
            .fixes
            .iter()
            .all(|fix| !fix.message.starts_with("annotate")),
        "{explicit:?}"
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
