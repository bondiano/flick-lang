//! `flick!(b, code)`: the code a plugin gives, written in Flick and built in
//! the arena of the builder `b`, a node at a time, children first. `#x` and
//! `#(expr)` put a value of the plugin in place: a `Span` is the user's code
//! there, a string a text, a `NodeId` a node built before, `Name("M.f")` a
//! name. `#(..xs)` puts each item of `xs` in a list, the arguments of an
//! application or the fields of a record. `flick!(b @ span, code)` gives the
//! nodes the origin `span`, and `flick!(b, ty T)` builds a type.
//!
//! The code is parsed when the plugin is compiled: what is not Flick the
//! arena holds is an error of `cargo build`.

mod code;
mod parse;

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as Tokens, TokenTree};
use quote::{quote, quote_spanned};

use parse::Parser;

#[proc_macro]
pub fn flick(input: TokenStream) -> TokenStream {
    match expand(input.into()) {
        Ok(code) => code.into(),
        Err((span, message)) => quote_spanned!(span=> ::core::compile_error!(#message)).into(),
    }
}

/// Where the code is wrong, and why.
type Error = (Span, String);

fn expand(input: Tokens) -> Result<Tokens, Error> {
    let tokens: Vec<TokenTree> = input.into_iter().collect();
    let comma = top_level(&tokens, ',').ok_or_else(|| {
        (
            Span::call_site(),
            "expected `flick!(builder, code)` or `flick!(builder @ origin, code)`".to_owned(),
        )
    })?;
    let (head, code) = (&tokens[..comma], &tokens[comma + 1..]);
    let (builder, origin) = match top_level(head, '@') {
        Some(at) => (&head[..at], Some(&head[at + 1..])),
        None => (head, None),
    };
    if builder.is_empty() || origin.is_some_and(<[TokenTree]>::is_empty) {
        return Err((
            Span::call_site(),
            "expected a builder before `,`".to_owned(),
        ));
    }
    let mut p = Parser::new(code.to_vec(), Span::call_site());
    let node = if p.at_word("ty") {
        p.bump();
        p.ty()?
    } else {
        p.expr()?
    };
    p.end()?;
    let builder: Tokens = builder.iter().cloned().collect();
    let at = origin.map(|o| {
        let o: Tokens = o.iter().cloned().collect();
        quote!(__b.at(::core::convert::Into::<::flick_plugin::Span>::into(#o));)
    });
    Ok(quote!({
        let __b: &mut ::flick_plugin::Builder = (#builder).as_builder();
        #at
        #node
    }))
}

/// The place of the first `c` outside brackets in `tokens`.
fn top_level(tokens: &[TokenTree], c: char) -> Option<usize> {
    tokens
        .iter()
        .position(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == c))
}
