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

mod scopes;
mod solve;

pub use solve::Outlives;

use super::Checker;
use super::kinds::Kind;
use super::ty::{Norm, Row, Type, capture_label};
use super::typing::Range;
use crate::diagnostics;
use crate::diagnostics::codes;
use crate::source::Span;

/// The service a `scope` provides to the operations that acquire resources.
pub const SCOPE: &str = "Prelude.Scope";

/// Regions and captures of the declaration being checked.
#[derive(Debug, Default)]
pub struct State {
    pub holds: Vec<Hold>,
    pub scopes: Vec<Scope>,
    /// The scopes around the expression being checked, innermost last.
    pub open: Vec<usize>,
    /// The locals used so far, by index into `Checker::locals`.
    pub uses: Vec<usize>,
    pub outlives: Vec<solve::Outlives>,
    /// Error rows raised, whose payloads must hold no region.
    pub raised: Vec<(Type, Span)>,
    /// Calls inside a `scope` that need `Scope`, until their innermost
    /// scope ends.
    pub needs: Vec<Need>,
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

impl Checker<'_> {
    /// What a value of `t` may hold, as a capture row: regions and type
    /// variables as entries, capture rows not inferred yet as tails. A
    /// function or an `Eff` holds only what its qualifier says; data holds
    /// what its parts hold.
    pub fn captures_of(&self, t: &Type) -> Norm {
        let mut out = Norm::default();
        self.collect_captures(t, &mut out);
        out.entries.sort_by(|a, b| a.0.cmp(&b.0));
        out.entries.dedup_by(|a, b| a.0 == b.0);
        out
    }

    fn collect_captures(&self, t: &Type, out: &mut Norm) {
        let (t, qualifier) = self.whnf_captured(t);
        if let Some(row) = qualifier {
            self.capture_row(&row, out);
            // a variable qualified with `^c` holds at most `c`
            if matches!(t, Type::Var(_) | Type::Skolem(_) | Type::Meta(_)) {
                return;
            }
        }
        match &t {
            Type::Var(_) | Type::Skolem(_) => push_atom(out, &t),
            Type::App { .. } => {
                let (head, args) = t.spine();
                let opaque = ["Eff", "Layer", "ServiceTag", "ServiceImpl", "Errors"];
                if opaque.iter().any(|n| head.is_prim(n)) {
                    return;
                }
                for arg in args {
                    match self.store.resolve(arg) {
                        row @ (Type::Row(_) | Type::Union(..)) => self.capture_row(&row, out),
                        _ => self.collect_captures(arg, out),
                    }
                }
            }
            Type::Tuple(items) => items.iter().for_each(|i| self.collect_captures(i, out)),
            Type::Record(row) => row
                .entries
                .iter()
                .for_each(|(_, t)| self.collect_captures(t, out)),
            _ => {}
        }
    }

    /// Adds the regions of a capture row; its rigid variables count as
    /// entries, so `{s | c}` and `{s, c}` are the same set.
    pub fn capture_row(&self, row: &Type, out: &mut Norm) {
        let norm = self.store.normalize(row);
        for (label, t) in &norm.entries {
            if label.starts_with('^') {
                push_atom(out, t);
            }
        }
        for tail in norm.tails {
            match tail {
                Type::Var(_) | Type::Skolem(_) => push_atom(out, &tail),
                Type::Meta(_) | Type::Error if !out.tails.contains(&tail) => out.tails.push(tail),
                _ => {}
            }
        }
    }

    /// A signature as it is checked: each stage of a curried function holds
    /// the regions of the parameters before it, as its partial application
    /// does, so `File s -> String -> String` is
    /// `File s -> (String -> String)^{s}`. What a stage's type shows anyway
    /// is left out; a type variable's captures are not known here, so a stage
    /// that hides them still needs `a^c`.
    pub fn implicit_captures(&self, t: &Type) -> Type {
        self.stages(t, &mut Vec::new(), &[])
    }

    fn stages(&self, t: &Type, kinds: &mut Vec<(String, Kind)>, held: &[(String, Type)]) -> Type {
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
            Type::Captured { ty, captures } => Type::Captured {
                ty: Box::new(self.stages(ty, kinds, held)),
                captures: captures.clone(),
            },
            Type::Fun {
                param,
                effects,
                result,
            } => {
                let param = self.stages(param, kinds, &[]);
                let mut held = held.to_vec();
                for (label, atom) in self.captures_of(&param).entries {
                    if self.is_region(&atom, kinds) && !held.iter().any(|(l, _)| *l == label) {
                        held.push((label, atom));
                    }
                }
                let mut result = self.stages(result, kinds, &held);
                if matches!(self.whnf(&result), Type::Fun { .. }) {
                    let visible = self.visible_in(&result);
                    held.retain(|(label, _)| !visible.contains(label));
                    result = qualify(result, held);
                }
                Type::Fun {
                    param: Box::new(param),
                    effects: effects.clone(),
                    result: Box::new(result),
                }
            }
            other => other.map_children(&mut |c| self.stages(c, kinds, &[])),
        }
    }

    /// Whether `atom` is a region or a capture row, not a type variable.
    fn is_region(&self, atom: &Type, kinds: &[(String, Kind)]) -> bool {
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
            Type::Skolem(_) => self
                .tyvars
                .iter()
                .rev()
                .find(|v| v.ty == *atom)
                .map(|v| v.kind.clone()),
            _ => None,
        };
        matches!(
            kind.map(|k| self.kinds.resolve(&k)),
            Some(Kind::Region | Kind::CaptureRow)
        )
    }

    /// The regions and variables a type mentions anywhere.
    pub fn visible_in(&self, t: &Type) -> Vec<String> {
        let mut out = Vec::new();
        self.zonk(t).visit(&mut |t| {
            if let Some(label) = matches!(t, Type::Var(_) | Type::Skolem(_))
                .then(|| capture_label(t))
                .flatten()
                && !out.contains(&label)
            {
                out.push(label);
            }
        });
        out
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
        let (bound, qualified) = if let Some(e) = &expected {
            let bound = self.whnf_captured(e).1.unwrap_or_else(Type::empty_row);
            (bound, ty.clone())
        } else {
            let row = self.fresh();
            let qualified = Type::Captured {
                ty: Box::new(ty.clone()),
                captures: Box::new(row.clone()),
            };
            (row, qualified)
        };
        let written = expected.is_some() && !matches!(self.store.resolve(&bound), Type::Meta(_));
        self.hold(Hold {
            held,
            visible: Visible::Late(ty),
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
        }
    }

    /// Unifies types of which at least one has a capture qualifier: the
    /// qualifiers are equal, as in mutable storage.
    pub fn unify_captured(
        &mut self,
        (a, ca): (Type, Option<Type>),
        (b, cb): (Type, Option<Type>),
    ) -> super::unify::Unified {
        let captured = |t: &Type, c: &Option<Type>| match c {
            Some(c) => Type::Captured {
                ty: Box::new(t.clone()),
                captures: Box::new(c.clone()),
            },
            None => t.clone(),
        };
        match (&a, &b) {
            (Type::Error, _) | (_, Type::Error) => Ok(()),
            (Type::Meta(m), _) if ca.is_none() => self.bind(*m, &captured(&b, &cb)),
            (_, Type::Meta(m)) if cb.is_none() => self.bind(*m, &captured(&a, &ca)),
            _ => {
                let empty = Type::empty_row();
                self.unify(ca.as_ref().unwrap_or(&empty), cb.as_ref().unwrap_or(&empty))?;
                self.unify(&a, &b)
            }
        }
    }

    /// Subsumption where a qualifier is involved: a value fits a position
    /// whose qualifier covers what it holds; a bare position takes no
    /// capture its type does not show.
    pub fn subsume_captured(
        &mut self,
        (actual, (a, ca)): (&Type, (Type, Option<Type>)),
        (expected, (e, ce)): (&Type, (Type, Option<Type>)),
        span: Span,
    ) -> super::unify::Unified {
        match (&a, &e) {
            (Type::Error, _) | (_, Type::Error) => return Ok(()),
            (_, Type::Meta(_)) if ce.is_none() => return self.unify(actual, expected),
            (Type::Meta(_), _) if ca.is_none() => return self.unify(actual, expected),
            _ => {}
        }
        let visible = Visible::Now(self.visible_in(&e));
        let bound = ce.unwrap_or_else(Type::empty_row);
        let written = !matches!(self.store.resolve(&bound), Type::Meta(_));
        self.hold(Hold {
            held: vec![(None, actual.clone())],
            visible,
            bound,
            expected: Some(expected.clone()),
            written,
            what: "value",
            span,
        });
        self.subsume(&a, &e, span)
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
fn qualify(t: Type, mut held: Vec<(String, Type)>) -> Type {
    if held.is_empty() {
        return t;
    }
    let (ty, captures) = match t {
        Type::Captured { ty, captures } => match *captures {
            Type::Row(row) => {
                for entry in row.entries {
                    if !held.iter().any(|(l, _)| *l == entry.0) {
                        held.push(entry);
                    }
                }
                (ty, row.tail)
            }
            other => (ty, Some(Box::new(other))),
        },
        other => (Box::new(other), None),
    };
    held.sort_by(|a, b| a.0.cmp(&b.0));
    Type::Captured {
        ty,
        captures: Box::new(Type::Row(Row {
            entries: held,
            tail: captures,
        })),
    }
}

fn push_atom(out: &mut Norm, t: &Type) {
    if let Some(label) = capture_label(t) {
        out.entries.push((label, t.clone()));
    }
}
