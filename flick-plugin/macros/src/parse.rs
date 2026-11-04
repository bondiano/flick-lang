//! The Flick of `flick!`, parsed from Rust's tokens into the builder calls
//! that make it. Each part becomes an expression giving the `NodeId` of its
//! node; a node's children are built, into locals, before it.
//!
//! - expressions: names (`Cli.flag`, `x`), numbers, strings, `true`,
//!   `false`, `()`, application, `e.label`, `(e : T)`, tuples, `[…]`,
//!   `{ l = e; … }`, `fun p … -> e`;
//! - types: names, `_`, strings (`Record.Label "x"`), application,
//!   `A -> B`, `{ l : T; … }`;
//! - patterns: names, `_`, `Con p …`, `{ l; l = p }`.

mod decls;
mod types;

use proc_macro2::{Delimiter, Literal, Span, TokenStream as Tokens, TokenTree};
use quote::quote;

use super::Error;
use crate::code::{Item, Role, apply, list, literal, number};

pub struct Parser {
    tokens: Vec<TokenTree>,
    pos: usize,
    /// Where the tokens end, for errors at their end.
    end: Span,
}

impl Parser {
    pub fn new(tokens: Vec<TokenTree>, end: Span) -> Self {
        Parser {
            tokens,
            pos: 0,
            end,
        }
    }

    fn inner(group: &proc_macro2::Group) -> Self {
        Parser::new(group.stream().into_iter().collect(), group.span_close())
    }

    fn peek(&self) -> Option<&TokenTree> {
        self.tokens.get(self.pos)
    }

    fn nth(&self, n: usize) -> Option<&TokenTree> {
        self.tokens.get(self.pos + n)
    }

    pub fn bump(&mut self) -> Option<TokenTree> {
        let t = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn span(&self) -> Span {
        self.peek().map_or(self.end, TokenTree::span)
    }

    fn error<T>(&self, message: &str) -> Result<T, Error> {
        Err((self.span(), message.to_owned()))
    }

    pub fn end(&self) -> Result<(), Error> {
        if self.peek().is_some() {
            return self.error("expected the end of the code here");
        }
        Ok(())
    }

    fn at_punct(&self, c: char) -> bool {
        matches!(self.peek(), Some(TokenTree::Punct(p)) if p.as_char() == c)
    }

    fn eat_punct(&mut self, c: char) -> bool {
        let at = self.at_punct(c);
        if at {
            self.pos += 1;
        }
        at
    }

    fn expect_punct(&mut self, c: char) -> Result<(), Error> {
        if self.eat_punct(c) {
            Ok(())
        } else {
            self.error(&format!("expected `{c}`"))
        }
    }

    pub fn at_word(&self, word: &str) -> bool {
        matches!(self.peek(), Some(TokenTree::Ident(i)) if i == word)
    }

    /// `->`.
    fn at_arrow(&self) -> bool {
        matches!(
            (self.peek(), self.nth(1)),
            (Some(TokenTree::Punct(a)), Some(TokenTree::Punct(b)))
                if a.as_char() == '-' && b.as_char() == '>'
        )
    }

    /// Where an application or a sequence of its arguments ends.
    fn at_stop(&self) -> bool {
        self.peek().is_none()
            || self.at_arrow()
            || [':', ',', ';', '='].iter().any(|&c| self.at_punct(c))
    }

    /// `#(..xs)`.
    fn at_spread(&self) -> bool {
        self.at_punct('#')
            && matches!(self.nth(1), Some(TokenTree::Group(g))
                if g.delimiter() == Delimiter::Parenthesis
                    && g.stream().into_iter().take(2).filter(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == '.')).count() == 2)
    }

    /// `#x` or `#(e)`: the Rust expression; `#(..e)` with `spread`.
    fn splice(&mut self, spread: bool) -> Result<Tokens, Error> {
        self.expect_punct('#')?;
        match self.bump() {
            Some(TokenTree::Ident(i)) if !spread => Ok(quote!(#i)),
            Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis => {
                let inner: Tokens = g
                    .stream()
                    .into_iter()
                    .skip(if spread { 2 } else { 0 })
                    .collect();
                if inner.is_empty() {
                    return Err((g.span(), "expected a Rust expression".to_owned()));
                }
                Ok(inner)
            }
            Some(t) => Err((t.span(), "expected `#name` or `#(expression)`".to_owned())),
            None => Err((self.end, "expected `#name` or `#(expression)`".to_owned())),
        }
    }

    /// A value of the plugin in place of `#x`, in `role`.
    fn spliced(&mut self, role: Role) -> Result<Tokens, Error> {
        if self.at_spread() {
            return self
                .error("`#(..xs)` goes in a list, a record or the arguments of an application");
        }
        let value = self.splice(false)?;
        let method = role.method();
        Ok(quote!(::flick_plugin::Splice::#method(&(#value), &mut *__b)))
    }

    /// `M.N.x`: its parts joined, and whether it is qualified by a module,
    /// a path that starts with a capital.
    fn path(&mut self) -> Result<(String, bool), Error> {
        let Some(TokenTree::Ident(first)) = self.bump() else {
            return self.error("expected a name");
        };
        let mut name = first.to_string();
        let qualified = name.starts_with(char::is_uppercase);
        while qualified && self.at_punct('.') && matches!(self.nth(1), Some(TokenTree::Ident(_))) {
            self.pos += 1;
            let Some(TokenTree::Ident(next)) = self.bump() else {
                unreachable!("checked above")
            };
            name.push('.');
            name.push_str(&next.to_string());
            if !next.to_string().starts_with(char::is_uppercase) {
                break;
            }
        }
        Ok((name, qualified))
    }

    // ---- expressions ----

    pub fn expr(&mut self) -> Result<Tokens, Error> {
        if self.at_word("fun") {
            return self.lambda();
        }
        let head = self.postfix()?;
        let mut arguments = Vec::new();
        while !self.at_stop() {
            arguments.push(if self.at_spread() {
                Item::Spread(self.splice(true)?)
            } else {
                Item::One(self.postfix()?)
            });
        }
        if arguments.is_empty() {
            return Ok(head);
        }
        Ok(apply(Role::Expr, &head, arguments))
    }

    fn lambda(&mut self) -> Result<Tokens, Error> {
        self.bump();
        let mut params = Vec::new();
        while !self.at_arrow() {
            if self.peek().is_none() {
                return self.error("expected `->` after the parameters");
            }
            params.push(Item::One(self.pat_atom()?));
        }
        if params.is_empty() {
            return self.error("a `fun` takes a parameter");
        }
        self.pos += 2;
        let body = self.expr()?;
        let params = list(Role::Pat, params);
        Ok(quote!({
            let __p = #params;
            let __e = #body;
            __b.expr(::flick_plugin::Expr::Lambda { params: __p, body: __e })
        }))
    }

    fn postfix(&mut self) -> Result<Tokens, Error> {
        let mut e = self.atom()?;
        while self.at_punct('.') {
            self.pos += 1;
            let Some(TokenTree::Ident(label)) = self.bump() else {
                return self.error("expected a field after `.`");
            };
            let label = label.to_string();
            e = quote!({
                let __r = #e;
                __b.expr(::flick_plugin::Expr::Get { record: __r, label: ::std::string::String::from(#label) })
            });
        }
        Ok(e)
    }

    fn atom(&mut self) -> Result<Tokens, Error> {
        let kind = Role::Expr.kind();
        match self.peek().cloned() {
            Some(TokenTree::Ident(i)) if i == "true" || i == "false" => {
                self.pos += 1;
                Ok(Role::Expr.node(&quote!(#kind::Boolean(#i))))
            }
            Some(TokenTree::Ident(i)) if i == "_" => self.error("`_` is a type or a pattern"),
            Some(TokenTree::Ident(_)) => {
                let (name, _) = self.path()?;
                Ok(Role::Expr.node(&quote!(#kind::Name(::std::string::String::from(#name)))))
            }
            Some(TokenTree::Literal(l)) => {
                self.pos += 1;
                literal(&l, Role::Expr)
            }
            Some(TokenTree::Punct(p)) if p.as_char() == '-' => {
                self.pos += 1;
                match self.bump() {
                    Some(TokenTree::Literal(l)) => {
                        let n = number(&l)?;
                        let n = Literal::f64_unsuffixed(-n);
                        Ok(Role::Expr.node(&quote!(#kind::Number(#n))))
                    }
                    _ => Err((
                        p.span(),
                        "operators are not code a plugin gives: apply a function".to_owned(),
                    )),
                }
            }
            Some(TokenTree::Punct(p)) if p.as_char() == '#' => self.spliced(Role::Expr),
            Some(TokenTree::Group(g)) => {
                self.pos += 1;
                Self::group_expr(&g)
            }
            Some(t) => Err((t.span(), "expected an expression".to_owned())),
            None => self.error("expected an expression"),
        }
    }

    fn group_expr(g: &proc_macro2::Group) -> Result<Tokens, Error> {
        let kind = Role::Expr.kind();
        let mut p = Parser::inner(g);
        match g.delimiter() {
            Delimiter::Parenthesis if p.peek().is_none() => {
                Ok(Role::Expr.node(&quote!(#kind::Unit)))
            }
            Delimiter::Parenthesis => {
                let e = p.expr()?;
                if p.eat_punct(':') {
                    let ty = p.ty()?;
                    p.end()?;
                    return Ok(quote!({
                        let __e = #e;
                        let __t = #ty;
                        __b.expr(#kind::Annot { expr: __e, ty: __t })
                    }));
                }
                if !p.at_punct(',') {
                    p.end()?;
                    return Ok(e);
                }
                let mut items = vec![Item::One(e)];
                while p.eat_punct(',') {
                    items.push(p.item(Role::Expr)?);
                }
                p.end()?;
                let items = list(Role::Expr, items);
                Ok(quote!({ let __i = #items; __b.expr(#kind::Tuple(__i)) }))
            }
            Delimiter::Bracket => {
                let mut items = Vec::new();
                while p.peek().is_some() {
                    items.push(p.item(Role::Expr)?);
                    if !p.eat_punct(',') {
                        break;
                    }
                }
                p.end()?;
                let items = list(Role::Expr, items);
                Ok(quote!({ let __i = #items; __b.expr(#kind::Items(__i)) }))
            }
            Delimiter::Brace => p.record(Role::Expr),
            Delimiter::None => {
                let e = p.expr()?;
                p.end()?;
                Ok(e)
            }
        }
    }

    /// An item of a list or tuple: a part, or `#(..xs)`.
    fn item(&mut self, role: Role) -> Result<Item, Error> {
        if self.at_spread() {
            return Ok(Item::Spread(self.splice(true)?));
        }
        Ok(Item::One(match role {
            Role::Expr => self.expr()?,
            Role::Ty => self.ty()?,
            Role::Pat => self.pat()?,
            Role::Decl => self.decl()?,
        }))
    }

    /// `{ l = e; … }`, `{ l : T; … }` or `{ l; l = p }`, with `#(..fields)`.
    fn record(&mut self, role: Role) -> Result<Tokens, Error> {
        let mut pushes = Vec::new();
        while self.peek().is_some() {
            if self.at_spread() {
                let fields = self.splice(true)?;
                let method = role.method();
                pushes.push(quote!(__f.extend(::flick_plugin::__private::fields(#fields, &mut *__b, ::flick_plugin::Splice::#method));));
            } else {
                let Some(TokenTree::Ident(label)) = self.bump() else {
                    return self.error("expected a field name");
                };
                let label = label.to_string();
                let value = match role {
                    Role::Expr => {
                        self.expect_punct('=')?;
                        self.expr()?
                    }
                    Role::Ty => {
                        self.expect_punct(':')?;
                        self.ty()?
                    }
                    // `{ l }` binds `l`
                    Role::Pat if self.eat_punct('=') => self.pat()?,
                    Role::Pat => Role::Pat.node(
                        &quote!(::flick_plugin::Pat::Var(::std::string::String::from(#label))),
                    ),
                    Role::Decl => return self.error("a declaration is no record"),
                };
                pushes.push(quote!({
                    let __v = #value;
                    __f.push(::flick_plugin::Field { label: ::std::string::String::from(#label), value: __v });
                }));
            }
            if !self.eat_punct(';') {
                break;
            }
        }
        self.end()?;
        let kind = role.kind();
        let method = role.method();
        Ok(quote!({
            let mut __f = ::std::vec::Vec::new();
            #(#pushes)*
            __b.#method(#kind::Record(__f))
        }))
    }
}
