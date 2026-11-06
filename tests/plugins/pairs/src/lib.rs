//! A plugin for the compiler's tests. `pairs { k = e; k : T = e }` stands
//! for the record `{ k = e; k = (e : T); blocks = n }`, where `n` is the
//! number of `pairs` blocks of the group, which `collect` counts; each
//! argument of the head, as in `pairs "a" (1 + 2) { … }`, adds a field
//! `arg0`, `arg1`, … of its value. The heads of the other forms of blocks
//! are in `forms`. `bad {}`
//! stands for `(1.0 : String)`, an error in generated code; `coded { X001 }`
//! refuses with the code its body names. `spin {}` never
//! ends and `hog {}` takes all memory: the limits of a plugin's calls.
//! `deriving Pairs.Arbitrary` is in `derive`, the heads that reach
//! outside their blocks are in `reach`, and those that tell other modules
//! about a name in `published`.

mod derive;
mod forms;
mod published;
mod reach;

use std::hint::black_box;

use flick_plugin::{
    Arena, Block, Builder, Compiler, DeriveTarget, Diagnostic, Fact, NodeId, Plugin, Published,
    Span, Token, TokenKind, flick,
};

pub struct Pairs;

/// What `reach` read and the notes of `published`, in a state.
fn parts(state: &[u8]) -> (&[u8], &[u8]) {
    let len = (state.get(4..8))
        .and_then(|s| s.try_into().ok())
        .map_or(0, u32::from_le_bytes);
    let len = usize::try_from(len).unwrap_or(usize::MAX);
    let rest = state.get(8..).unwrap_or_default();
    rest.split_at(len.min(rest.len()))
}

#[cfg(feature = "guest")]
flick_plugin::export!(Pairs);

impl Plugin for Pairs {
    /// The number of `pairs` blocks, the length of what `reach` reads and
    /// it, then the notes of `published`.
    fn collect(&self, blocks: &[Block], compiler: &mut dyn Compiler) -> Vec<u8> {
        let pairs = blocks.iter().filter(|b| b.head == "pairs").count();
        let count = u32::try_from(pairs).unwrap_or(u32::MAX).to_le_bytes();
        let reached = reach::collect(blocks, compiler);
        let len = u32::try_from(reached.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes();
        [
            count.as_slice(),
            &len,
            &reached,
            &published::collect(blocks),
        ]
        .concat()
    }

    fn publish(&self, state: &[u8]) -> Vec<Published> {
        published::publish(parts(state).1)
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
        if let Some(made) = reach::expand(block, parts(state).0, compiler) {
            return made;
        }
        if let Some(made) = published::expand(block, compiler) {
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
        let count = (state.get(..4))
            .and_then(|s| s.try_into().ok())
            .map_or(0, u32::from_le_bytes);
        let record = flick!(b @ whole, { #(..fields); blocks = #(f64::from(count)) });
        Ok(b.finish(record))
    }

    /// What `typed { e }` checks: the type of its `_`, as `X002`.
    fn check(&self, block: &Block, _: &[u8], facts: &[Fact]) -> Vec<Diagnostic> {
        if block.head != "typed" {
            return Vec::new();
        }
        (facts.iter())
            .map(|f| {
                Diagnostic::error(format!("`_` is `{}`", f.ty.shown), f.span).with_code("X002")
            })
            .collect()
    }

    fn derive(
        &self,
        target: &DeriveTarget,
        class: &str,
        _: Option<&Block>,
        _: &[DeriveTarget],
        _: &[u8],
        _: &mut dyn Compiler,
    ) -> Result<Arena, Vec<Diagnostic>> {
        derive::derive(target, class)
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
