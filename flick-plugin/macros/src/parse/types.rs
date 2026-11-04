//! Types and patterns of `flick!`.

use proc_macro2::{Delimiter, TokenStream as Tokens, TokenTree};
use quote::quote;

use super::Parser;
use crate::Error;
use crate::code::{Item, Role, apply, list, literal};

impl Parser {
    pub fn ty(&mut self) -> Result<Tokens, Error> {
        let head = self.ty_atom()?;
        let mut arguments = Vec::new();
        while !self.at_stop() {
            arguments.push(Item::One(self.ty_atom()?));
        }
        let applied = if arguments.is_empty() {
            head
        } else {
            apply(Role::Ty, &head, arguments)
        };
        if !self.at_arrow() {
            return Ok(applied);
        }
        self.pos += 2;
        let output = self.ty()?;
        Ok(quote!({
            let __i = #applied;
            let __o = #output;
            __b.ty(::flick_plugin::Ty::Function { input: __i, output: __o })
        }))
    }

    pub(super) fn ty_atom(&mut self) -> Result<Tokens, Error> {
        let kind = Role::Ty.kind();
        match self.peek().cloned() {
            Some(TokenTree::Ident(i)) if i == "_" => {
                self.pos += 1;
                Ok(Role::Ty.node(&quote!(#kind::Wildcard)))
            }
            Some(TokenTree::Ident(_)) => {
                let (name, _) = self.path()?;
                Ok(Role::Ty.node(&quote!(#kind::Name(::std::string::String::from(#name)))))
            }
            Some(TokenTree::Literal(l)) => {
                self.pos += 1;
                literal(&l, Role::Ty)
            }
            Some(TokenTree::Punct(p)) if p.as_char() == '#' => self.spliced(Role::Ty),
            Some(TokenTree::Group(g)) => {
                self.pos += 1;
                let mut p = Parser::inner(&g);
                match g.delimiter() {
                    Delimiter::Brace => p.record(Role::Ty),
                    Delimiter::Parenthesis | Delimiter::None => {
                        let t = p.ty()?;
                        p.end()?;
                        Ok(t)
                    }
                    Delimiter::Bracket => Err((g.span(), "expected a type".to_owned())),
                }
            }
            Some(t) => Err((t.span(), "expected a type".to_owned())),
            None => self.error("expected a type"),
        }
    }

    // ---- patterns ----

    /// A pattern: a constructor applied to patterns, or one atom.
    pub(super) fn pat(&mut self) -> Result<Tokens, Error> {
        let constructor = matches!(self.peek(), Some(TokenTree::Ident(i)) if i.to_string().starts_with(char::is_uppercase));
        if !constructor {
            return self.pat_atom();
        }
        let (name, _) = self.path()?;
        let mut arguments = Vec::new();
        while !self.at_stop() {
            arguments.push(Item::One(self.pat_atom()?));
        }
        let arguments = list(Role::Pat, arguments);
        Ok(quote!({
            let __a = #arguments;
            __b.pat(::flick_plugin::Pat::Con { name: ::std::string::String::from(#name), arguments: __a })
        }))
    }

    pub(super) fn pat_atom(&mut self) -> Result<Tokens, Error> {
        let kind = Role::Pat.kind();
        match self.peek().cloned() {
            Some(TokenTree::Ident(i)) if i == "_" => {
                self.pos += 1;
                Ok(Role::Pat.node(&quote!(#kind::Wildcard)))
            }
            Some(TokenTree::Ident(i)) if i.to_string().starts_with(char::is_uppercase) => {
                let (name, _) = self.path()?;
                Ok(Role::Pat.node(&quote!(#kind::Con { name: ::std::string::String::from(#name), arguments: ::std::vec::Vec::new() })))
            }
            Some(TokenTree::Ident(i)) => {
                self.pos += 1;
                let name = i.to_string();
                Ok(Role::Pat.node(&quote!(#kind::Var(::std::string::String::from(#name)))))
            }
            Some(TokenTree::Punct(p)) if p.as_char() == '#' => self.spliced(Role::Pat),
            Some(TokenTree::Group(g)) => {
                self.pos += 1;
                let mut p = Parser::inner(&g);
                match g.delimiter() {
                    Delimiter::Brace => p.record(Role::Pat),
                    Delimiter::Parenthesis | Delimiter::None => {
                        let q = p.pat()?;
                        p.end()?;
                        Ok(q)
                    }
                    Delimiter::Bracket => Err((g.span(), "expected a pattern".to_owned())),
                }
            }
            Some(t) => Err((t.span(), "expected a pattern".to_owned())),
            None => self.error("expected a pattern"),
        }
    }
}
