//! A `match` on a value of `Errors ε`, the errors of a row: an arm headed by
//! an error `E` matches the values of the element `E ῡ` of `ε`, and the
//! elements of the row are the constructors its arms have to cover. The
//! match removes no error from a row: it only takes one value apart.

use super::Checker;
use super::env::Found;
use super::exhaust::arity;
use super::handle::{arm_pattern, failed_pattern};
use super::handlers::catch_all;
use super::patterns::{Ctor, is_pattern};
use super::ty::{Norm, Row, Type};
use crate::diagnostics::codes;
use crate::iface::{Entity, Namespace};
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
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
    /// `None` when `ty` is no such row, or a row still unknown, which the
    /// error then joins as `Errors e ~ E` makes it. An error outside a
    /// known row is reported, and its pattern matches nothing, as in a row
    /// already reported.
    pub(super) fn error_element(&mut self, built: &Type, ty: &Type, node: &Node) -> Option<Type> {
        let label = self.error_label(built)?;
        let norm = self.errors_row(ty)?;
        if let Some(element) = element(&norm.entries, &label) {
            return Some(element.clone());
        }
        if norm.tails.iter().any(|t| matches!(t, Type::Error)) {
            return Some(Type::Error);
        }
        if norm.tails.iter().any(|t| t.inference_meta_root().is_some()) {
            return None;
        }
        let (error, row) = (self.show(built), self.show(ty));
        self.error(
            codes::T003,
            self.span(node),
            format!("`{error}` is not one of the errors of `{row}`"),
        );
        Some(Type::Error)
    }

    /// `match e with …` with `e : Errors ε`, where `ε` is not known yet:
    /// without an arm for every error, `ε` is exactly the errors the arms
    /// are headed by, its only solution. An arm for every error tells
    /// nothing of `ε`, which then has to be known.
    pub(super) fn errors_of_arms(&mut self, node: &Node, scrutinee: &Type) {
        let Some(norm) = self.errors_row(scrutinee) else {
            return;
        };
        let [tail] = &norm.tails[..] else {
            return;
        };
        if tail.inference_meta_root().is_none() {
            return;
        }
        let (mut heads, mut every) = (Vec::new(), None);
        for arm in node.nodes().filter(|n| n.kind == K::MatchArm) {
            let Some(pattern) = arm_pattern(arm) else {
                continue;
            };
            let pattern = failed_pattern(pattern, self.text).unwrap_or(pattern);
            if !catch_all(pattern) {
                self.error_heads(pattern, &norm.entries, &mut heads);
            } else if !arm.nodes().any(|n| n.kind == K::Guard) {
                every = Some(pattern);
            }
        }
        if heads.is_empty() {
            return;
        }
        let row = if let Some(pattern) = every {
            self.error(
                codes::T057,
                self.span(pattern),
                "this arm matches every error, so the arms do not tell the errors of the value: annotate it",
            );
            // what the row should be is not known: nothing else is reported of it
            Type::Error
        } else {
            Type::Row(Row::closed(heads))
        };
        let _ = self.bind_inference(tail, &row);
    }

    /// The errors `pattern` is headed by that `known` lacks, each at
    /// unknown arguments.
    fn error_heads(
        &mut self,
        pattern: &Node,
        known: &[(Elem, Type)],
        heads: &mut Vec<(Elem, Type)>,
    ) {
        if matches!(pattern.kind, K::OrPat | K::ParenPat) {
            for alternative in pattern.nodes().filter(|n| is_pattern(n.kind)) {
                self.error_heads(alternative, known, heads);
            }
            return;
        }
        if pattern.kind != K::ConPat {
            return;
        }
        // the arms report a name that is missing when they are checked
        let mark = self.diagnostics.len();
        let entity = self.pattern_constructor(pattern).and_then(|(_, e)| e);
        self.diagnostics.truncate(mark);
        let Some(scheme) = entity.and_then(|e| self.env.value(&e).cloned()) else {
            return;
        };
        let mut built = self.instantiate(&scheme, self.span(pattern));
        while let Type::Fun { result, .. } = self.whnf(&built) {
            built = *result;
        }
        let Some(head) = self.error_label(&built) else {
            return;
        };
        if element(known, &head).is_none() && element(heads, &head).is_none() {
            heads.push((head, built));
        }
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
