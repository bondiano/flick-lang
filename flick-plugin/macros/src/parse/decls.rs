//! Declarations of `flick!`: `let name params : T = e`, `name : T`,
//! `type alias N a = T`, `type N a = A x | B deriving C`, and
//! `instance C a => C (T a) { let m x = e; m : T }`. A name a declaration
//! declares is written as it is, or `#(expr)` for one the plugin makes.

use proc_macro2::{Delimiter, Spacing, TokenStream as Tokens, TokenTree};
use quote::quote;

use super::Parser;
use crate::Error;
use crate::code::{Item, Role, list};

impl Parser {
    /// Whether a declaration starts here.
    pub fn at_decl(&self) -> bool {
        self.at_word("let")
            || self.at_word("type") && !self.at_type()
            || self.at_word("instance")
            || self.at_signature()
    }

    /// `type T`, a type: unlike `type N = …` and `type alias N = …`, it has
    /// no `=` of its own.
    pub fn at_type(&self) -> bool {
        let alias = matches!(self.nth(1), Some(TokenTree::Ident(i)) if i == "alias");
        let equals = (self.tokens[self.pos..].iter())
            .any(|t| matches!(t, TokenTree::Punct(p) if p.as_char() == '=' && p.spacing() == Spacing::Alone));
        self.at_word("type") && !alias && !equals
    }

    /// `name :` or `#(name) :`.
    fn at_signature(&self) -> bool {
        let colon =
            |t: Option<&TokenTree>| matches!(t, Some(TokenTree::Punct(c)) if c.as_char() == ':');
        match (self.peek(), self.nth(1)) {
            (Some(TokenTree::Ident(_)), next) => colon(next),
            (Some(TokenTree::Punct(h)), Some(TokenTree::Group(_))) => {
                h.as_char() == '#' && colon(self.nth(2))
            }
            _ => false,
        }
    }

    pub fn decl(&mut self) -> Result<Tokens, Error> {
        if self.at_word("let") {
            self.let_decl()
        } else if self.at_word("type") {
            self.type_decl()
        } else if self.at_word("instance") {
            self.instance()
        } else {
            self.signature()
        }
    }

    /// A name a declaration declares, as a `String`.
    fn declared(&mut self) -> Result<Tokens, Error> {
        if self.at_punct('#') {
            let name = self.splice(false)?;
            return Ok(quote!(::std::string::ToString::to_string(&(#name))));
        }
        match self.peek().cloned() {
            Some(TokenTree::Ident(i)) => {
                self.bump();
                let name = i.to_string();
                Ok(quote!(::std::string::String::from(#name)))
            }
            _ => self.error("expected the name it declares"),
        }
    }

    fn let_decl(&mut self) -> Result<Tokens, Error> {
        self.bump();
        let rec = self.at_word("rec");
        if rec {
            self.bump();
        }
        let name = self.declared()?;
        let mut params = Vec::new();
        while !self.at_punct(':') && !self.at_punct('=') {
            if self.peek().is_none() {
                return self.error("expected `=`");
            }
            params.push(if self.at_spread() {
                Item::Spread(self.splice(true)?)
            } else {
                Item::One(self.pat_atom()?)
            });
        }
        let ty = if self.eat_punct(':') {
            let ty = self.ty()?;
            quote!(::core::option::Option::Some(#ty))
        } else {
            quote!(::core::option::Option::None)
        };
        self.expect_punct('=')?;
        let body = self.expr()?;
        let params = list(Role::Pat, params);
        Ok(quote!({
            let __n = #name;
            let __p = #params;
            let __t = #ty;
            let __e = #body;
            __b.decl(::flick_plugin::Decl::Let { name: __n, params: __p, ty: __t, body: __e, rec: #rec })
        }))
    }

    fn signature(&mut self) -> Result<Tokens, Error> {
        let name = self.declared()?;
        self.expect_punct(':')?;
        let ty = self.ty()?;
        Ok(quote!({
            let __n = #name;
            let __t = #ty;
            __b.decl(::flick_plugin::Decl::Signature { name: __n, ty: __t })
        }))
    }

    fn type_decl(&mut self) -> Result<Tokens, Error> {
        self.bump();
        let alias = self.at_word("alias");
        if alias {
            self.bump();
        }
        let name = self.declared()?;
        let mut params = Vec::new();
        while !self.at_punct('=') {
            match self.peek().cloned() {
                Some(TokenTree::Ident(i)) => {
                    self.bump();
                    let param = i.to_string();
                    params.push(quote!(::std::string::String::from(#param)));
                }
                _ => return self.error("expected a parameter or `=`"),
            }
        }
        self.expect_punct('=')?;
        let params = quote!(::std::vec![#(#params),*]);
        if alias {
            let ty = self.ty()?;
            return Ok(quote!({
                let __n = #name;
                let __t = #ty;
                __b.decl(::flick_plugin::Decl::Alias { name: __n, params: #params, ty: __t })
            }));
        }
        self.eat_punct('|');
        let mut cases = Vec::new();
        loop {
            let case = self.declared()?;
            let mut fields = Vec::new();
            while self.peek().is_some() && !self.at_punct('|') && !self.at_word("deriving") {
                fields.push(Item::One(self.ty_atom()?));
            }
            let fields = list(Role::Ty, fields);
            cases.push(quote!({
                let __n = #case;
                let __f = #fields;
                ::flick_plugin::Case { name: __n, fields: __f }
            }));
            if !self.eat_punct('|') {
                break;
            }
        }
        let mut deriving = Vec::new();
        if self.at_word("deriving") {
            self.bump();
            loop {
                let (class, _) = self.path()?;
                deriving.push(quote!(::std::string::String::from(#class)));
                if !self.eat_punct(',') {
                    break;
                }
            }
        }
        Ok(quote!({
            let __n = #name;
            let mut __c = ::std::vec::Vec::new();
            #( __c.push(#cases); )*
            __b.decl(::flick_plugin::Decl::Adt {
                name: __n,
                params: #params,
                cases: __c,
                deriving: ::std::vec![#(#deriving),*],
            })
        }))
    }

    /// `instance context => head { members }`, the members the last group.
    fn instance(&mut self) -> Result<Tokens, Error> {
        self.bump();
        let rest: Vec<TokenTree> = self.tokens[self.pos..].to_vec();
        self.pos = self.tokens.len();
        let (head, members) = match rest.split_last() {
            Some((TokenTree::Group(g), head)) if g.delimiter() == Delimiter::Brace => {
                (head.to_vec(), Some(g.clone()))
            }
            _ => (rest, None),
        };
        let arrow = head.windows(2).position(|w| {
            matches!((&w[0], &w[1]), (TokenTree::Punct(a), TokenTree::Punct(b))
                if a.as_char() == '=' && b.as_char() == '>')
        });
        let (context, head) = match arrow {
            Some(i) => (head[..i].to_vec(), head[i + 2..].to_vec()),
            None => (Vec::new(), head),
        };
        let mut constraints = Vec::new();
        match context.as_slice() {
            [] => {}
            // `(C a, D b) =>`: each
            [TokenTree::Group(g)] if g.delimiter() == Delimiter::Parenthesis => {
                let mut p = Parser::inner(g);
                loop {
                    constraints.push(Item::One(p.ty()?));
                    if !p.eat_punct(',') {
                        break;
                    }
                }
                p.end()?;
            }
            one => {
                let mut p = Parser::new(one.to_vec(), self.end);
                constraints.push(Item::One(p.ty()?));
                p.end()?;
            }
        }
        let mut p = Parser::new(head, self.end);
        let head = p.ty()?;
        p.end()?;
        let mut methods = Vec::new();
        if let Some(group) = members {
            let mut p = Parser::inner(&group);
            while p.peek().is_some() {
                methods.push(if p.at_spread() {
                    Item::Spread(p.splice(true)?)
                } else {
                    Item::One(p.decl()?)
                });
                if !p.eat_punct(';') {
                    break;
                }
            }
            p.end()?;
        }
        let context = list(Role::Ty, constraints);
        let members = list(Role::Decl, methods);
        Ok(quote!({
            let __x = #context;
            let __h = #head;
            let __m = #members;
            __b.decl(::flick_plugin::Decl::Instance { context: __x, head: __h, members: __m })
        }))
    }
}
