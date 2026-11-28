//! Regions and captures. `scope s { … }` makes a fresh region `s`: a rigid
//! type one level deeper than the code around the scope, so unification
//! refuses to let any type outside the scope mention it. The region is the
//! owner of the resources acquired inside: an operation that needs one asks
//! for the service `Scope s`, which the innermost `scope` provides.
//!
//! A value may also hold a region its type does not show: a closure or a
//! `task` that uses a scoped value. Such hidden captures belong in the
//! qualifier of its type, `T^{s}`. A closure's qualifier is inferred from the
//! locals it uses and must fit the qualifier its position allows; a bare type
//! allows none. These constraints are solved with the effect rows, once the
//! declaration is inferred (see [`solve`]).

mod applied;
mod bounds;
mod captures;
mod compare;
mod data;
mod defer;
mod hidden_rows;
pub use defer::{DeferBarrier, DeferOwner};
pub use hidden_rows::{is_elided_row, strip_elided};
mod live;
mod live_rows;
mod outlives;
mod provided;
mod provided_diag;
mod qualifiers;
mod reader;
mod scopes;
mod solve;
mod unfold;

pub use live::{has_live, strip_live};
pub use outlives::Outlives;
pub use provided::Provide;
pub use reader::Reader;
pub(in crate::types) use unfold::{AliasKey, Unfold, leave_alias};

use super::Checker;
use super::kinds::Kind;
use super::ty::{Norm, Type, capture_label};
use super::typing::Range;
use super::unify::Mismatch;
use crate::diagnostics;
use crate::diagnostics::codes;
use crate::source::Span;
use crate::types::Elem;
use crate::types::Elided;
use crate::types::lang::{self, Lang};
use std::ops::{Deref, DerefMut};

/// The service a `scope` provides to the operations that acquire resources.
pub fn scope() -> &'static str {
    &lang::item(Lang::Scope).label
}

/// Regions and captures of the declaration being checked.
#[derive(Debug, Default)]
pub struct State {
    pub holds: Vec<DeferredHold>,
    pub scopes: Vec<Scope>,
    /// The scopes around the expression being checked, innermost last.
    pub open: Vec<usize>,
    /// Explicit lexical `scope` owners and deferred execution boundaries for
    /// `defer`/`errdefer`, innermost last.
    pub defer_owners: Vec<DeferOwner>,
    /// The scopes and `provide`s around it, innermost last: what a new one
    /// nests in.
    pub nest: Vec<usize>,
    /// The `provide`s around it, innermost last.
    pub provides: Vec<Provide>,
    /// When `Layer.run layer (fun () -> …)` checks its immediate callback, the
    /// `provides` index that callback's stage runs inside.
    pub immediate_provide: Option<usize>,
    /// `Fiber.Provided` constraints until their call has its arguments,
    /// and those whose row may still grow (`provided.rs`).
    pub provided: Vec<provided::Want>,
    pub provided_late: Vec<provided::Want>,
    /// Whether `pass_on` placed a `Provided` obligation into a stage row,
    /// so `settle_performed` grows open rows before checking them.
    pub passed_provided: bool,
    /// A `provide` compared with an owner not known yet: its scope, the
    /// owner, the service, where, and an optional selected-owner span for `defer`.
    pub late_owners: Vec<(usize, Type, Elem, Span, Option<Span>)>,
    /// The locals used so far, by index into `Checker::locals`.
    pub uses: Vec<usize>,
    pub outlives: Vec<outlives::Outlives>,
    /// Error rows raised, whose payloads must hold no region.
    pub raised: Vec<(Type, Span)>,
    /// What each call in a `tail` arm needs, and the rows it may suspend through.
    pub tail_calls: Vec<(Type, Vec<Type>, Span)>,
    /// Calls inside a `scope` that need `Scope`, until their innermost
    /// scope ends.
    pub needs: Vec<Need>,
    /// While an `Eff` argument is checked: tasks in it that acquire into a
    /// region not known yet, with the innermost scope, until the call has
    /// all its arguments, since a later one may name the region (`docs/formal/regions.md#scope-region-of-a-task-argument-is-resolved-after-the-call`).
    pub defaults: Option<Vec<(Type, usize)>>,
    /// The functions around the expression being checked, innermost last,
    /// with the reader each lends to its lazy `for`s.
    pub readers: Vec<Reader>,
    /// Every reader region made so far, for diagnostics.
    pub lent: Vec<u32>,
    /// Kinds of rigid signature variables, including nested `forall`s.
    pub rigid_kinds: Vec<(u32, Kind)>,
    /// The rigid variables of the top-level signatures being checked: the
    /// only ones a constraint a fix adds to such a signature can name.
    pub signed: Vec<u32>,
    /// What the type variable elements of error rows, `{t | e}`, were
    /// instantiated to, and where: each must be an error type.
    pub error_elements: Vec<(Type, Span)>,
    /// The span of the value being subsumed: qualifiers that wait for the
    /// captures of a type not inferred yet are compared once it is.
    pub deferring: Option<Span>,
    /// Qualifiers to compare at the end, and where they were.
    pub deferred: Vec<(Type, Type, Span)>,
    /// Values that hold captures their position does not allow: reported
    /// at the end, unless the hold on them named what escapes.
    pub held: Vec<(Mismatch, Type, Type, Span)>,
}

#[derive(Debug, Clone)]
pub struct Need {
    pub region: Type,
    /// The innermost scope around the call.
    pub inner: usize,
    /// The scopes around the call opened in its stage, outermost first.
    pub own: Vec<usize>,
    pub span: Span,
    pub site: usize,
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub region: u32,
    /// The `scope` keyword.
    pub span: Span,
    /// The whole `scope` expression.
    pub node: Range,
    /// Optional explicit owner name written on `scope name { … }`.
    #[cfg(feature = "ide")]
    pub name: Option<String>,
    /// The services of the stage around the scope.
    pub outer: Type,
    pub parent: Option<usize>,
    /// The region of `resume` in the arms of a handler, not a `scope`'s.
    pub handler: bool,
}

/// Values a closure holds, by name, with their types.
pub type Held = Vec<(Option<String>, Type)>;

/// The regions and variables a type shows openly.
#[derive(Debug, Clone)]
pub enum Visible {
    /// Those of a closure's own type, once inferred.
    Late(Type),
    /// A refinement may forget an abstract upper bound, never a concrete region.
    Refined(Type),
    /// Those of an expected type as written, before the value's type is
    /// unified into it: `a^c` shows nothing of what `a` stands for.
    Now(Vec<Elem>),
    /// None of the regions of the value: a bound on a variable bounds the
    /// whole value it stands for (`docs/formal/regions.md` (Inst)), while a
    /// type variable in it is no use of what it stands for.
    Regions,
}

/// What a value holds must fit the qualifier `bound`.
#[derive(Debug, Clone)]
pub struct Hold {
    /// The values held: the locals a closure uses, or one value passed on.
    pub held: Held,
    /// What the holder's type shows is held openly, not hidden.
    pub visible: Visible,
    pub bound: Type,
    /// The type whose qualifier is the bound, when the position gave one.
    pub expected: Option<Type>,
    /// The bound is written out, not a qualifier being inferred.
    pub written: bool,
    pub what: &'static str,
    pub span: Span,
}

/// A constraint together with the GADT assumptions at its creation point.
#[derive(Debug, Clone)]
pub struct DeferredHold {
    hold: Hold,
    givens: Vec<Type>,
}

impl Deref for DeferredHold {
    type Target = Hold;
    fn deref(&self) -> &Hold {
        &self.hold
    }
}

impl DerefMut for DeferredHold {
    fn deref_mut(&mut self) -> &mut Hold {
        &mut self.hold
    }
}

/// The types whose values hold nothing of their arguments: a witness
/// describes values of its type, holding none of them.
pub(super) const HOLD_NOTHING: [Lang; 6] = [
    Lang::Layer,
    Lang::ServiceTag,
    Lang::ServiceImpl,
    Lang::Schema,
    Lang::Url,
    Lang::Form,
];

impl Checker<'_> {
    /// What a value held under the bound of `ty^c` shows: what the bounded
    /// type shows, as `a` of `List a`, is no hold, as a type argument carries
    /// its own qualifier; a bound on a variable bounds the whole value it
    /// stands for, `a^{}` with `a := File s` too (`regions.md` (Inst)).
    pub(super) fn bounded_visible(&self, ty: &Type) -> Visible {
        match self.store.resolve(ty) {
            Type::Meta(_) | Type::Var(_) | Type::Skolem(_) => Visible::Regions,
            _ => Visible::Late(ty.clone()),
        }
    }

    pub fn hold(&mut self, hold: Hold) {
        let givens = self.equalities_in_scope();
        self.cx.regions.holds.push(DeferredHold { hold, givens });
    }

    /// Marks where a closure starts: the uses after it are its body's.
    pub fn uses_mark(&self) -> usize {
        self.cx.regions.uses.len()
    }

    /// The locals below `locals` that were used since `uses`, by name.
    pub fn held_since(&self, uses: usize, locals: usize) -> Held {
        let mut seen: Vec<usize> = Vec::new();
        for &i in &self.cx.regions.uses[uses..] {
            if i < locals && !seen.contains(&i) {
                seen.push(i);
            }
        }
        seen.into_iter()
            .map(|i| {
                (
                    Some(self.cx.locals[i].name.clone()),
                    self.cx.locals[i].ty.clone(),
                )
            })
            .collect()
    }

    /// The type `ty` of a closure that holds `held`: its qualifier is that
    /// of the `expected` type, or inferred when the position gave none.
    pub fn close_over(
        &mut self,
        ty: Type,
        held: Held,
        expected: Option<Type>,
        what: &'static str,
        span: Span,
    ) -> Type {
        // under `live.rs` no qualifier says what a closure holds, but the
        // reader of a lazy `for`, which that discipline does not know
        if held.is_empty() || !self.holds_reader(&held) {
            return ty;
        }
        if let Some(expected) = &expected
            && matches!(self.bound_head(expected), Type::Bounded { .. })
        {
            let identity = self.capture_identity(expected);
            let (bare, captures) = self.whnf_captured(&identity);
            if !matches!(bare, Type::Meta(_)) {
                let visible = if captures.is_some() {
                    Visible::Now(Vec::new())
                } else {
                    Visible::Late(identity.clone())
                };
                self.hold(Hold {
                    held: held.clone(),
                    visible,
                    bound: captures.unwrap_or_else(Type::empty_row),
                    expected: Some(identity),
                    written: true,
                    what,
                    span,
                });
            }
        }
        let (bound, qualified) = if let Some(e) = &expected {
            let captures = self.whnf_captured(e).1;
            let qualified = match &captures {
                Some(captures) => Type::Captured {
                    ty: Box::new(ty.clone()),
                    captures: Box::new(captures.clone()),
                    value: false,
                    elided: Elided::default(),
                },
                None => ty.clone(),
            };
            (captures.unwrap_or_else(Type::empty_row), qualified)
        } else {
            let row = self.fresh();
            let qualified = Type::Captured {
                ty: Box::new(ty.clone()),
                captures: Box::new(row.clone()),
                value: false,
                elided: Elided::default(),
            };
            (row, qualified)
        };
        let written = expected.is_some() && !matches!(self.store.resolve(&bound), Type::Meta(_));
        let visible = if expected
            .as_ref()
            .is_none_or(|e| self.whnf_captured(e).1.is_some())
        {
            Visible::Now(Vec::new())
        } else {
            Visible::Late(ty)
        };
        self.hold(Hold {
            held,
            visible,
            bound,
            expected,
            written,
            what,
            span,
        });
        qualified
    }

    /// `ty` qualified with `captures`, a qualifier being inferred: it holds
    /// nothing unless a value it bounds grows it.
    pub fn inferred_captures(&mut self, ty: Type, captures: Type, span: Span) -> Type {
        self.hold(Hold {
            held: Vec::new(),
            visible: Visible::Now(Vec::new()),
            bound: captures.clone(),
            expected: None,
            written: false,
            what: "value",
            span,
        });
        Type::Captured {
            ty: Box::new(ty),
            captures: Box::new(captures),
            value: false,
            elided: Elided::default(),
        }
    }

    /// The scope whose region is the skolem `s`.
    pub fn scope_of(&self, s: u32) -> Option<&Scope> {
        self.cx
            .regions
            .scopes
            .iter()
            .find(|scope| scope.region == s)
    }

    /// Reports a type that would take the region of a scope outside it;
    /// `false` when `skolem` is no scope's region.
    pub fn region_escape(&mut self, skolem: u32, head: &str, span: Span) -> bool {
        let Some(scope) = self.scope_of(skolem).cloned() else {
            return false;
        };
        let name = self.store.skolem_name(skolem).to_owned();
        let (message, related) = if scope.handler {
            (
                format!(
                    "{head}: region `{name}` is that of `resume`, which stays in the arms of its handler"
                ),
                "the handler ends here",
            )
        } else {
            (
                format!(
                    "{head}: region `{name}` ends with its `scope`, so the value cannot leave it"
                ),
                "the scope ends here",
            )
        };
        self.diagnostics.push(
            diagnostics::Diagnostic::error(codes::G002, message, span)
                .with_related(scope.span, related),
        );
        true
    }

    /// Whether `atom` is the region of `resume` in a handler's arms.
    pub fn handler_region(&self, atom: &Type) -> bool {
        matches!(atom, Type::Skolem(s) if self.scope_of(*s).is_some_and(|scope| scope.handler))
    }
}

/// `t` with `held` added to its qualifier, without duplicating identical leaves.
pub(super) fn qualify(t: Type, held: Type) -> Type {
    match t {
        Type::Captured {
            ty,
            captures,
            value,
            elided,
        } => Type::Captured {
            ty,
            captures: Box::new(union_captures(*captures, held)),
            value,
            elided,
        },
        other => Type::Captured {
            ty: Box::new(other),
            captures: Box::new(held),
            value: false,
            elided: Elided::default(),
        },
    }
}

/// Joins two capture terms without duplicating structurally equal leaves.
pub(super) fn union_captures(left: Type, right: Type) -> Type {
    fn is_empty_row(t: &Type) -> bool {
        matches!(t, Type::Row(r) if r.entries.is_empty() && r.tail.is_none())
    }
    fn has_leaf(haystack: &Type, needle: &Type) -> bool {
        if haystack == needle {
            return true;
        }
        match haystack {
            Type::Union(a, b) => has_leaf(a, needle) || has_leaf(b, needle),
            _ => false,
        }
    }
    fn append(acc: Type, term: Type) -> Type {
        match term {
            Type::Union(a, b) => append(append(acc, *a), *b),
            leaf if is_empty_row(&leaf) || has_leaf(&acc, &leaf) => acc,
            leaf if is_empty_row(&acc) => leaf,
            leaf => Type::Union(Box::new(acc), Box::new(leaf)),
        }
    }
    append(left, right)
}

/// Whether types differ only in what they capture: the hold, not the type,
/// says what escapes.
fn capture_mismatch(mismatch: &Mismatch) -> bool {
    matches!(mismatch.reason(), Mismatch::Label { label, .. } if matches!(label, Elem::Capture(_)))
}

fn push_atom(out: &mut Norm, t: &Type) {
    if let Some(label) = capture_label(t) {
        out.entries.push((label, t.clone()));
    }
}

/// `t` without the service and error rows of its arrows and `Eff`s: a row
/// holds no captures, and an `Eff` holds what its result holds
/// (`docs/formal/regions.md` §2.1, `docs/formal/regions.md#a-scope-in-a-local-function-row-is-not-a-capture`).
fn without_rows(t: &Type) -> Type {
    match t {
        Type::Fun { param, result, .. } => Type::Fun {
            param: Box::new(without_rows(param)),
            effects: None,
            result: Box::new(without_rows(result)),
        },
        Type::App { .. } if t.spine().0.is_lang(Lang::Eff) => t
            .spine()
            .1
            .last()
            .map_or(Type::Error, |result| without_rows(result)),
        _ => t.map_children(&mut without_rows),
    }
}
