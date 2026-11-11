use std::sync::Arc;

use flick_plugin::{Arena, Block, Piece};

use crate::diagnostics::codes;
use crate::fmt::format_with;
use crate::plugin::source::BlockSource;
use crate::plugin::{Expander, Failure};
use crate::source::SourceMap;

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

#[test]
fn a_call_in_a_ui_body_keeps_its_arguments_on_its_line() {
    // broken over lines, its arguments would be its children
    let long = "module App.V exposing (view)\n\nview : String -> String -> String -> Ui\nlet view first second third = ui {\n  text (String.join \", \" [first, second, third])\n  classed \"a class name that is long enough to pass the width of a line\" (text first) (text second)\n}\n";
    assert_eq!(super::fmt(long), long);
}
