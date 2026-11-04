//! What `flick!` puts where its code says `#x`: a value of the plugin, as
//! an expression, a type, a pattern or a declaration by where it stands.

use crate::{Builder, Decl, Expr, NodeId, Pat, Span, Ty};

/// A value `flick!` puts in place of `#x`. A `Span` is the user's code
/// there (a hole of the kind its place asks for), a string a text, a [`NodeId`] the node built before, a
/// [`Name`] a name and [`Wildcard`] `_`; a value in a place it cannot stand,
/// as a text in a pattern, is a bug of the plugin and panics.
pub trait Splice {
    fn expr(&self, b: &mut Builder) -> NodeId;
    fn ty(&self, b: &mut Builder) -> NodeId;
    fn pat(&self, b: &mut Builder) -> NodeId;
    /// As a declaration: only the user's (a `Span`) or one built before.
    fn decl(&self, _: &mut Builder) -> NodeId {
        misplaced("this value", "a declaration")
    }
}

/// A name of the code, such as `Name("Cli.switch")`: a variable, a value
/// or a constructor of a module, or a type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Name<'a>(pub &'a str);

/// `_`: a type the checker infers, or a pattern that binds nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wildcard;

fn misplaced(what: &str, place: &str) -> ! {
    panic!("flick!: {what} cannot stand as {place}")
}

impl Splice for Span {
    fn expr(&self, b: &mut Builder) -> NodeId {
        b.expr(Expr::Hole(*self))
    }
    fn ty(&self, b: &mut Builder) -> NodeId {
        b.ty(Ty::Hole(*self))
    }
    fn pat(&self, b: &mut Builder) -> NodeId {
        b.pat(Pat::Hole(*self))
    }
    fn decl(&self, b: &mut Builder) -> NodeId {
        b.decl(Decl::Hole(*self))
    }
}

impl Splice for str {
    fn expr(&self, b: &mut Builder) -> NodeId {
        b.expr(Expr::Text(self.to_owned()))
    }
    fn ty(&self, b: &mut Builder) -> NodeId {
        b.ty(Ty::Text(self.to_owned()))
    }
    fn pat(&self, _: &mut Builder) -> NodeId {
        misplaced("a text", "a pattern")
    }
}

impl Splice for String {
    fn expr(&self, b: &mut Builder) -> NodeId {
        self.as_str().expr(b)
    }
    fn ty(&self, b: &mut Builder) -> NodeId {
        self.as_str().ty(b)
    }
    fn pat(&self, b: &mut Builder) -> NodeId {
        self.as_str().pat(b)
    }
    fn decl(&self, b: &mut Builder) -> NodeId {
        self.as_str().decl(b)
    }
}

/// A node built before, used as it is.
impl Splice for NodeId {
    fn expr(&self, _: &mut Builder) -> NodeId {
        *self
    }
    fn ty(&self, _: &mut Builder) -> NodeId {
        *self
    }
    fn pat(&self, _: &mut Builder) -> NodeId {
        *self
    }
    fn decl(&self, _: &mut Builder) -> NodeId {
        *self
    }
}

impl Splice for Name<'_> {
    fn expr(&self, b: &mut Builder) -> NodeId {
        b.expr(Expr::Name(self.0.to_owned()))
    }
    fn ty(&self, b: &mut Builder) -> NodeId {
        b.ty(Ty::Name(self.0.to_owned()))
    }
    fn pat(&self, b: &mut Builder) -> NodeId {
        if self.0.starts_with(char::is_uppercase) {
            b.pat(Pat::Con {
                name: self.0.to_owned(),
                arguments: Vec::new(),
            })
        } else {
            b.pat(Pat::Var(self.0.to_owned()))
        }
    }
}

impl Splice for Wildcard {
    fn expr(&self, _: &mut Builder) -> NodeId {
        misplaced("`_`", "an expression")
    }
    fn ty(&self, b: &mut Builder) -> NodeId {
        b.ty(Ty::Wildcard)
    }
    fn pat(&self, b: &mut Builder) -> NodeId {
        b.pat(Pat::Wildcard)
    }
}

impl Splice for f64 {
    fn expr(&self, b: &mut Builder) -> NodeId {
        b.expr(Expr::Number(*self))
    }
    fn ty(&self, _: &mut Builder) -> NodeId {
        misplaced("a number", "a type")
    }
    fn pat(&self, _: &mut Builder) -> NodeId {
        misplaced("a number", "a pattern")
    }
}

impl Splice for bool {
    fn expr(&self, b: &mut Builder) -> NodeId {
        b.expr(Expr::Boolean(*self))
    }
    fn ty(&self, _: &mut Builder) -> NodeId {
        misplaced("a boolean", "a type")
    }
    fn pat(&self, _: &mut Builder) -> NodeId {
        misplaced("a boolean", "a pattern")
    }
}

impl<T: Splice + ?Sized> Splice for &T {
    fn expr(&self, b: &mut Builder) -> NodeId {
        (**self).expr(b)
    }
    fn ty(&self, b: &mut Builder) -> NodeId {
        (**self).ty(b)
    }
    fn pat(&self, b: &mut Builder) -> NodeId {
        (**self).pat(b)
    }
    fn decl(&self, b: &mut Builder) -> NodeId {
        (**self).decl(b)
    }
}

/// What the code `flick!` writes calls.
#[doc(hidden)]
pub mod __private {
    use super::Splice;
    use crate::{Builder, Field, NodeId};

    /// Each item of `items`, by `put`, for `#(..items)`.
    pub fn each<I>(
        items: I,
        b: &mut Builder,
        put: impl Fn(&I::Item, &mut Builder) -> NodeId,
    ) -> Vec<NodeId>
    where
        I: IntoIterator,
        I::Item: Splice,
    {
        items.into_iter().map(|item| put(&item, b)).collect()
    }

    /// Each `(label, value)` of `fields`, by `put`, for `{ #(..fields) }`.
    pub fn fields<I, L, V>(
        fields: I,
        b: &mut Builder,
        put: impl Fn(&V, &mut Builder) -> NodeId,
    ) -> Vec<Field>
    where
        I: IntoIterator<Item = (L, V)>,
        L: AsRef<str>,
        V: Splice,
    {
        (fields.into_iter())
            .map(|(label, value)| Field {
                label: label.as_ref().to_owned(),
                value: put(&value, b),
            })
            .collect()
    }
}
