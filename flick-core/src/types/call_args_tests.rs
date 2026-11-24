use crate::diagnostics::codes;
use crate::types::lint_tests::{HEAD, check};

/// Applied where it is written, the function takes the type of its
/// parameter from its argument: the callback is the runtime function given.
#[test]
fn a_callback_applied_where_it_is_written_takes_the_argument_type() {
    let head = "service Log =\n  info : String ->{} Unit\n\nsay : Unit ->{Log} Unit\nlet say () = Log.info \"hi\"\n\n";
    for body in [
        "apply : Unit ->{Log} Unit\nlet apply () = (fun h -> h ()) say\n",
        "piped : Unit ->{Log} Unit\nlet piped () = say |> fun h -> h ()\n",
        "both : Unit ->{Log} Unit\nlet both () = (fun h _ -> h ()) say 1\n",
    ] {
        let diagnostics = check(&format!("{HEAD}{head}{body}"));
        assert!(diagnostics.is_empty(), "{body}: {diagnostics:?}");
    }
    // the effects of the callback are the call's: a signature must allow them
    let pure = format!("{HEAD}{head}pure : Unit -> Unit\nlet pure () = (fun h -> h ()) say\n");
    let diagnostics = check(&pure);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == codes::E001 || d.code == codes::E002),
        "{diagnostics:?}"
    );
}

/// A generic local of the same shape does not take the type of `k`: applied
/// where it is written, the `fun` takes it from `say`.
#[test]
fn a_callback_applied_where_it_is_written_ignores_a_generic_local() {
    let say = "service Log =\n  info : String ->{} Unit\n\nsay : Unit ->{Log} Unit\nlet say () = Log.info \"hi\"\n\n";
    let body = "apply : Unit ->{Log} Unit\nlet apply () =\n  let unrelated h = h ()\n  unrelated (fun () -> ())\n  (fun k -> k ()) say\n";
    let diagnostics = check(&format!("{HEAD}{say}{body}"));
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}
