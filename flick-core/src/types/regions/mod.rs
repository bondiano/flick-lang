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

mod compare;
mod data;
mod provided;
mod scopes;
mod solve;

pub use provided::{PROVIDED, Provide};
pub use solve::Outlives;

use super::Checker;
use super::kinds::Kind;
use super::ty::{Norm, Type, capture_label};
use super::typing::Range;
use super::unify::Mismatch;
use crate::diagnostics;
use crate::diagnostics::codes;
use crate::source::Span;
use crate::types::lang::{self, Lang};

/// The service a `scope` provides to the operations that acquire resources.
pub fn scope() -> &'static str {
    &lang::item(Lang::Scope).label
}

/// Regions and captures of the declaration being checked.
#[derive(Debug, Default)]
pub struct State {
    pub holds: Vec<Hold>,
    pub scopes: Vec<Scope>,
    /// The scopes around the expression being checked, innermost last.
    pub open: Vec<usize>,
    /// The scopes and `provide`s around it, innermost last: what a new one
    /// nests in.
    pub nest: Vec<usize>,
    /// The `provide`s around it, innermost last.
    pub provides: Vec<Provide>,
    /// `Fiber.Provided` constraints until their call has its arguments,
    /// and those whose row may still grow (`provided.rs`).
    pub provided: Vec<provided::Want>,
    pub provided_late: Vec<provided::Want>,
    /// A `provide` compared with an owner not known yet: its scope, the
    /// owner, the service and where.
    pub late_owners: Vec<(usize, Type, String, Span)>,
    /// The locals used so far, by index into `Checker::locals`.
    pub uses: Vec<usize>,
    pub outlives: Vec<solve::Outlives>,
    /// Error rows raised, whose payloads must hold no region.
    pub raised: Vec<(Type, Span)>,
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

/// A function's reader: the services it reads atoms through, which end
/// when it returns, so a lazy `for` that reads them cannot leave it.
#[derive(Debug, Clone)]
pub struct Reader {
    pub level: u32,
    /// The region, once a `for` needs it, and that `for`.
    pub region: Option<(u32, Span)>,
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
    /// The services of the stage around the scope.
    pub outer: Type,
    pub parent: Option<usize>,
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
    Now(Vec<String>),
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

/// The types whose values hold nothing of their arguments: a witness
/// describes values of its type, holding none of them.
const HOLD_NOTHING: [Lang; 7] = [
    Lang::Layer,
    Lang::ServiceTag,
    Lang::ServiceImpl,
    Lang::Errors,
    Lang::Schema,
    Lang::Url,
    Lang::Form,
];

impl Checker<'_> {
    /// What a value of `t` may hold, as a capture row: regions and type
    /// variables as entries, capture rows not inferred yet as tails. A
    /// qualifier bounds abstract reachable captures; concrete region
    /// dependencies remain visible even beneath a bound.
    pub fn captures_of(&self, t: &Type) -> Norm {
        let mut out = Norm::default();
        self.collect_captures(t, &mut out);
        out.entries.sort_by(|a, b| a.0.cmp(&b.0));
        out.entries.dedup_by(|a, b| a.0 == b.0);
        out
    }

    fn collect_captures(&self, t: &Type, out: &mut Norm) {
        self.collect_bounded_captures(t, out, false);
    }

    fn collect_bounded_captures(&self, t: &Type, out: &mut Norm, bounded: bool) {
        let t = self.qualified_head(t);
        match &t {
            Type::Bounded { ty, captures } => {
                if self.holds_nothing(ty) {
                    return;
                }
                self.capture_row_filtered(captures, out, false);
                self.collect_bounded_captures(ty, out, true);
            }
            Type::Captured { ty, captures, .. } => {
                if self.holds_nothing(ty) {
                    return;
                }
                self.capture_row_filtered(captures, out, bounded);
                self.collect_bounded_captures(ty, out, true);
            }
            // a capture row not inferred yet is a tail: it may still be empty
            #[allow(
                clippy::collapsible_match,
                reason = "a known tail must not fall to an atom"
            )]
            Type::Meta(m) if self.store.is_capture_row(*m) => {
                if !out.tails.contains(&t) {
                    out.tails.push(t.clone());
                }
            }
            // an integer literal's type is a number, which captures nothing
            Type::Meta(m) if self.store.is_numeric(*m) => {}
            Type::Var(_) | Type::Skolem(_) | Type::Meta(_)
                if !bounded || matches!(self.atom_kind(&t, &[]), Some(Kind::Region)) =>
            {
                push_atom(out, &t);
            }
            Type::App { .. } => {
                let (head, args) = t.spine();
                // an `Eff` holds what its result holds, as a function does: a
                // capture shown only in the result is lost when the result is dropped
                if HOLD_NOTHING.into_iter().any(|n| head.is_lang(n)) {
                    return;
                }
                let kinds = match head {
                    Type::Con { module, name } => self
                        .env
                        .type_def(module, name)
                        .map(|d| self.kinds.resolve(&d.kind)),
                    _ => None,
                };
                let mut kind = kinds.as_ref();
                for arg in args {
                    let param = match kind {
                        Some(Kind::Arrow(param, rest)) => {
                            kind = Some(rest);
                            Some(&**param)
                        }
                        _ => None,
                    };
                    // an error cannot carry a region, so its row holds none; a
                    // service row is needed again wherever the value runs; a
                    // symbol is text
                    if matches!(
                        param,
                        Some(Kind::ErrorRow | Kind::ServiceRow | Kind::Symbol)
                    ) {
                        continue;
                    }
                    match self.store.resolve(arg) {
                        row @ (Type::Row(_) | Type::Union(..)) => {
                            self.capture_row_filtered(&row, out, bounded);
                        }
                        _ => self.collect_bounded_captures(arg, out, bounded),
                    }
                }
            }
            Type::Tuple(items) => items
                .iter()
                .for_each(|i| self.collect_bounded_captures(i, out, bounded)),
            Type::Record(row) => row
                .entries
                .iter()
                .for_each(|(_, t)| self.collect_bounded_captures(t, out, bounded)),
            Type::Fun { param, result, .. } => {
                if bounded {
                    without_rows(&self.zonk(&t)).visit(&mut |t| {
                        if matches!(self.atom_kind(t, &[]), Some(Kind::Region)) {
                            push_atom(out, t);
                        }
                    });
                } else {
                    self.collect_bounded_captures(param, out, false);
                    self.collect_bounded_captures(result, out, false);
                }
            }
            // what the `forall` binds is no capture of the value: each use
            // instantiates it (a recursive local sees its own scheme)
            Type::Forall { vars, body, .. } => {
                let mut inner = Norm::default();
                self.collect_bounded_captures(body, &mut inner, bounded);
                let bound =
                    |t: &Type| matches!(t, Type::Var(v) if vars.iter().any(|b| b.name == *v));
                out.entries
                    .extend(inner.entries.into_iter().filter(|(_, t)| !bound(t)));
                for tail in inner.tails {
                    if !out.tails.contains(&tail) {
                        out.tails.push(tail);
                    }
                }
            }
            _ => {}
        }
    }

    /// Adds the regions of a capture row; its rigid variables count as
    /// entries, so `{s | c}` and `{s, c}` are the same set.
    pub fn capture_row(&self, row: &Type, out: &mut Norm) {
        self.capture_row_filtered(row, out, false);
        out.entries.sort_by(|a, b| a.0.cmp(&b.0));
        out.entries.dedup_by(|a, b| a.0 == b.0);
    }

    fn capture_row_filtered(&self, row: &Type, out: &mut Norm, bounded: bool) {
        let norm = self.store.normalize(row);
        for (label, t) in &norm.entries {
            if label.starts_with('^') {
                match self.store.resolve(t) {
                    row @ (Type::Row(_) | Type::Union(..)) => {
                        self.capture_row_filtered(&row, out, bounded);
                    }
                    other => self.collect_bounded_captures(&other, out, bounded),
                }
            }
        }
        for tail in norm.tails {
            match tail {
                Type::Var(_) | Type::Skolem(_)
                    if !bounded || matches!(self.atom_kind(&tail, &[]), Some(Kind::Region)) =>
                {
                    push_atom(out, &tail);
                }
                Type::Meta(_) if !bounded && !out.tails.contains(&tail) => out.tails.push(tail),
                Type::Error if !out.tails.contains(&tail) => out.tails.push(tail),
                _ => {}
            }
        }
    }

    /// A curried stage retains earlier parameters. Include projections of
    /// generic value captures as well as regions, so partial application
    /// has the same whole-value bound as an explicitly written closure.
    pub fn implicit_captures(&self, t: &Type) -> Type {
        self.stages(t, &mut Vec::new(), &Type::empty_row())
    }

    fn stages(&self, t: &Type, kinds: &mut Vec<(String, Kind)>, held: &Type) -> Type {
        match t {
            Type::Forall {
                vars,
                constraints,
                body,
            } => {
                let mark = kinds.len();
                kinds.extend(vars.iter().map(|v| (v.name.clone(), v.kind.clone())));
                let body = self.stages(body, kinds, held);
                kinds.truncate(mark);
                Type::Forall {
                    vars: vars.clone(),
                    constraints: constraints.clone(),
                    body: Box::new(body),
                }
            }
            Type::Captured { ty, captures, .. } | Type::Bounded { ty, captures } => {
                let ty = Box::new(self.stages(ty, kinds, captures));
                if matches!(t, Type::Bounded { .. }) {
                    Type::Bounded {
                        ty,
                        captures: captures.clone(),
                    }
                } else {
                    Type::Captured {
                        ty,
                        captures: captures.clone(),
                        value: matches!(t, Type::Captured { value: true, .. }),
                    }
                }
            }
            Type::Fun {
                param,
                effects,
                result,
            } => {
                let param = self.stages(param, kinds, &Type::empty_row());
                let mut captures = self.captures_of(&param);
                captures.entries.retain(|(_, atom)| {
                    matches!(
                        self.atom_kind(atom, kinds),
                        None | Some(Kind::Type | Kind::Region | Kind::CaptureRow)
                    )
                });
                let held = Type::Union(Box::new(held.clone()), Box::new(captures.into_type()));
                let explicit = self.whnf_captured(result).1.is_some();
                let mut result = self.stages(result, kinds, &held);
                let retained = self.store.normalize(&held);
                if !explicit
                    && matches!(self.whnf(&result), Type::Fun { .. })
                    && (!retained.entries.is_empty() || !retained.tails.is_empty())
                {
                    result = qualify(result, held);
                }
                Type::Fun {
                    param: Box::new(param),
                    effects: effects.clone(),
                    result: Box::new(result),
                }
            }
            other => {
                let expanded = self.qualified_head(other);
                if matches!(expanded, Type::Fun { .. }) {
                    self.stages(&expanded, kinds, held)
                } else {
                    other.map_children(&mut |c| self.stages(c, kinds, &Type::empty_row()))
                }
            }
        }
    }

    /// Whether `atom` is a region or a capture row, not a type variable.
    fn is_region(&self, atom: &Type, kinds: &[(String, Kind)]) -> bool {
        matches!(
            self.atom_kind(atom, kinds),
            Some(Kind::Region | Kind::CaptureRow)
        )
    }

    fn atom_kind(&self, atom: &Type, kinds: &[(String, Kind)]) -> Option<Kind> {
        let kind = match atom {
            Type::Var(name) => kinds
                .iter()
                .rev()
                .find(|(n, _)| n == name)
                .map(|(_, k)| k.clone())
                .or_else(|| {
                    let v = self.tyvars.iter().rev().find(|v| v.name == *name)?;
                    Some(v.kind.clone())
                }),
            Type::Skolem(s) => self
                .regions
                .rigid_kinds
                .iter()
                .rev()
                .find(|(id, _)| id == s)
                .map(|(_, kind)| kind.clone())
                .or_else(|| self.scope_of(*s).map(|_| Kind::Region)),
            _ => None,
        };
        kind.map(|k| self.kinds.resolve(&k))
    }

    /// What a type shows its values may hold: exactly what they may reach, so
    /// a region a type mentions where no value keeps it hides no capture.
    pub fn shown_by(&self, t: &Type) -> Vec<String> {
        self.captures_of(t)
            .entries
            .into_iter()
            .map(|(label, _)| label)
            .collect()
    }

    pub fn hold(&mut self, hold: Hold) {
        self.regions.holds.push(hold);
    }

    /// Marks where a closure starts: the uses after it are its body's.
    pub fn uses_mark(&self) -> usize {
        self.regions.uses.len()
    }

    /// The locals below `locals` that were used since `uses`, by name.
    pub fn held_since(&self, uses: usize, locals: usize) -> Held {
        let mut seen: Vec<usize> = Vec::new();
        for &i in &self.regions.uses[uses..] {
            if i < locals && !seen.contains(&i) {
                seen.push(i);
            }
        }
        seen.into_iter()
            .map(|i| (Some(self.locals[i].name.clone()), self.locals[i].ty.clone()))
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
        if held.is_empty() {
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
        }
    }

    /// Starts the reader of a function whose body is checked next.
    pub fn begin_reader(&mut self) {
        let level = self.store.level;
        self.regions.readers.push(Reader {
            level,
            region: None,
        });
    }

    /// The region of the reader the innermost function lends to a lazy
    /// `for` at `span`.
    pub fn reader_region(&mut self, span: Span) -> Type {
        let level = self
            .regions
            .readers
            .last()
            .map_or(self.store.level, |r| r.level);
        if let Some(Reader {
            region: Some((id, _)),
            ..
        }) = self.regions.readers.last()
        {
            return Type::Skolem(*id);
        }
        let region = self.store.skolem_at("reader", level);
        if let Type::Skolem(id) = &region {
            self.regions.lent.push(*id);
            if let Some(reader) = self.regions.readers.last_mut() {
                reader.region = Some((*id, span));
            }
        }
        region
    }

    /// `ty` qualified with the reader `region` too. An inferred qualifier is
    /// the sequence's own: it already holds what it reads.
    pub fn read_through(&mut self, ty: Type, region: &Type) -> Type {
        let label = capture_label(region).expect("a skolem has a label");
        let held = Type::Row(super::ty::Row::closed(vec![(label, region.clone())]));
        let inferred = match &ty {
            Type::Captured { captures, .. } => match **captures {
                Type::Meta(m) if matches!(self.store.resolve(captures), Type::Meta(_)) => Some(m),
                _ => None,
            },
            _ => None,
        };
        match inferred {
            Some(m) => {
                self.store.set(m, held);
                ty
            }
            None => qualify(ty, held),
        }
    }

    /// Ends the innermost function's reader, reporting its `result` if it
    /// holds the reader.
    pub fn end_reader(&mut self, result: &Type, span: Span) {
        let Some(Reader {
            region: Some((id, at)),
            ..
        }) = self.regions.readers.pop()
        else {
            return;
        };
        let mut found = false;
        self.zonk(result)
            .visit(&mut |t| found |= *t == Type::Skolem(id));
        if !found {
            return;
        }
        let head = self.show(result);
        self.diagnostics.push(
            diagnostics::Diagnostic::error(
                codes::G002,
                format!(
                    "the result has type `{head}`, a sequence whose clauses read atoms through this function's reader, so it cannot leave the function"
                ),
                span,
            )
            .with_related(at, "the sequence reads atoms here").with_fix(|| ("build a list with `[for …]`, or iterate the sequence in this function", Vec::new())),
        );
    }
    /// The scope whose region is the skolem `s`.
    pub fn scope_of(&self, s: u32) -> Option<&Scope> {
        self.regions.scopes.iter().find(|scope| scope.region == s)
    }

    /// Reports a type that would take the region of a scope outside it;
    /// `false` when `skolem` is no scope's region.
    pub fn region_escape(&mut self, skolem: u32, head: &str, span: Span) -> bool {
        let Some(scope) = self.scope_of(skolem).cloned() else {
            return false;
        };
        let name = self.store.skolem_name(skolem).to_owned();
        self.diagnostics.push(
            diagnostics::Diagnostic::error(
                codes::G002,
                format!(
                    "{head}: region `{name}` ends with its `scope`, so the value cannot leave it"
                ),
                span,
            )
            .with_related(scope.span, "the scope ends here"),
        );
        true
    }
}

/// `t` with `held` added to its qualifier.
fn qualify(t: Type, held: Type) -> Type {
    match t {
        Type::Captured {
            ty,
            captures,
            value,
        } => Type::Captured {
            ty,
            captures: Box::new(Type::Union(captures, Box::new(held))),
            value,
        },
        other => Type::Captured {
            ty: Box::new(other),
            captures: Box::new(held),
            value: false,
        },
    }
}

/// Whether types differ only in what they capture: the hold, not the type,
/// says what escapes.
fn capture_mismatch(mismatch: &Mismatch) -> bool {
    matches!(mismatch, Mismatch::Label { label, .. } if label.starts_with('^'))
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
