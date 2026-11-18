//! A `match` on a value of `Errors ε`, the errors of a row: an arm headed by
//! an error `E` matches the values of the element `E ῡ` of `ε`, and the
//! elements of the row are the constructors its arms have to cover. The
//! match removes no error from a row: it only takes one value apart. A
//! term of an error has the type of the errors of a row that names it
//! (Errors-In).

use std::slice;

use super::Checker;
use super::env::Found;
use super::exhaust::arity;
use super::gadt::columns::Coverage;
use super::patterns::{Ctor, Pat};
use super::ty::{Norm, Row, Type};
use crate::diagnostics::codes;
use crate::iface::{Entity, Namespace};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::types::Elem;
use crate::types::lang::Lang;

impl Checker<'_> {
    /// The row of `ty` when it is `Errors ε`.
    #[inline(never)]
    pub(in crate::types) fn errors_row(&self, ty: &Type) -> Option<Norm> {
        let t = self.whnf(ty);
        let (head, args) = t.spine();
        match args[..] {
            [row] if head.is_lang(Lang::Errors) => Some(self.store.normalize(row)),
            _ => None,
        }
    }

    /// What a constructor pattern that builds `built` matches in a value of
    /// `ty`: for `Errors ε` and an error `E`, the element `E ῡ` of `ε`.
    /// `None` when `ty` is no such row. In a pattern of a `match`, the
    /// error joins a row still unknown, which stays open behind it until
    /// [`Self::close_error_rows`]; elsewhere `None`, and the row becomes
    /// the error, as `Errors e ~ E` makes it. An error outside a known row
    /// is reported, and its pattern matches nothing, as in a row already
    /// reported.
    pub(super) fn error_element(&mut self, built: &Type, ty: &Type, node: &Node) -> Option<Type> {
        let label = self.error_label(built)?;
        let norm = self.errors_row(ty)?;
        if let Some(element) = element(&norm.entries, &label) {
            return Some(element.clone());
        }
        if norm.tails.iter().any(|t| matches!(t, Type::Error)) {
            return Some(Type::Error);
        }
        if let Some(tail) = norm
            .tails
            .iter()
            .find(|t| t.inference_meta_root().is_some())
        {
            self.error_rows.as_ref()?;
            let rest = self.store.fresh_error_row();
            let row = Row {
                entries: vec![(label, built.clone())],
                tail: Some(Box::new(rest.clone())),
                ..Row::default()
            };
            let _ = self.bind_inference(tail, &Type::Row(row));
            self.error_rows.get_or_insert_default().push(rest);
            return Some(built.clone());
        }
        let (error, row) = (self.show(built), self.show(ty));
        self.error(
            codes::T003,
            self.span(node),
            format!("`{error}` is not one of the errors of `{row}`"),
        );
        Some(Type::Error)
    }

    /// After the arms of a `match`: the rows its patterns added errors to
    /// stay open when the arms without `when` cover the value with them
    /// open, which leaves them to the rest of the program, as the
    /// `raised ⊆ caught` of a `try`. Otherwise `Exh` needs them closed, and
    /// the errors named are their only solution.
    pub(super) fn close_error_rows(
        &mut self,
        rows: Vec<Type>,
        arms: &[(Pat, bool, Span)],
        scrutinee: &Type,
    ) {
        if rows.is_empty() {
            return;
        }
        let unguarded: Vec<Vec<Pat>> = arms
            .iter()
            .filter(|(_, guarded, _)| !guarded)
            .map(|(pat, _, _)| vec![pat.clone()])
            .collect();
        let tys = slice::from_ref(scrutinee);
        if self
            .useful_at(&unguarded, &[Pat::Wild], tys, &Coverage::new(self))
            .is_none()
        {
            return;
        }
        for row in rows {
            for tail in self.store.normalize(&row).tails {
                if tail.inference_meta_root().is_some() {
                    let _ = self.bind_inference(&tail, &Type::empty_row());
                }
            }
        }
    }

    /// (Errors-In): a term of an error `E ῡ` checked against `Errors ε`
    /// whose row names `E` has the element `E ῡ` of `ε` as its type. `None`
    /// when the rule does not apply: a row of one error is that error, and
    /// a row not known yet becomes the error, as `Errors e ~ E` makes it.
    /// `Some(false)` after a diagnostic.
    pub(super) fn errors_in(&mut self, actual: &Type, expected: &Type, span: Span) -> Option<bool> {
        let norm = self.errors_row(expected)?;
        let actual = self.whnf(actual);
        let label = self.error_label(&actual)?;
        if let Some(element) = element(&norm.entries, &label).cloned() {
            return Some(match self.unify(&actual, &element) {
                Ok(()) => true,
                Err(mismatch) => {
                    self.mismatch(&mismatch, &actual, expected, span);
                    false
                }
            });
        }
        let unknown = norm
            .tails
            .iter()
            .any(|t| matches!(t, Type::Error) || t.inference_meta_root().is_some());
        if unknown || norm.entries.is_empty() || (norm.tails.is_empty() && norm.entries.len() < 2) {
            return None;
        }
        let (error, row) = (self.show(&actual), self.show(expected));
        self.error(
            codes::T003,
            span,
            format!("`{error}` is not one of the errors of `{row}`"),
        );
        Some(false)
    }

    /// The constructors of a column of `Errors ε` with the row `row`: one
    /// for each error of a closed `ε`; `None` for a row that may hold others.
    pub(super) fn row_constructors(&self, row: &Norm) -> Option<Vec<(Ctor, usize)>> {
        if !row.tails.is_empty() {
            return None;
        }
        let mut ctors = Vec::with_capacity(row.entries.len());
        for (_, element) in &row.entries {
            let Type::Con { module, name } = element.spine().0 else {
                return None;
            };
            let entity = Entity {
                module: module.clone(),
                namespace: Namespace::Constructor,
                name: name.clone(),
            };
            let arity = self.env.value(&entity).map_or(0, arity);
            let ctor = Ctor::Variant {
                module: module.clone(),
                ty: name.clone(),
                name: name.clone(),
            };
            ctors.push((ctor, arity));
        }
        Some(ctors)
    }

    /// A constructor's name as this module writes it: qualified when its
    /// plain name means another constructor or none.
    pub(super) fn written_constructor(&self, module: &str, name: &str) -> String {
        let plain = matches!(
            self.env.find(Namespace::Constructor, name),
            Found::Entity(e) if e.module == module
        );
        match self.env.qualifier_of(module) {
            Some(q) if !plain => format!("{q}.{name}"),
            _ => name.to_owned(),
        }
    }
}

/// What a column of `Errors ε` with the row `row` is once specialized to
/// `ctor`: the element of `ε` an error's constructor matches, or `Error`
/// when `ε` may hold the error, or for a constructor of no error; `None`
/// when no value of `ε` is built by it, a closed `ε` that lacks it.
pub(in crate::types) fn error_column(ctor: &Ctor, row: &Norm) -> Option<Type> {
    let Ctor::Variant {
        module, ty: error, ..
    } = ctor
    else {
        return Some(Type::Error);
    };
    match element(&row.entries, &Elem::Named(format!("{module}.{error}"))) {
        Some(element) => Some(element.clone()),
        None if row.tails.is_empty() => None,
        None => Some(Type::Error),
    }
}

/// The element of a row labelled `label`.
#[inline(never)]
fn element<'e>(entries: &'e [(Elem, Type)], label: &Elem) -> Option<&'e Type> {
    entries.iter().find(|(l, _)| l == label).map(|(_, t)| t)
}

#[cfg(test)]
#[path = "error_match_tests.rs"]
mod tests;
