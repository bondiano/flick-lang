//! Values a module makes once. A `BigInt` literal, a constructor without
//! fields, the labels of a record and an adapter of stages read nothing but
//! the runtime, so each is a package variable, made when the program
//! starts, and not made again every time the code that names it runs.

use std::cmp::Ordering;
use std::iter;

use flick_pretty::{Document, docvec};

use super::expr::literal;
use super::layout::{Code, call};
use super::shape::builds_settled;
use super::{Gen, names};
use flick_ir::Dict;
use flick_ir::{Expr, Lit};

/// How the Go of a value made on first use begins.
const LAZY: &str = "flickrt.Lazy(";

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
            let made = a.alloc_str(made);
            if made.starts_with(LAZY) {
                // set as the package starts, not where it is declared: Go
                // refuses a variable whose value names a function that
                // names the variable, as an instance of a recursive type does
                let set = docvec![a, "func init() { ", name, " = ", made, " }"];
                docvec![a, "var ", name, " func() flickrt.Value", a.lines(2), set]
            } else {
                docvec![a, "var ", name, " = ", made]
            }
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
    /// Structural record literals evaluate field expressions in source order
    /// and construct their shape in canonical label order so permutations share
    /// one shape and written order is not observable.
    pub(super) fn record_erased(
        &mut self,
        base: Option<&Expr>,
        fields: &[(String, Expr)],
    ) -> Code<'a> {
        let a = self.a;
        let mut all: Vec<&Expr> = base.into_iter().collect();
        all.extend(fields.iter().map(|(_, v)| v));
        let mut go = self.ordered(&all);
        let base = base.map(|_| go.remove(0));
        let mut labels: Vec<&str> = fields.iter().map(|(label, _)| label.as_str()).collect();
        labels.sort_unstable();
        labels.dedup();
        if fields.is_empty() || labels.len() < fields.len() {
            // a label written twice is its last value: the runtime sorts that out
            let record = call(a, "flickrt.Rec", self.entries(fields, go));
            return match base {
                Some(b) => call(a, "flickrt.Update", [b.doc, record]).into(),
                None => record.into(),
            };
        }
        if let Some(b) = base {
            // the values go into the copy of the record in place, preserving the
            // base record's canonical key order while evaluating replacements in source order
            let written: Vec<String> = fields.iter().map(|(l, _)| names::string(l)).collect();
            let shape = self.once(&format!("flickrt.ShapeOf({})", written.join(", ")));
            let values = iter::once(a.text(shape)).chain(go.iter().map(|v| v.doc));
            return call(a, "flickrt.With", iter::once(b.doc).chain(values)).into();
        }
        let mut order: Vec<usize> = (0..fields.len()).collect();
        order.sort_by(|&i, &j| canonical_key_cmp(&fields[i].0, &fields[j].0));
        let needs_spill = (0..order.len()).any(|pos| {
            let later = order[pos];
            !go[later].atomic
                && order[pos + 1..]
                    .iter()
                    .any(|&earlier| earlier < later && !go[earlier].atomic)
        });
        if needs_spill {
            go = go.into_iter().map(|v| self.spill(v)).collect();
        }
        let canonical: Vec<String> = order.iter().map(|&i| names::string(&fields[i].0)).collect();
        let shape = self.once(&format!("flickrt.ShapeOf({})", canonical.join(", ")));
        let values = iter::once(a.text(shape)).chain(order.iter().map(|&i| go[i].doc));
        call(a, "flickrt.Shaped", values).into()
    }

    /// A dictionary built from others that names nothing of the code around
    /// it and is the same whoever builds it, as `Ord (Option Int)` is: built
    /// the first time it is needed, not every time. It reads the instances
    /// of modules, which are made as the program starts, so it cannot be
    /// made before them as a literal is.
    pub(super) fn dict_once(&mut self, dict: &Dict) -> Option<Code<'a>> {
        let built = matches!(dict, Dict::Instance { args, .. } if !args.is_empty())
            || matches!(dict, Dict::Structural { .. });
        let settled = self.settled?;
        if !built || !closed(dict) || !builds_settled(dict, settled, self.shapes) {
            return None;
        }
        // printed as any dictionary is, with no fiber to build it in
        let fiber = self.context.replace("nil");
        let made = self.dict_made(dict).doc.to_pretty_string(usize::MAX / 2);
        self.context = fiber;
        let lazy = format!("{LAZY}func() flickrt.Value {{ return {made} }})");
        let name = self.once(&lazy);
        Some(call(self.a, name, iter::empty::<Document<'a>>()).into())
    }
}

/// Whether `dict` names no dictionary given to the code around it and no
/// local of that code, so that it can be made outside it.
fn closed(dict: &Dict) -> bool {
    match dict {
        Dict::Instance { args, .. } => args.iter().all(closed),
        Dict::Structural { fields, .. } => fields.iter().all(closed),
        Dict::Super { dict, .. } => closed(dict),
        Dict::Primitive { .. } | Dict::Site { .. } => true,
        _ => false,
    }
}

/// Canonical structural record key order: JS array indices first in ascending
/// numeric order, then all other keys in UTF-16 code-unit order.
pub(super) fn canonical_key_cmp(a: &str, b: &str) -> Ordering {
    match (array_index(a), array_index(b)) {
        (Some(na), Some(nb)) => na.cmp(&nb),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a.encode_utf16().cmp(b.encode_utf16()),
    }
}

fn array_index(key: &str) -> Option<u32> {
    let first = *key.as_bytes().first()?;
    if !first.is_ascii_digit() {
        return None;
    }
    let n: u32 = key.parse().ok()?;
    (n != u32::MAX && n.to_string() == key).then_some(n)
}
