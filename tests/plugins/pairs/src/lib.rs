//! A plugin for the compiler's tests. `pairs { k = e; k : T = e }` stands
//! for the record `{ k = e; k = (e : T); blocks = n }`, where `n` is the
//! number of `pairs` blocks of the group, which `collect` counts; each
//! argument of the head, as in `pairs "a" (1 + 2) { … }`, adds a field
//! `arg0`, `arg1`, … of its value. The heads of the other forms of blocks
//! are in `forms`. `bad {}`
//! stands for `(1.0 : String)`, an error in generated code; `coded { X001 }`
//! refuses with the code its body names. `spin {}` never
//! ends and `hog {}` takes all memory: the limits of a plugin's calls.

mod forms;

use std::hint::black_box;

use flick_plugin::{
    Arena, Block, Builder, Compiler, Diagnostic, NodeId, Plugin, Span, Token, TokenKind, flick,
};

pub struct Pairs;

#[cfg(feature = "guest")]
flick_plugin::export!(Pairs);

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
        if let Some(made) = forms::expand(block, compiler) {
            return made;
        }
        let whole = block.whole();
        let mut b = Builder::new(whole);
        match block.head.as_str() {
            "spin" => loop {
                black_box(block);
            },
            "hog" => {
                let mut held = Vec::new();
                loop {
                    held.push(vec![1u8; 1 << 20]);
                    black_box(&held);
                }
            }
            _ => {}
        }
        if block.head == "coded" {
            let code = block.text.trim();
            return Err(vec![Diagnostic::error("refused", whole).with_code(code)]);
        }
        if block.head == "bad" {
            let wrong = flick!(b, (1.0 : String));
            return Ok(b.finish(wrong));
        }
        let mut fields = arguments(block, compiler, &mut b)?;
        let mut c = block.body();
        while let Some(name) = c.next() {
            if name.kind != TokenKind::Ident {
                return Err(vec![refuse(name, "expected a name")]);
            }
            let ty = match c.eat(":") {
                Some(_) => Some(c.ty(compiler).map_err(|d| vec![d])?),
                None => None,
            };
            if c.eat("=").is_none() {
                return Err(vec![refuse(c.peek().unwrap_or(name), "expected `=`")]);
            }
            let value = c.expr(compiler).map_err(|d| vec![d])?;
            c.eat(";");
            // a pair with a type stands for its name to its value
            let node = if let Some(ty) = ty {
                let pair = Span {
                    start: name.span.start,
                    end: value.end,
                };
                flick!(b @ pair, (#value : #ty))
            } else {
                flick!(b @ value, #value)
            };
            fields.push((c.text(name).to_owned(), node));
        }
        let count = state.try_into().map_or(0, u32::from_le_bytes);
        let record = flick!(b @ whole, { #(..fields); blocks = #(f64::from(count)) });
        Ok(b.finish(record))
    }
}

/// A field `arg<i>` for each argument of the head: a string, or `( … )`.
fn arguments(
    block: &Block,
    compiler: &mut dyn Compiler,
    b: &mut Builder,
) -> Result<Vec<(String, NodeId)>, Vec<Diagnostic>> {
    let mut fields = Vec::new();
    let mut depth = 0u32;
    let mut start = None;
    for t in block.arguments() {
        start = start.or(Some(t.span.start));
        match block.text_of(t.span) {
            "(" => depth += 1,
            ")" => depth -= 1,
            _ => {}
        }
        if depth == 0
            && let Some(from) = start.take()
        {
            let value = compiler
                .parse_expr(from, t.span.end)
                .map_err(|d| vec![d])?
                .span;
            fields.push((format!("arg{}", fields.len()), flick!(b @ value, #value)));
        }
    }
    Ok(fields)
}

fn refuse(token: Token, message: &str) -> Diagnostic {
    Diagnostic::error(message, token.span)
}
