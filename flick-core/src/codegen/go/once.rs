//! Values a module makes once. A `BigInt` literal, a constructor without
//! fields, the labels of a record and an adapter of stages read nothing but
//! the runtime, so each is a package variable, made when the program
//! starts, and not made again every time the code that names it runs.

use std::iter;

use flick_pretty::{Document, docvec};

use super::expr::literal;
use super::layout::{Code, call};
use super::{Gen, names};
use crate::ir::{Expr, Lit};

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

    /// A literal as an expression.
    pub(super) fn lit(&mut self, lit: &Lit) -> Code<'a> {
        let go = self.literal(lit);
        self.code(&go)
    }

    /// A record of `fields`, or `base` with them. Where each label goes in a
    /// record is found once for the literal, not for every record it makes.
    pub(super) fn record(&mut self, base: Option<&Expr>, fields: &[(String, Expr)]) -> Code<'a> {
        let a = self.a;
        let mut all: Vec<&Expr> = base.into_iter().collect();
        all.extend(fields.iter().map(|(_, v)| v));
        let mut go = self.ordered(&all);
        let base = base.map(|_| go.remove(0));
        let mut labels: Vec<&str> = fields.iter().map(|(label, _)| label.as_str()).collect();
        labels.sort_unstable();
        labels.dedup();
        let record = if fields.is_empty() || labels.len() < fields.len() {
            // a label written twice is its last value: the runtime sorts that out
            call(a, "flickrt.Rec", self.entries(fields, go))
        } else {
            let written: Vec<String> = fields.iter().map(|(l, _)| names::string(l)).collect();
            let shape = self.once(&format!("flickrt.ShapeOf({})", written.join(", ")));
            let values = go.iter().map(|v| v.doc);
            call(a, "flickrt.Shaped", iter::once(a.text(shape)).chain(values))
        };
        match base {
            Some(b) => call(a, "flickrt.Update", [b.doc, record]).into(),
            None => record.into(),
        }
    }
}
