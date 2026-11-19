use std::sync::Arc;

use flick_plugin::{Arena, Block, Piece};

use crate::diagnostics::codes;
use crate::fmt::format_with;
use crate::fmt::plugins::heads;
use crate::fmt::program::{Lines, program};
use crate::plugin::source::BlockSource;
use crate::plugin::{Expander, Failure};
use crate::source::SourceMap;
use crate::syntax::ast::Ast;
use crate::syntax::parse;

/// A plugin that fails to lay out any block.
struct Broken;

impl Expander for Broken {
    fn id(&self) -> &'static str {
        "broken"
    }

    fn collect(&self, _: &[Block], _: &mut BlockSource) -> Result<Vec<u8>, String> {
        Ok(Vec::new())
    }

    fn publish(&self, _: &[u8]) -> Result<Vec<flick_plugin::Published>, String> {
        Ok(Vec::new())
    }

    fn expand(&self, _: &Block, _: &[u8], _: &mut BlockSource) -> Result<Arena, Failure> {
        Err(Failure::Broken("not here".into()))
    }

    fn format(
        &self,
        _: &Block,
        _: &[u8],
        _: &mut BlockSource,
    ) -> Result<Option<Vec<Piece>>, String> {
        Err("it trapped".into())
    }
}

#[test]
fn a_plugin_that_fails_to_lay_out_a_block_leaves_the_file_and_says_so() {
    let text = "module App.Main exposing (x)\n\nimport Other exposing (plugin broken)\n\nx : Int\nlet   x = broken {   a }\n";
    let mut sources = SourceMap::default();
    let file = sources.add("App/Main.fl", text);
    let plugin: Arc<dyn Expander> = Arc::new(Broken);
    let found = format_with(text, file, &|head| {
        (head == "broken").then(|| Arc::clone(&plugin))
    });
    let failed = found.expect_err("the plugin failed");
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert_eq!(failed[0].code, codes::P036);
    assert!(
        failed[0].message.contains("it trapped"),
        "{}",
        failed[0].message
    );
    assert_eq!(
        &text[failed[0].span.expect("a span").range()],
        "broken {   a }"
    );
}

#[test]
fn the_children_of_a_ui_element_keep_their_lines_and_their_comments() {
    let messy = "module App.V exposing (view)\n\nview : Float -> Ui\nlet view n = ui {\n      let shown = show n\n      div   { class_name = \"counter\" }\n          // the title\n          heading   (text \"count \\{shown}\")   // once\n          paragraph\n                text   \"it is \\{shown}\"\n\n                text \"today\"\n          div { class_name = \"spacer\" }\n}\n";
    let tidy = "module App.V exposing (view)\n\nview : Float -> Ui\nlet view n = ui {\n  let shown = show n\n  div { class_name = \"counter\" }\n    // the title\n    heading (text \"count \\{shown}\") // once\n    paragraph\n      text \"it is \\{shown}\"\n\n      text \"today\"\n    div { class_name = \"spacer\" }\n}\n";
    assert_eq!(super::fmt(messy), tidy);
    assert_eq!(super::fmt(tidy), tidy);
}

/// A module whose `view` is a `ui` block of these lines.
fn ui(lines: &str) -> String {
    format!("module App.V exposing (view)\n\nview : Page -> Ui\nlet view page = ui {{\n{lines}}}\n")
}

#[test]
fn a_call_in_a_ui_body_that_does_not_fit_puts_its_calls_on_lines_under_it() {
    let long = ui(concat!(
        "  text (String.join \", \" [page.first, page.second])\n",
        "  classed \"a class name that is long enough to pass the width of a line\" (text page.first) (text page.second)\n",
    ));
    let tidy = ui(concat!(
        "  text (String.join \", \" [page.first, page.second])\n",
        "  classed \"a class name that is long enough to pass the width of a line\"\n",
        "    text page.first\n",
        "    text page.second\n",
    ));
    assert_eq!(super::fmt(&long), tidy);
}

#[test]
fn a_tree_in_parentheses_becomes_lines() {
    let nested = ui(concat!(
        "  div { class_name = \"auth-page\" }\n",
        "    div { class_name = \"container page\" }\n",
        "      (div\n",
        "        { class_name = \"row\" }\n",
        "        (div\n",
        "          { class_name = \"col-md-6 offset-md-3 col-xs-12\" }\n",
        "          (View.classed \"text-xs-center\" (heading_at H1 (text page.heading)))\n",
        "          (View.classed \"text-xs-center\" (paragraph (page.other)))\n",
        "          (render View.errors ())\n",
        "          (form { on_submit = page.submit } (props.fields ++ [send]))))\n",
    ));
    let flat = ui(concat!(
        "  div { class_name = \"auth-page\" }\n",
        "    div { class_name = \"container page\" }\n",
        "      div { class_name = \"row\" }\n",
        "        div { class_name = \"col-md-6 offset-md-3 col-xs-12\" }\n",
        "          View.classed \"text-xs-center\" (heading_at H1 (text page.heading))\n",
        "          View.classed \"text-xs-center\" (paragraph (page.other))\n",
        "          render View.errors ()\n",
        "          form { on_submit = page.submit } (props.fields ++ [send])\n",
    ));
    assert_eq!(super::fmt(&nested), flat);
}

/// The program `text` means, as the formatter compares it.
fn meaning(text: &str) -> Ast {
    let mut sources = SourceMap::default();
    let file = sources.add("App/V.fl", text);
    let tree = parse(text, file).tree;
    let heads = heads(&tree, text, file);
    let heads: Vec<&str> = heads.iter().map(String::as_str).collect();
    let bodies = ["ui"];
    let lines = Lines {
        file,
        heads: &heads,
        bodies: &bodies,
    };
    program(&tree, text, &lines)
}

#[test]
fn a_line_under_a_call_means_its_argument_in_parentheses() {
    let nested = meaning(&ui(
        "  div {} (p {} (text \"a\")) (span {} (text page.b))\n",
    ));
    let lines = meaning(&ui(
        "  div {}\n    p {}\n      text \"a\"\n    span {} (text page.b)\n",
    ));
    assert_eq!(nested, lines);
    // in a component's body too, which is the tokens of its block
    let component = |body: &str| meaning(&format!("component counter () = ui {{\n{body}}}\n"));
    assert_eq!(
        component("  div {} (text \"a\")\n"),
        component("  div {}\n    text \"a\"\n")
    );
    // the same tokens on one line are other arguments
    assert_ne!(
        meaning(&ui("  div {} p {} text \"a\"\n")),
        meaning(&ui("  div {}\n    p {}\n      text \"a\"\n"))
    );
    assert_ne!(meaning("let y = f (g x)\n"), meaning("let y = f g x\n"));
}

#[test]
fn a_call_with_lines_under_it_puts_its_calls_before_them() {
    let pad = "a class name that is long enough to pass the width of a line";
    let text = ui(&format!(
        "  div {{ class_name = \"{pad}\" }} (text page.first)\n    text page.second\n"
    ));
    let tidy = ui(&format!(
        "  div {{ class_name = \"{pad}\" }}\n    text page.first\n    text page.second\n"
    ));
    assert_eq!(super::fmt(&text), tidy);
}

#[test]
fn a_call_in_parentheses_on_its_line_keeps_them_where_it_must_break() {
    let pad = "a class name that is long enough to pass the width of a line";
    let kept = [
        // without them its arguments could not break
        format!("  div {{}}\n    (classed \"{pad}\" (text page.first) page.second)\n"),
        // a comment in them
        "  div {}\n    (text /* the first */ page.first)\n".to_owned(),
    ];
    for text in kept {
        let text = ui(&text);
        let once = super::fmt(&text);
        assert_eq!(
            once.matches('(').count(),
            text.matches('(').count(),
            "{once}"
        );
    }
    let bare = ui("  div {}\n    (text page.first) // the first\n");
    assert_eq!(
        super::fmt(&bare),
        ui("  div {}\n    text page.first // the first\n")
    );
}

#[test]
fn what_fits_or_cannot_be_a_line_keeps_its_parentheses() {
    let pad = "a class name that is long enough to pass the width of a line";
    let kept = [
        // it fits
        "  div { class_name = \"row\" } (text page.first) (text page.second)\n".to_owned(),
        // a comment among them
        format!("  classed \"{pad}\" (text page.first) /* first */ (text page.second)\n"),
        // a line starting with `-` would go on with the one before it
        format!("  classed \"{pad}\" (show (-page.count)) (-page.count)\n"),
        // `if` would take the lines after it
        format!("  classed \"{pad}\" (if page.on then text \"on\" else text \"off\")\n"),
        // inside brackets a line break means nothing
        format!("  ul {{}} [li {{}} (text \"{pad}\"), li {{}} (text page.second)]\n"),
    ];
    for text in kept {
        let text = ui(&text);
        let once = super::fmt(&text);
        assert_eq!(
            once.matches('(').count(),
            text.matches('(').count(),
            "{once}"
        );
    }
}

#[test]
fn a_refined_type_is_laid_out_by_its_plugin() {
    let messy = "module App.Ports exposing (..)\n\n/// A port of TCP.\nrefined   type Port=Float   where fun n->n>0&&n<65536 deriving Eq,Hash\n\nrefined type Tagged a = List a\n  where fun items -> not (List.is_empty items)\n";
    let tidy = "module App.Ports exposing (..)\n\n/// A port of TCP.\nrefined type Port = Float where fun n -> n > 0 && n < 65536\n  deriving Eq, Hash\n\nrefined type Tagged a = List a where fun items -> not (List.is_empty items)\n";
    assert_eq!(super::fmt(messy), tidy);
    assert_eq!(super::fmt(tidy), tidy);
}

#[test]
fn a_hook_lays_out_as_a_case_without_a_name_and_a_timeout_after_the_name() {
    let messy = "module App.DbTest\n\nsuite \"Db\"   timeout   2000 {\n    before_all {   Platform.stdout_line \"open\" }\n\n  before_each {\n       Platform.stdout_line \"begin\"\n  }\n  test \"reads\"  timeout 50   {}\n  after_all {}\n}\n";
    let tidy = "module App.DbTest\n\nsuite \"Db\" timeout 2000 {\n  before_all { Platform.stdout_line \"open\" }\n\n  before_each {\n    Platform.stdout_line \"begin\"\n  }\n  test \"reads\" timeout 50 {}\n  after_all {}\n}\n";
    assert_eq!(super::fmt(messy), tidy);
    assert_eq!(super::fmt(tidy), tidy);
}

#[test]
fn a_suite_lays_out_a_case_a_line_and_a_modifier_after_its_head() {
    let messy = "module App.UsersTest\n\nsuite.skip   \"Users\"   {\n  /// the common case\n    test \"trims\" {   Test.expect (Test.equal 1 1) }\n\n  test.only \"keeps\" {\n       let a = 1\n       Test.expect (Test.equal a 1)\n  }\n  suite \"empty\" { test \"todo\" {} }\n}\n\ntest   \"alone\" {   Test.expect (Test.equal 2 2) }\n\ntest \"lines\" {\n      Test.expect (Test.equal 3 3)\n}\n";
    let tidy = "module App.UsersTest\n\nsuite.skip \"Users\" {\n  /// the common case\n  test \"trims\" { Test.expect (Test.equal 1 1) }\n\n  test.only \"keeps\" {\n    let a = 1\n    Test.expect (Test.equal a 1)\n  }\n  suite \"empty\" {\n    test \"todo\" {}\n  }\n}\n\ntest \"alone\" { Test.expect (Test.equal 2 2) }\n\ntest \"lines\" {\n  Test.expect (Test.equal 3 3)\n}\n";
    assert_eq!(super::fmt(messy), tidy);
    assert_eq!(super::fmt(tidy), tidy);
}
