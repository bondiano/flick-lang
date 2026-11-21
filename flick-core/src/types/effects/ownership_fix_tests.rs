use crate::diagnostics::codes;
use crate::source::SourceMap;

#[test]
fn e002_never_suggests_editing_another_binding_signature() {
    // Another top-level function's signature (`run_quiet`, `needs_logged`),
    // a sibling in a `rec` group, another top-level cell (`global_cell`), or a
    // local helper's signature must not be offered as an E002 edit when a
    // caller fails a closed-row check.
    let cases = [
        (
            "parameter of another binding",
            "run_quiet : (Unit ->{} Unit) ->{} Unit\n\
             let run_quiet f = f ()\n\
             caller : Unit ->{Log} Unit\n\
             let caller () = run_quiet (fun () -> Log.line \"x\")\n",
        ),
        (
            "global cell binding",
            "global_cell : Ref (Unit ->{} Unit)\n\
             foreign pure global_cell from \"./c.js\" \"c\"\n\
             caller : Unit ->{Log} Unit\n\
             let caller () = Ref.set global_cell (fun () -> Log.line \"x\")\n",
        ),
        (
            "contravariant function from another binding",
            "needs_logged : (Unit ->{Log} Unit) ->{Log} Unit\n\
             let needs_logged f = f ()\n\
             needs_quiet : (Unit ->{} Unit) ->{} Unit\n\
             let needs_quiet f = f ()\n\
             caller : Bool ->{Log} Unit\n\
             let caller b =\n  \
               let g = if b then needs_logged else needs_quiet\n  \
               g (fun () -> Log.line \"x\")\n",
        ),
        (
            "mutual rec group sibling",
            "rec {\n  \
               run_q : (Unit ->{} Unit) ->{} Unit\n  \
               let run_q f = f ()\n  \
               caller : Unit ->{Log} Unit\n  \
               let caller () = run_q (fun () -> Log.line \"x\")\n\
             }\n",
        ),
        (
            "local helper function",
            "caller : Unit ->{Log} Unit\n\
             let caller () =\n  \
               run_q : (Unit ->{} Unit) ->{} Unit\n  \
               let run_q f = f ()\n  \
               run_q (fun () -> Log.line \"x\")\n",
        ),
        (
            "local qualified helper function",
            "caller : Unit ->{Log} Unit\n\
             let caller () =\n  \
               run_q : ((Unit ->{} Unit) ->{} Unit)^{}\n  \
               let run_q = fun f -> f ()\n  \
               run_q (fun () -> Log.line \"x\")\n",
        ),
    ];
    for (name, body) in cases {
        let text = format!(
            "module Probe exposing (..)\n\
             service Log =\n  line : String ->{{}} Unit\n\
             {body}"
        );
        let mut sources = SourceMap::default();
        let file = sources.add("Probe.fl", text);
        let diagnostics = crate::check(&sources, file);
        let e002: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == codes::E002)
            .collect();
        assert!(
            !e002.is_empty(),
            "{name}: expected E002, got {diagnostics:?}"
        );
        let edits: Vec<_> = e002
            .iter()
            .flat_map(|d| &d.fixes)
            .flat_map(|f| &f.edits)
            .collect();
        assert!(
            edits.is_empty(),
            "{name}: E002 must not edit another binding's signature, got {e002:?}"
        );
        let helps: Vec<_> = e002
            .iter()
            .flat_map(|d| &d.fixes)
            .map(|f| f.message.as_str())
            .collect();
        assert!(
            helps.is_empty(),
            "{name}: E002 must not suggest changing another binding's expected type, got {e002:?}"
        );
    }
}

#[test]
fn e002_edits_own_local_cell_or_signature_not_unrelated_quiet_binding() {
    // Local written closed cell: edit targets the local cell signature, not `quiet`.
    let text = "module Probe exposing (..)\n\
                service Log =\n  line : String ->{} Unit\n\
                quiet : Unit ->{} Unit\n\
                let quiet () = ()\n\
                logged : Unit ->{Log} Unit\n\
                let logged () = Log.line \"x\"\n\
                caller : Unit ->{Log} Unit\n\
                let caller () =\n  \
                  cell : Ref (Unit ->{} Unit)\n  \
                  let cell = Ref.make quiet\n  \
                  Ref.set cell logged\n";
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text.to_owned());
    let diagnostics = crate::check(&sources, file);
    let edits: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == codes::E002)
        .flat_map(|d| &d.fixes)
        .flat_map(|f| &f.edits)
        .collect();
    let [edit] = edits[..] else {
        panic!("expected one edit on `cell`, got {diagnostics:?}");
    };
    let mut out = text.to_owned();
    out.replace_range(edit.span.range(), &edit.replacement);
    assert!(
        out.contains("quiet : Unit ->{} Unit"),
        "`quiet` must stay untouched:\n{out}"
    );
    assert!(
        out.contains("cell : Ref (Unit ->{Log} Unit)"),
        "`cell` should receive `Log`:\n{out}"
    );

    // Unannotated `Ref.make quiet` + `Ref.set cell logged` + `(Ref.get cell) ()`
    // in a closed function: edit targets `bad`, not `quiet`.
    let closed_caller = "module Probe exposing (..)\n\
                         service Log =\n  line : String ->{} Unit\n\
                         quiet : Unit ->{} Unit\n\
                         let quiet () = ()\n\
                         logged : Unit ->{Log} Unit\n\
                         let logged () = Log.line \"x\"\n\
                         bad : Unit ->{} Unit\n\
                         let bad () =\n  \
                           let cell = Ref.make quiet\n  \
                           Ref.set cell logged\n  \
                           let f = Ref.get cell\n  \
                           f ()\n";
    let mut caller_sources = SourceMap::default();
    let caller_file = caller_sources.add("Probe.fl", closed_caller.to_owned());
    let caller_diags = crate::check(&caller_sources, caller_file);
    let caller_edits: Vec<_> = caller_diags
        .iter()
        .filter(|d| d.code == codes::E002)
        .flat_map(|d| &d.fixes)
        .flat_map(|f| &f.edits)
        .collect();
    let [caller_edit] = caller_edits[..] else {
        panic!("expected one edit on `bad`, got {caller_diags:?}");
    };
    let mut rewritten = closed_caller.to_owned();
    rewritten.replace_range(caller_edit.span.range(), &caller_edit.replacement);
    assert!(
        rewritten.contains("quiet : Unit ->{} Unit"),
        "`quiet` must stay untouched:\n{rewritten}"
    );
    assert!(
        rewritten.contains("bad : Unit ->{Log} Unit"),
        "`bad` should receive `Log`:\n{rewritten}"
    );
}

#[test]
fn the_fix_of_g003_adds_provided_for_scoped_ref_set() {
    let text = "module Probe exposing (..)\n\
        service Greeter =\n  greet : String ->{} String\n\
        replace : ScopedRef.ScopedRef s String ->{Greeter} Unit\n\
        let replace r =\n  \
          ScopedRef.set\n    \
            (task { Resource.acquire_release (task { \"w\" }) (fun _ -> ignore (Greeter.greet \"release\")) })\n    \
            r\n";
    let mut sources = SourceMap::default();
    let file = sources.add("Probe.fl", text.to_owned());
    let diagnostics = crate::check(&sources, file);
    let g003: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.code == codes::G003)
        .collect();
    assert_eq!(g003.len(), 1, "{diagnostics:?}");
    let edits: Vec<_> = g003[0].fixes.iter().flat_map(|f| &f.edits).collect();
    assert_eq!(edits.len(), 1, "{g003:?}");
    let mut out = text.to_owned();
    out.replace_range(edits[0].span.range(), &edits[0].replacement);
    assert!(
        out.contains(
            "replace : Fiber.Provided {Greeter} s => ScopedRef.ScopedRef s String ->{Greeter} Unit"
        ),
        "{out}"
    );
}
