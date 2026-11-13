//! `validate { positive expression }` builds a typed, pure validation.
use flick_plugin::{
    Arena, Block, Builder, Compiler, Completion, Diagnostic, Fact, Hover, Plugin, Span, flick,
};

pub struct Validate;

#[cfg(feature = "guest")]
flick_plugin::export!(Validate);

fn keyword(block: &Block) -> Result<Span, Vec<Diagnostic>> {
    let token = block
        .tokens
        .iter()
        .find(|token| token.span.start >= block.body);
    match token {
        Some(token) if block.text_of(token.span) == "positive" => Ok(token.span),
        _ => Err(vec![
            Diagnostic::error(
                "expected `positive` followed by an Int expression",
                token.map_or(block.whole(), |t| t.span),
            )
            .with_code("V001"),
        ]),
    }
}

impl Plugin for Validate {
    fn collect(&self, _: &[Block], _: &mut dyn Compiler) -> Vec<u8> {
        Vec::new()
    }

    fn expand(
        &self,
        block: &Block,
        _: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>> {
        let word = keyword(block)?;
        let hole = compiler
            .parse_expr(word.end, block.whole().end)
            .map_err(|error| vec![error])?;
        if block
            .tokens
            .iter()
            .any(|token| token.span.start >= hole.span.end && block.text_of(token.span) != "}")
        {
            return Err(vec![
                Diagnostic::error("expected one expression", block.whole()).with_code("V001"),
            ]);
        }
        let mut b = Builder::new(block.whole());
        let hole = hole.span;
        let expression = flick!(b, PositiveDsl.positive #hole);
        Ok(b.finish(expression))
    }

    fn complete(
        &self,
        _: &Block,
        _: &[u8],
        _: u32,
        member: bool,
        _: &mut dyn Compiler,
    ) -> Vec<Completion> {
        if member {
            return Vec::new();
        }
        vec![Completion {
            label: "positive".into(),
            detail: Some("Int -> Result ValidationError Int".into()),
            ..Completion::default()
        }]
    }

    fn hover(&self, block: &Block, _: &[u8], offset: u32, _: &[Fact]) -> Option<Hover> {
        let span = keyword(block).ok()?;
        (span.start <= offset && offset <= span.end).then(|| Hover {
            span,
            markdown: "`positive`: an Int greater than zero, or `NotPositive`. No I/O.".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flick_plugin::{Place, Token, TokenKind};

    fn block(word: &str) -> Block {
        Block {
            head: "validate".into(),
            text: format!("{word} 42 }}"),
            body: 0,
            place: Place::Expression,
            tokens: vec![Token {
                kind: TokenKind::Ident,
                span: Span {
                    start: 0,
                    end: u32::try_from(word.len()).expect("small keyword"),
                },
            }],
            docs: String::new(),
            signature: None,
            module: "Main".into(),
        }
    }
    struct NoParsing;
    impl Compiler for NoParsing {
        fn parse_expr(&mut self, _: u32, _: u32) -> Result<flick_plugin::Hole, Diagnostic> {
            panic!("completion must not parse")
        }
        fn parse_type(&mut self, _: u32, _: u32) -> Result<flick_plugin::Hole, Diagnostic> {
            panic!("completion must not parse")
        }
        fn parse_pat(&mut self, _: u32, _: u32) -> Result<flick_plugin::Hole, Diagnostic> {
            panic!("completion must not parse")
        }
        fn parse_decl(&mut self, _: u32, _: u32) -> Result<flick_plugin::Hole, Diagnostic> {
            panic!("completion must not parse")
        }
    }

    #[test]
    fn completion_offers_the_rule_but_not_for_a_member_request() {
        let block = block("positive");
        let choices = Validate.complete(&block, &[], 0, false, &mut NoParsing);
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].label, "positive");
        assert_eq!(
            choices[0].detail.as_deref(),
            Some("Int -> Result ValidationError Int")
        );
        assert!(
            Validate
                .complete(&block, &[], 0, true, &mut NoParsing)
                .is_empty()
        );
    }

    #[test]
    fn keyword_rejection_and_hover_keep_the_user_span() {
        assert_eq!(keyword(&block("positive")), Ok(Span { start: 0, end: 8 }));
        let errors = keyword(&block("negative")).expect_err("not our grammar");
        assert_eq!(errors[0].code.as_deref(), Some("V001"));
        assert_eq!(errors[0].span, Span { start: 0, end: 8 });
        let hover = Validate
            .hover(&block("positive"), &[], 2, &[])
            .expect("keyword hover");
        assert_eq!(hover.span, Span { start: 0, end: 8 });
        assert!(Validate.hover(&block("positive"), &[], 10, &[]).is_none());
        assert!(Validate.hover(&block("negative"), &[], 2, &[]).is_none());
    }
}
