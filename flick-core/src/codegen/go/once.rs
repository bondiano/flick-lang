//! Values a module makes once. A `BigInt` literal, a constructor without
//! fields and an adapter of stages read nothing but the runtime, so each is
//! a package variable, made when the program starts, and not made again
//! every time the code that names it runs.

use flick_pretty::{Document, docvec};

use super::expr::literal;
use super::{Gen, names};
use crate::ir::Lit;

impl<'a> Gen<'a> {
    /// The package variable of `made`, a Go expression over the runtime
    /// alone: Go makes it before any code of the program runs.
    pub(super) fn once(&mut self, made: &str) -> &'a str {
        let at = self
            .once
            .iter()
            .position(|other| other == made)
            .unwrap_or_else(|| {
                self.once.push(made.to_owned());
                self.once.len() - 1
            });
        self.a.alloc_str(&names::once(self.module, at))
    }

    /// The declarations of the variables the module's code named.
    pub(super) fn once_decls(&self) -> Vec<Document<'a>> {
        let a = self.a;
        let declare = |(at, made): (usize, &String)| {
            let name = a.alloc_str(&names::once(self.module, at));
            docvec![a, "var ", name, " = ", a.alloc_str(made)]
        };
        self.once.iter().enumerate().map(declare).collect()
    }

    /// A literal as Go: a `BigInt` one is read from its digits once.
    pub(super) fn literal(&mut self, lit: &Lit) -> String {
        let go = literal(lit);
        if matches!(lit, Lit::BigInt(_)) {
            self.once(&go).to_owned()
        } else {
            go
        }
    }
}
