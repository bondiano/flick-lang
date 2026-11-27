//! Scheme compatibility checking for hot swaps and typed migrations
//! (`docs/formal/hot-swap.md`, `formal/Formal/HotSwap.lean`).
//!
//! A swap replacing a global cell of scheme `σ_old` with a closed value of
//! scheme `σ_new` is compatible iff `σ_new` subsumes `σ_old` (`σ_new ≤ σ_old`):
//! every valid instantiation of `σ_old` under its region/outlives facts can be
//! satisfied by an instantiation of `σ_new` whose body is a live subtype of
//! `σ_old`'s instantiated body. Consequently:
//! - generalizing a scheme is accepted; narrowing is rejected;
//! - covariant effect/service/error rows may shrink (or become pure), never widen;
//! - contravariant parameter capture/region qualifiers may widen, never narrow;
//! - covariant callable capture/region qualifiers may shrink, never widen.

use std::error::Error;
use std::fmt;

use super::{Check, Checker, TypeError, error};
use crate::decls::Decls;
use crate::subst::{any, free_vars};
use crate::syntax::{Path, Term};
use crate::types::{Scheme, Stage, Type};

/// Error returned when a hot-swap scheme or value fails compatibility checking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompatibilityError {
    /// The formal rule whose premise failed.
    pub rule: &'static str,
    /// Human-readable reason for rejection.
    pub message: String,
    /// String rendering of the previous scheme, when available.
    pub old_scheme: String,
    /// String rendering of the proposed replacement scheme, when available.
    pub new_scheme: String,
}

impl fmt::Display for CompatibilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "({}) incompatible hot swap (`{}` does not subsume `{}`): {}",
            self.rule, self.new_scheme, self.old_scheme, self.message
        )
    }
}

impl Error for CompatibilityError {}

impl CompatibilityError {
    fn from_type_error(err: TypeError, new_scheme: &Scheme, old_scheme: &Scheme) -> Self {
        Self {
            rule: err.rule,
            message: err.message,
            old_scheme: format_scheme(old_scheme),
            new_scheme: format_scheme(new_scheme),
        }
    }
}

/// Renders a [`Scheme`] concisely for diagnostics and compatibility errors.
#[must_use]
pub fn format_scheme(scheme: &Scheme) -> String {
    if scheme.vars.is_empty() {
        scheme.body.to_string()
    } else {
        let vars: Vec<&str> = scheme.vars.iter().map(|(n, _)| &**n).collect();
        format!("forall {}. {}", vars.join(" "), scheme.body)
    }
}

/// Returns `true` iff `term` is a closed source term containing no free local
/// variables and no runtime-only forms (store locations, regions, handles,
/// handlers, continuations, or runtime environments).
#[must_use]
pub fn is_closed_source_term(term: &Term) -> bool {
    free_vars(term).is_empty() && !has_runtime_form(term)
}

/// Returns `true` iff `term` is a closed source value suitable for replacing a
/// global cell in a hot swap.
#[must_use]
pub fn is_closed_source_value(term: &Term) -> bool {
    term.is_value() && is_closed_source_term(term)
}

fn has_runtime_form(term: &Term) -> bool {
    any(term, &|t| {
        matches!(
            t,
            Term::Loc(_)
                | Term::Own(_)
                | Term::Cap(..)
                | Term::Inside(..)
                | Term::Settle(..)
                | Term::Env(_)
                | Term::Bind(..)
                | Term::Join(..)
                | Term::With(..)
                | Term::Handle(..)
                | Term::Raise(_)
                | Term::Cont(_)
                | Term::Signal(_)
                | Term::OwnerCandidate(_)
        )
    })
}

impl Checker<'_> {
    /// Checks `new_scheme ≤ old_scheme` (scheme subsumption for hot swap):
    /// skolemizes `old_scheme` (assuming its facts and bounds as givens),
    /// instantiates `new_scheme` with fresh metas (posting its facts and bounds
    /// as obligations), checks live subtyping `new_body ≤ old_body`, and
    /// settles all row and region/capture constraints.
    ///
    /// # Errors
    /// Returns a [`TypeError`] if `new_scheme` narrows a polymorphic binder,
    /// widens an effect/service/error row, narrows a parameter capture/region
    /// qualifier, widens a callable capture/region qualifier, or violates any
    /// outlives/variance constraint.
    pub fn check_scheme_subsumes(&mut self, new_scheme: &Scheme, old_scheme: &Scheme) -> Check<()> {
        let old_live = self.live_scheme(old_scheme);
        let new_live = self.live_scheme(new_scheme);
        let (old_body, _rigids) = self.skolemize(&old_live, false);
        let new_body = self.instantiate(&new_live);
        self.subsume(&new_body, &old_body)?;
        self.finish()
    }
}

/// Checks whether replacing a global cell of scheme `old_scheme` with one of
/// scheme `new_scheme` is hot-swap-compatible under `decls`.
///
/// Exposed as a standalone function so downstream VM/compiler consumers
/// (e.g. `#990`) can enforce the exact same scheme-subsumption rule as the
/// reference model.
///
/// # Errors
/// Returns [`CompatibilityError`] describing the violated typing, effect-row,
/// or capture/region rule when `new_scheme` does not subsume `old_scheme`.
pub fn check_scheme_compatibility(
    decls: &Decls,
    new_scheme: &Scheme,
    old_scheme: &Scheme,
) -> Result<(), CompatibilityError> {
    let mut checker = Checker::new(decls);
    checker
        .check_scheme_subsumes(new_scheme, old_scheme)
        .map_err(|err| CompatibilityError::from_type_error(err, new_scheme, old_scheme))
}

/// Checks that `new_value` is a closed source value that type-checks purely
/// against `new_scheme` under `decls` (with `target` bound to `new_scheme` for
/// recursive definitions).
///
/// # Errors
/// Returns a [`TypeError`] if `new_value` is not a closed source value or fails
/// `check_top` against `new_scheme`.
pub fn check_swap_value(
    decls: &Decls,
    target: &Path,
    new_scheme: &Scheme,
    new_value: &Term,
) -> Check<()> {
    if !is_closed_source_value(new_value) {
        return error(
            "Swap",
            format!("replacement for `{target}` must be a closed source value"),
        );
    }
    let mut checker = Checker::new(decls);
    checker.module = target.module.clone();
    checker.own = Some((target.clone(), new_scheme.clone()));
    checker.check_top(new_value, new_scheme)
}

/// Checks that `migrate_fn` is a closed pure function of type
/// `old_ty -> new_ty` (`Stage::Pure`) under `decls`.
///
/// # Errors
/// Returns a [`TypeError`] if `migrate_fn` is open, contains runtime forms, is
/// effectful, or does not match `old_ty -> new_ty`.
pub fn check_migration_fn(
    decls: &Decls,
    old_ty: &Type,
    new_ty: &Type,
    migrate_fn: &Term,
) -> Check<()> {
    if !is_closed_source_term(migrate_fn) {
        return error(
            "Migrate",
            "migration function must be a closed pure source term",
        );
    }
    let expected = Scheme::new(
        Vec::new(),
        Type::fun(old_ty.clone(), Stage::Pure, new_ty.clone()),
    );
    let mut checker = Checker::new(decls);
    checker.check_top(migrate_fn, &expected)
}
