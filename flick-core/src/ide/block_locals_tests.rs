//! A local the user's code of a block binds: the generated code decides
//! which binder a name sees, so the editor asks the checker.

use super::tests::fixture_of;
use super::{Symbol, Target};

#[test]
fn a_parameter_of_a_server_fn_is_found_and_renamed_with_its_uses() {
    let source = "module App.Api exposing (greet)\n\nimport Server exposing (plugin server)\n\ngreet : String ->{Server.Server;} String\nserver fn greet name = \"Hello, \" ++ name\n";
    let f = fixture_of(&[("src/App/Api.fl", source)]);
    let ide = f.ide();
    let file = f.file("App.Api");
    let at = |needle: &str| {
        let start = u32::try_from(source.find(needle).expect("occurs")).expect("small");
        (start, start + 4)
    };
    let (binder, used) = (at("name ="), at("name\n"));
    let (symbol, range) = ide.symbol_at(file, used.0 + 1).expect("a local");
    assert_eq!(range, used);
    assert!(
        matches!(&symbol, Symbol::Local { binder: b, .. } if *b == binder),
        "{symbol:?}"
    );
    let definition = ide.definition(&symbol).expect("its binder");
    assert_eq!(
        (definition.target, definition.start, definition.end),
        (Target::File(file), binder.0, binder.1)
    );
    let both = vec![(file, binder), (file, used)];
    assert_eq!(ide.references(&symbol), both);
    assert_eq!(
        ide.rename(file, binder.0 + 1, "who").expect("renamed"),
        both
    );
}

#[test]
fn the_render_of_a_component_sees_its_props_and_the_names_of_its_setup() {
    let source =
        "module App.C exposing (c)\n\ncomponent c (p : String) =\n  let n = p\n  ui { text n }\n";
    let f = fixture_of(&[("src/App/C.fl", source)]);
    let ide = f.ide();
    let file = f.file("App.C");
    let m = ide.analysis.by_file(file).expect("the module");
    let offset = u32::try_from(source.rfind('n').expect("a use")).expect("small");
    let names: Vec<String> = super::locals::in_scope(&m.tree, offset, source)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, ["p", "n"]);
}

#[test]
fn a_ui_block_is_code_of_the_user_to_an_editor() {
    let source = "module App.V exposing (view)\n\nview : Ui.View Float\nlet view =\n  Ui.view (fun n -> ui {\n    let shown = show n\n    text shown\n  })\n";
    let f = fixture_of(&[("src/App/V.fl", source)]);
    let ide = f.ide();
    let file = f.file("App.V");
    let found = &ide.analysis.diagnostics;
    assert!(found.is_empty(), "{found:?}");
    let at = |needle: &str| u32::try_from(source.find(needle).expect("occurs")).expect("small");
    // the head: the type of the block, and what its plugin says of it
    let (head, range) = ide.hover(file, at("ui {") + 1).expect("the head");
    assert_eq!(range, (at("ui {"), at("ui {") + 2));
    assert!(head.contains("ui : Ui"), "{head}");
    assert!(head.contains("Describes a user interface"), "{head}");
    // a name of the open module, by what it is
    let (text, _) = ide
        .hover(file, at("text shown") + 1)
        .expect("a name of `Ui`");
    assert!(text.contains("text : String -> Ui"), "{text}");
    // a local of the block
    let (binder, used) = (at("shown ="), at("shown\n"));
    let (symbol, _) = ide.symbol_at(file, used + 1).expect("a local");
    assert!(
        matches!(&symbol, Symbol::Local { binder: b, .. } if *b == (binder, binder + 5)),
        "{symbol:?}"
    );
    let m = ide.analysis.by_file(file).expect("the module");
    let names: Vec<String> = super::locals::in_scope(&m.tree, used, source)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, ["n", "shown"]);
}

#[test]
fn a_field_a_record_pattern_puns_is_a_name_in_scope() {
    let source = "module App.P exposing (f)\n\nf : { x : Int, y : Int } -> Int\nlet f r =\n  let { x; y = other } = r\n  x + other\n";
    let f = fixture_of(&[("src/App/P.fl", source)]);
    let ide = f.ide();
    let file = f.file("App.P");
    let m = ide.analysis.by_file(file).expect("the module");
    let offset = u32::try_from(source.rfind("other").expect("a use")).expect("small");
    let names: Vec<String> = super::locals::in_scope(&m.tree, offset, source)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, ["r", "x", "other"]);
    // and both are parameters to the editor's colours
    let tokens = ide.semantic_tokens(file);
    let pun = u32::try_from(source.find("{ x;").expect("the pun")).expect("small") + 2;
    assert!(
        tokens
            .iter()
            .any(|&(start, _, kind)| start == pun && kind == super::tokens::PARAMETER)
    );
}
