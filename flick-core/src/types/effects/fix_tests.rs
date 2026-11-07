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
        "f : String ->{Log;} Unit"
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
         wait : Unit ->{Async;} Unit\n\
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
        "f : String ->{Scope s;} Unit"
    );
    assert_eq!(
        fixed("f : String ->{; NotFound} Unit", program),
        "f : String ->{Scope s; NotFound} Unit"
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
