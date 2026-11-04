//! A plugin for the compiler's tests. `pairs { k = e; k : T = e }` stands
//! for the record `{ k = e; k = (e : T); blocks = n }`, where `n` is the
//! number of `pairs` blocks of the group, which `collect` counts. `bad {}`
//! stands for `(1.0 : String)`, an error in generated code.

use flick_plugin::{
    Arena, Block, Builder, Compiler, Diagnostic, Expr, Field, Plugin, Span, Token, TokenKind, Ty,
};

pub struct Pairs;

impl Plugin for Pairs {
    fn collect(&self, blocks: &[Block]) -> Vec<u8> {
        let pairs = blocks.iter().filter(|b| b.head == "pairs").count();
        u32::try_from(pairs)
            .unwrap_or(u32::MAX)
            .to_le_bytes()
            .to_vec()
    }

    fn expand(
        &self,
        block: &Block,
        state: &[u8],
        compiler: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>> {
        let len = u32::try_from(block.text.len()).unwrap_or(u32::MAX);
        let whole = Span { start: 0, end: len };
        let mut b = Builder::new(whole);
        if block.head == "bad" {
            let one = b.expr(Expr::Number(1.0));
            let text = b.ty(Ty::Name("String".into()));
            let wrong = b.expr(Expr::Annot {
                expr: one,
                ty: text,
            });
            return Ok(b.finish(wrong));
        }
        let mut fields = Vec::new();
        let mut rest: &[Token] = &block.tokens;
        while let Some((name, after)) = rest.split_first() {
            let label = word(block, name, TokenKind::Ident)
                .ok_or_else(|| vec![refuse(name, "expected a name")])?;
            rest = after;
            let ty = if punct(block, rest.first(), ":") {
                let start = rest.get(1).map_or(len, |t| t.span.start);
                let hole = compiler.parse_type(start, len).map_err(|d| vec![d])?;
                rest = past(rest, hole.span.end);
                Some(hole)
            } else {
                None
            };
            if !punct(block, rest.first(), "=") {
                return Err(vec![refuse(rest.first().unwrap_or(name), "expected `=`")]);
            }
            let start = rest.get(1).map_or(len, |t| t.span.start);
            let value = compiler.parse_expr(start, len).map_err(|d| vec![d])?;
            rest = past(rest, value.span.end);
            if punct(block, rest.first(), ";") {
                rest = &rest[1..];
            }
            let mut node = b.at(value.span).expr(Expr::Hole(value.span));
            if let Some(ty) = ty {
                let annotation = b.at(ty.span).ty(Ty::Hole(ty.span));
                let pair = Span {
                    start: name.span.start,
                    end: value.span.end,
                };
                node = b.at(pair).expr(Expr::Annot {
                    expr: node,
                    ty: annotation,
                });
            }
            fields.push(Field {
                label: label.to_owned(),
                value: node,
            });
        }
        let count = state.try_into().map_or(0, u32::from_le_bytes);
        let blocks = b.at(whole).expr(Expr::Number(f64::from(count)));
        fields.push(Field {
            label: "blocks".into(),
            value: blocks,
        });
        let record = b.expr(Expr::Record(fields));
        Ok(b.finish(record))
    }
}

fn word<'b>(block: &'b Block, token: &Token, kind: TokenKind) -> Option<&'b str> {
    (token.kind == kind).then(|| &block.text[token.span.start as usize..token.span.end as usize])
}

fn punct(block: &Block, token: Option<&Token>, text: &str) -> bool {
    token.is_some_and(|t| word(block, t, TokenKind::Punct) == Some(text))
}

/// The tokens of `rest` after `end`.
fn past(rest: &[Token], end: u32) -> &[Token] {
    let skip = rest.iter().take_while(|t| t.span.end <= end).count();
    &rest[skip..]
}

fn refuse(token: &Token, message: &str) -> Diagnostic {
    Diagnostic {
        message: message.into(),
        span: token.span,
    }
}
