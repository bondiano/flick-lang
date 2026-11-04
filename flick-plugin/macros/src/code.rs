//! The builder calls the parts of `flick!` become: each an expression
//! giving the `NodeId` of its node, its children built into locals first.

use proc_macro2::{Literal, TokenStream as Tokens};
use quote::quote;

use super::Error;

/// What a list holds: a part, or the items of a Rust collection.
pub enum Item {
    One(Tokens),
    Spread(Tokens),
}

#[derive(Clone, Copy)]
pub enum Role {
    Expr,
    Ty,
    Pat,
    Decl,
}

impl Role {
    pub fn method(self) -> Tokens {
        match self {
            Role::Expr => quote!(expr),
            Role::Ty => quote!(ty),
            Role::Pat => quote!(pat),
            Role::Decl => quote!(decl),
        }
    }

    pub fn kind(self) -> Tokens {
        match self {
            Role::Expr => quote!(::flick_plugin::Expr),
            Role::Ty => quote!(::flick_plugin::Ty),
            Role::Pat => quote!(::flick_plugin::Pat),
            Role::Decl => quote!(::flick_plugin::Decl),
        }
    }

    /// `__b.expr(kind)` and the like.
    pub fn node(self, kind: &Tokens) -> Tokens {
        let method = self.method();
        quote!(__b.#method(#kind))
    }
}

/// `head arguments…` in `role`.
pub fn apply(role: Role, head: &Tokens, arguments: Vec<Item>) -> Tokens {
    let kind = role.kind();
    let method = role.method();
    let arguments = list(role, arguments);
    quote!({
        let __h = #head;
        let __a = #arguments;
        __b.#method(#kind::Apply { function: __h, arguments: __a })
    })
}

/// The ids of `items`, in order, as a `Vec`.
pub fn list(role: Role, items: Vec<Item>) -> Tokens {
    let method = role.method();
    let pushes = items.into_iter().map(|item| match item {
        Item::One(code) => quote!({ let __n = #code; __v.push(__n); }),
        Item::Spread(xs) => quote!(__v.extend(::flick_plugin::__private::each(#xs, &mut *__b, ::flick_plugin::Splice::#method));),
    });
    quote!({
        let mut __v: ::std::vec::Vec<::flick_plugin::NodeId> = ::std::vec::Vec::new();
        #(#pushes)*
        __v
    })
}

/// A string or a number, in `role`.
pub fn literal(l: &Literal, role: Role) -> Result<Tokens, Error> {
    let kind = role.kind();
    let text = l.to_string();
    if text.starts_with('"') || text.starts_with("r\"") || text.starts_with("r#") {
        return match role {
            Role::Expr | Role::Ty => {
                Ok(role.node(&quote!(#kind::Text(::std::string::String::from(#l)))))
            }
            Role::Pat | Role::Decl => Err((
                l.span(),
                "only an expression or a type is a text".to_owned(),
            )),
        };
    }
    let n = Literal::f64_unsuffixed(number(l)?);
    match role {
        Role::Expr => Ok(role.node(&quote!(#kind::Number(#n)))),
        Role::Ty | Role::Pat | Role::Decl => {
            Err((l.span(), "a number is an expression".to_owned()))
        }
    }
}

/// The value of a number literal, which Flick's numbers hold.
pub fn number(l: &Literal) -> Result<f64, Error> {
    let text = l.to_string().replace('_', "");
    match text.parse::<f64>() {
        Ok(n) if n.is_finite() => Ok(n),
        _ => Err((l.span(), "expected a string or a number".to_owned())),
    }
}
