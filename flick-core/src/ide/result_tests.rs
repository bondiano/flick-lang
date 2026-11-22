//! IDE queries (tokens, folding, completions, partial input) on `result { … }`.

use super::tests::fixture_of;

#[test]
fn result_block_tokens_folding_completion_and_partial_input() {
    let source = "module App.Main exposing (run, call_rec)\n\n\
type error Boom = Boom\n\n\
fail_boom : Unit ->{; Boom} Int\n\
let fail_boom () = Fail.fail Boom\n\n\
run : Unit ->{} Result Boom Int\n\
let run () =\n  result {\n    let x = fail_boom ()\n    x + 1\n  }\n\n\
call_rec : ({ field : Int } -> Int) -> Int\n\
let call_rec result = (result) { field = 1 }\n";
    let f = fixture_of(&[("src/App/Main.fl", source)]);
    let ide = f.ide();
    let file = f.file("App.Main");

    // Semantic tokens: `result` in `result { ... }` is `keyword`, whereas `(result)` is `variable`
    let tokens = ide.semantic_tokens(file);
    let kw_at = f.at("App.Main", "result {\n", 0);
    let var_at = f.at("App.Main", "(result)", 0) + 1;
    let kind_at = |at: u32| {
        tokens
            .iter()
            .find(|&&(s, e, _)| s == at && e == at + 6)
            .map(|&(_, _, k)| super::TOKEN_TYPES[k as usize])
    };
    assert_eq!(kind_at(kw_at), Some("keyword"), "{tokens:?}");
    assert_eq!(kind_at(var_at), Some("variable"), "{tokens:?}");

    // Folding range covers multi-line `result { ... }`
    let folds = ide.folding_ranges(file);
    assert!(
        folds.iter().any(|&(s, _)| s == kw_at),
        "expected folding range starting at result block: {folds:?}"
    );

    // Completion inside `result { ... }` includes effectful/failing functions
    let at_call = f.at("App.Main", "fail_boom ()", 0);
    let items = ide.complete(file, at_call + 4, false);
    assert!(
        items.iter().any(|c| c.label == "fail_boom"),
        "expected fail_boom in completions inside result block: {items:?}"
    );

    // Partial/incomplete input must not panic in hover, tokens, actions, or completion
    let partial = "module App.Partial exposing (run)\n\nlet run () = result {\n  let x = ";
    let pf = fixture_of(&[("src/App/Partial.fl", partial)]);
    let pide = pf.ide();
    let pfile = pf.file("App.Partial");
    let at = pf.at("App.Partial", "result {", 0);
    let _ = pide.hover(pfile, at + 1);
    let _ = pide.semantic_tokens(pfile);
    let _ = pide.actions(pfile, at, at + 6);
    let _ = pide.complete(pfile, u32::try_from(partial.len()).expect("len"), false);
}
