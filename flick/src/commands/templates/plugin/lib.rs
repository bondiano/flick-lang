//! The plugin of NAME: `tagged "name" { value }` stands for the record
//! `{ tag = "name"; value = value }`, a `Tagged.Tagged`.

use flick_plugin::{Arena, Block, Builder, Compiler, Diagnostic, Plugin, TokenKind, flick};

pub struct Tagged;

#[cfg(feature = "guest")]
flick_plugin::export!(Tagged);

impl Plugin for Tagged {
    fn collect(&self, _: &[Block]) -> Vec<u8> {
        Vec::new()
    }

    fn expand(
        &self,
        block: &Block,
        _: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>> {
        let tag = match block.arguments() {
            [t] if t.kind == TokenKind::String => t.span,
            _ => {
                let message = "`tagged` takes its tag: `tagged \"name\" { value }`";
                return Err(vec![Diagnostic::error(message, block.whole()).with_code("T001")]);
            }
        };
        let mut body = block.body();
        let value = body.expr(compiler).map_err(|d| vec![d])?;
        if let Some(more) = body.next() {
            let message = "a `tagged` block holds one expression";
            return Err(vec![Diagnostic::error(message, more.span).with_code("T002")]);
        }
        let mut b = Builder::new(block.whole());
        let record = flick!(b, { tag = #tag; value = #value });
        Ok(b.finish(record))
    }
}

#[cfg(test)]
mod tests {
    use flick_core::plugin::testing::expand;

    use super::Tagged;

    /// What the plugin makes of the block `block`.
    fn one(block: &str) -> Result<String, Vec<String>> {
        let source = format!("module Main exposing (x)\n\nimport Tagged exposing (plugin tagged)\n\nlet x = {block}\n");
        expand(&Tagged, &source).remove(0)
    }

    #[test]
    fn a_block_is_its_record() {
        assert_eq!(
            one("tagged \"a\" { 1.0 + 2.0 }"),
            Ok("{ tag = \"a\"; value = (1.0 + 2.0) }".to_owned())
        );
    }

    #[test]
    fn a_block_without_its_tag_is_an_error() {
        let errors = one("tagged { 1.0 }").expect_err("an error");
        assert!(errors[0].starts_with("T001 on the block"), "{errors:?}");
    }
}
