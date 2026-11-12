//! Children and errors that leave a scope: a child may hold only regions
//! that outlive its owner, and an error carries no region at all.

use crate::diagnostics::{Diagnostic, codes};
use crate::source::Span;
use crate::types::Checker;
use crate::types::ty::{Norm, Type};

/// `Outlives c s`: the captures `c` live at least as long as the region `s`.
#[derive(Debug, Clone)]
pub struct Outlives {
    pub captures: Type,
    pub owner: Type,
    pub givens: Vec<Type>,
    pub span: Span,
}

impl Checker<'_> {
    /// Whether the scope `outer` is `inner` or around it.
    pub(super) fn encloses(&self, outer: usize, inner: usize) -> bool {
        let mut at = Some(inner);
        while let Some(i) = at {
            if i == outer {
                return true;
            }
            at = self.cx.regions.scopes[i].parent;
        }
        false
    }

    pub(super) fn scope_index(&self, t: &Type) -> Option<usize> {
        match t {
            Type::Skolem(s) => self.cx.regions.scopes.iter().position(|x| x.region == *s),
            _ => None,
        }
    }

    /// A child registered with `owner` runs until the owner's scope closes,
    /// so it may hold only regions that end no earlier.
    pub(super) fn check_outlives(&mut self, o: &Outlives) {
        let owner = self.store.resolve(&o.owner);
        let owner_scope = self.scope_index(&owner);
        let mut held = Norm::default();
        self.capture_row(&o.captures, &mut held);
        for (label, atom) in held.entries {
            if atom == owner {
                continue;
            }
            let fits = match self.scope_index(&atom) {
                Some(scope) => owner_scope.is_some_and(|owner| self.encloses(scope, owner)),
                // what the signature names outlives every scope inside the body
                None => {
                    owner_scope.is_some()
                        || o.givens.iter().any(|g| match g.spine().1[..] {
                            [c, s] if self.store.resolve(s) == owner => {
                                let mut allowed = Norm::default();
                                self.capture_row(c, &mut allowed);
                                allowed.get(&label).is_some()
                            }
                            _ => false,
                        })
                }
            };
            if !fits {
                let (region, scope) = self.describe(&atom);
                let (owner_name, _) = self.describe(&owner);
                let mut diagnostic = Diagnostic::error(
                    codes::G003,
                    format!(
                        "the child holds {region}, which may end before its owner {owner_name}: a child runs until its owner's scope closes"
                    ),
                    o.span,
                );
                if let Some(span) = scope {
                    diagnostic = diagnostic.with_related(span, "the region's scope ends here");
                }
                self.diagnostics.push(diagnostic);
                return;
            }
        }
    }

    /// An error carries data out of every scope, even when a handler inside
    /// the scope catches it: its payload holds no region.
    pub(super) fn check_raised(&mut self, errors: &Type, span: Span) {
        for (_, error) in self.store.normalize(errors).entries {
            let regions = self.captures_of(&error);
            if let Some((_, atom)) = regions
                .entries
                .iter()
                .find(|(_, t)| self.scope_index(t).is_some())
            {
                let (region, _) = self.describe(atom);
                let shown = self.show_exact(&error);
                self.error(
                    codes::E005,
                    span,
                    format!(
                        "the error `{shown}` would carry {region}; an error carries data such as a path, an id or a copy"
                    ),
                );
                return;
            }
        }
    }
}
