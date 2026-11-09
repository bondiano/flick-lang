//! The fix of an E002 adds what a written row lacks right after its `{`.

use std::cmp::Reverse;

use crate::diagnostics::codes;
use crate::source::SourceMap;

/// `signature` after the fix of its E002, applied to the text of `program`.
fn fixed(signature: &str, program: &str) -> String {
    let text = format!(
        "module Probe exposing (..)\n\
         type error NotFound = NotFound Float\n\
         service Users =\n  get : Float ->{{; NotFound}} String\n\
         service Log =\n  line : String ->{{}} Unit\n\
         {signature}\n{program}\n"
    );
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text.clone());
    let diagnostics = crate::check(&sources, file);
    let edits: Vec<_> = (diagnostics.iter())
        .filter(|d| [codes::E002, codes::T003, codes::G004].contains(&d.code))
        .flat_map(|d| &d.fixes)
        .flat_map(|f| &f.edits)
        .collect();
    let [edit] = edits[..] else {
        panic!("expected one edit, got {diagnostics:?}");
    };
    let mut out = text;
    out.replace_range(edit.span.range(), &edit.replacement);
    let line = out
        .lines()
        .find(|l| l.starts_with("f :"))
        .expect("the signature");
    line.to_owned()
}

#[test]
fn the_fix_adds_the_service_after_the_brace() {
    let program = "let f x = Log.line x";
    assert_eq!(
        fixed("f : String ->{} Unit", program),
        "f : String ->{Log} Unit"
    );
    let program = "let f x = Log.line x";
    assert_eq!(
        fixed("f : String ->{; NotFound} Unit", program),
        "f : String ->{Log; NotFound} Unit"
    );
    let program = "let f x =\n  let _ = Users.get 1.0\n  Log.line x";
    assert_eq!(
        fixed("f : String ->{Users; NotFound} Unit", program),
        "f : String ->{Log, Users; NotFound} Unit"
    );
}

#[test]
fn a_row_of_a_variable_gets_the_service_before_it() {
    let program = "let f x = Log.line x";
    assert_eq!(
        fixed("f : forall r. String ->{r} Unit", program),
        "f : forall r. String ->{Log | r} Unit"
    );
}

#[test]
fn adding_async_alone_is_safe() {
    let text = "module Probe exposing (..)\n\
                wait : Unit ->{Async;} Unit\n\
                foreign effect wait from \"./wait.mjs\" \"wait\"\n\
                f : Unit ->{} Unit\n\
                let f () =\n  wait ()\n  wait ()\n";
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text.to_owned());
    let diagnostics = crate::check(&sources, file);
    let fixes: Vec<_> = (diagnostics.iter())
        .filter(|d| d.code == codes::E002)
        .flat_map(|d| &d.fixes)
        .collect();
    assert!(matches!(fixes[..], [fix] if fix.safe), "{diagnostics:?}");
}

#[test]
fn the_services_of_a_written_eff_get_it_too() {
    let program = "let f () = task { Log.line \"x\" }";
    assert_eq!(
        fixed("f : Unit -> Eff none none Unit", program),
        "f : Unit -> Eff {Log} none Unit"
    );
    let program = "let f () = task {\n  let _ = Users.get 1.0\n  Log.line \"x\"\n}";
    assert_eq!(
        fixed("f : Unit -> Eff {Users} {NotFound} Unit", program),
        "f : Unit -> Eff {Log, Users} {NotFound} Unit"
    );
}

#[test]
fn a_row_given_to_an_alias_gets_it_where_it_is_given() {
    let program = "type alias Handlers r = { run : Unit ->{r;} Unit }\n\
                   let f = { run = fun () -> Log.line \"x\" }";
    assert_eq!(fixed("f : Handlers none", program), "f : Handlers {Log}");
}

#[test]
fn a_set_gets_what_it_lacks_beside_it() {
    let program = "type alias Env = {Users; NotFound}\nlet f x = Log.line x";
    assert_eq!(
        fixed("f : String ->{...Env} Unit", program),
        "f : String ->{Log, ...Env} Unit"
    );
}

/// `signature` after the fix of its T002, in the text of `program`.
fn fixed_t002(signature: &str, program: &str) -> String {
    let mut text = format!(
        "module Probe exposing (..)\n\
         service Log =\n  line : String ->{{}} Unit\n\
         type alias Env = {{Log}}\n\
         {signature}\n{program}\n"
    );
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text.clone());
    let diagnostics = crate::check(&sources, file);
    let mut edits: Vec<_> = (diagnostics.iter())
        .filter(|d| d.code == codes::T002)
        .flat_map(|d| &d.fixes)
        .flat_map(|f| &f.edits)
        .collect();
    assert!(!edits.is_empty(), "{diagnostics:?}");
    edits.sort_by_key(|e| Reverse(e.span.range().start));
    for edit in edits {
        text.replace_range(edit.span.range(), &edit.replacement);
    }
    let line = text.lines().find(|l| l.starts_with("f :"));
    line.expect("the signature").to_owned()
}

#[test]
fn a_set_without_dots_gets_them_and_an_element_loses_them() {
    let program = "let f x = Log.line x";
    // with a set the row needs no `;`, unless errors follow it
    assert_eq!(
        fixed_t002("f : String ->{Env;} Unit", program),
        "f : String ->{...Env} Unit"
    );
    assert_eq!(
        fixed_t002("f : String ->{Log, Env ; } Unit", program),
        "f : String ->{Log, ...Env} Unit"
    );
    assert_eq!(
        fixed_t002(
            "f : String ->{Env; Boom} Unit",
            "type error Boom = Boom\nlet f x = Log.line x"
        ),
        "f : String ->{...Env; Boom} Unit"
    );
    assert_eq!(
        fixed_t002("f : String ->{...Env, -Env} Unit", program),
        "f : String ->{...Env, -...Env} Unit"
    );
}

#[test]
fn an_arrow_without_its_only_set_needs_no_semicolon() {
    let program = "let f x = Log.line x";
    assert_eq!(
        fixed_t002("f : String ->{...Log} Unit", program),
        "f : String ->{Log} Unit"
    );
    assert_eq!(
        fixed_t002(
            "f : String ->{...Log; Boom} Unit",
            "type error Boom = Boom\nlet f x = Log.line x"
        ),
        "f : String ->{Log; Boom} Unit"
    );
    // another set stays, so the row needs no `;`
    assert_eq!(
        fixed_t002("f : String ->{...Log, ...Env} Unit", program),
        "f : String ->{Log, ...Env} Unit"
    );
}

/// The edits of the E002 of `signature`, in the text of `program`.
fn edits_of_e002(signature: &str, program: &str) -> usize {
    let text = format!(
        "module Probe exposing (..)\n\
         service Users =\n  get : Float ->{{}} String\n\
         service Log =\n  line : String ->{{}} Unit\n\
         {signature}\n{program}\n"
    );
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text);
    let diagnostics = crate::check(&sources, file);
    assert!(
        diagnostics.iter().any(|d| d.code == codes::E002),
        "{diagnostics:?}"
    );
    (diagnostics.iter())
        .filter(|d| d.code == codes::E002)
        .flat_map(|d| &d.fixes)
        .map(|f| f.edits.len())
        .sum()
}

#[test]
fn a_row_named_by_its_alias_is_not_edited_through_it() {
    let program = "type alias Env = {Users}\nlet f = task { Log.line \"x\" }";
    assert_eq!(edits_of_e002("f : Eff Env none Unit", program), 0);
}

#[test]
fn a_row_with_omit_gets_no_fix() {
    // adding `Log` beside `-Log` would change nothing
    let program = "type alias Env = {Users, Log}\nlet f x = Log.line x";
    assert_eq!(
        edits_of_e002("f : String ->{...Env, -Log} Unit", program),
        0
    );
}

#[test]
fn a_type_that_lacks_only_async_gets_it_where_it_is_written() {
    let program = "type Job (r : ServiceRow) = Job\n\
                   job : Unit -> Job {Async}\n\
                   let job () = Job\n\
                   let f = job ()";
    assert_eq!(fixed("f : Job none", program), "f : Job {Async}");
}

#[test]
fn the_words_of_before_go_and_a_promise_says_async() {
    let text = "module Probe exposing (..)\n\
                wait : Unit ->{} Unit\n\
                foreign async wait from \"./wait.mjs\" \"wait\"\n\
                tick : Unit ->{} Unit\n\
                foreign sync tick from \"./tick.mjs\" \"tick\"\n";
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text.to_owned());
    let diagnostics = crate::check(&sources, file);
    let mut edits: Vec<_> = (diagnostics.iter())
        .filter(|d| d.code == codes::F001)
        .flat_map(|d| &d.fixes)
        .filter(|f| f.safe)
        .flat_map(|f| &f.edits)
        .collect();
    edits.sort_by_key(|e| Reverse(e.span.start));
    let mut out = text.to_owned();
    for e in edits {
        out.replace_range(e.span.range(), &e.replacement);
    }
    assert_eq!(
        out,
        "module Probe exposing (..)\n\
         wait : Unit ->{Async} Unit\n\
         foreign wait from \"./wait.mjs\" \"wait\"\n\
         tick : Unit ->{} Unit\n\
         foreign tick from \"./tick.mjs\" \"tick\"\n"
    );
}

#[test]
fn a_resource_without_an_owner_asks_the_row_for_its_scope() {
    let program = "let f x = ignore (Resource.acquire_release (task { x }) (fun _ -> ()))";
    assert_eq!(
        fixed("f : String ->{} Unit", program),
        "f : String ->{Scope s} Unit"
    );
    assert_eq!(
        fixed("f : String ->{; NotFound} Unit", program),
        "f : String ->{Scope s; NotFound} Unit"
    );
    // a name the signature uses is not taken for it
    let program = "let f _ x = ignore (Resource.acquire_release (task { x }) (fun _ -> ()))";
    assert_eq!(
        fixed("f : s -> String ->{} Unit", program),
        "f : s -> String ->{Scope s1} Unit"
    );
}

/// `signature` after the edits of the fix of its `Fiber.Provided` G003.
fn provided(signature: &str) -> String {
    let text = format!(
        "module Probe exposing (..)\n\
         service Log =\n  line : String ->{{}} Unit\n\
         {signature}\n\
         let f work = Scope.add_finalizer work\n"
    );
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text.clone());
    let diagnostics = crate::check(&sources, file);
    let mut edits: Vec<_> = (diagnostics.iter())
        .filter(|d| d.code == codes::G003)
        .filter(|d| d.fixes.iter().any(|f| f.message.contains("Fiber.Provided")))
        .flat_map(|d| &d.fixes)
        .flat_map(|f| &f.edits)
        .collect();
    assert!(!edits.is_empty(), "{diagnostics:?}");
    edits.sort_by_key(|e| Reverse(e.span.start));
    let mut out = text;
    for e in edits {
        out.replace_range(e.span.range(), &e.replacement);
    }
    out.lines()
        .find(|l| l.starts_with("f :"))
        .expect("the signature")
        .to_owned()
}

#[test]
fn the_fix_of_g003_adds_the_constraint_to_the_context() {
    let row = "(Eff r none Unit)^c ->{Scope s | r;} Unit";
    assert_eq!(
        provided(&format!("f : (Fiber.Outlives c s, Detached r) => {row}")),
        format!("f : (Fiber.Outlives c s, Detached r, Fiber.Provided r s) => {row}")
    );
    assert_eq!(
        provided(&format!("f : Fiber.Outlives c s => {row}")),
        format!("f : (Fiber.Outlives c s, Fiber.Provided r s) => {row}")
    );
    assert_eq!(
        provided(&format!("f : {row}")),
        format!("f : Fiber.Provided r s => {row}")
    );
}

#[test]
fn the_fix_of_g001_adds_the_region_to_a_written_qualifier() {
    let text = |signature: &str| {
        format!(
            "module Probe exposing (..)\n\
             type Handle (s : Region) = Handle\n\
             {signature}\n\
             let f h = fun () -> ignore h\n"
        )
    };
    let fixed = |signature: &str| {
        let text = text(signature);
        let mut sources = SourceMap::default();
        let file = sources.add("Probe.fl", text.clone());
        let diagnostics = crate::check(&sources, file);
        let edits: Vec<_> = (diagnostics.iter())
            .filter(|d| d.code == codes::G001)
            .flat_map(|d| &d.fixes)
            .flat_map(|f| &f.edits)
            .collect();
        let [edit] = edits[..] else {
            panic!("expected one edit, got {diagnostics:?}");
        };
        let mut out = text;
        out.replace_range(edit.span.range(), &edit.replacement);
        out.lines()
            .find(|l| l.starts_with("f :"))
            .expect("the signature")
            .to_owned()
    };
    assert_eq!(
        fixed("f : Handle s -> (Unit ->{} Unit)^{}"),
        "f : Handle s -> (Unit ->{} Unit)^{s}"
    );
}
