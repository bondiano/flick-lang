//! A branch of a `match` on a constructor of a GADT: what it learns about
//! the matched value's type holds in its guard and body only. Rigid
//! variables it learns the type of become equalities of the store, and a
//! `TypeEq` assumption among the givens, so that constraints deferred from
//! the branch are solved under them too.

use std::mem;

use super::refine::{Refined, Unsupported};
use super::scheme::{built, is_hidden_captures, is_indexed};
use crate::diagnostics::codes;
use crate::syntax::cst::Node;
use crate::types::Checker;
use crate::types::classes::args_of;
use crate::types::lang::Lang;
use crate::types::patterns::Pat;
use crate::types::ty::{Binder, Type};
use crate::types::typing::Range;
use crate::types::unify::Equality;
#[cfg(feature = "ide")]
use crate::types::{Typing, show_type};

/// What the checker knows about the code around the pattern being bound.
#[derive(Debug, Default)]
pub struct State {
    /// The pattern being bound is that of a `match` arm, outside an
    /// or-pattern: a constructor in it may refine types.
    pub refining: bool,
    /// The pattern of the arm being bound matches no value of its type.
    pub dead: bool,
    /// The code is checked against a complete signature.
    pub signed: bool,
    /// The rigid variables of constructor fields hidden from their type.
    pub existentials: Vec<u32>,
    /// The type of the local just looked up, as declared, in a GADT branch.
    pub declared: Option<Type>,
}

/// The state around an arm, restored when it is left.
#[derive(Clone, Copy)]
pub struct Arm {
    equalities: usize,
    givens: usize,
    dead: bool,
}

impl Checker<'_> {
    /// Enters an arm: one level deeper, so that what it hides or learns
    /// stays inside.
    pub(in crate::types) fn enter_arm(&mut self) -> Arm {
        self.store.level += 1;
        Arm {
            equalities: self.store.equalities.len(),
            givens: self.classes.givens.len(),
            dead: mem::take(&mut self.gadt.dead),
        }
    }

    /// Binds the pattern of an arm, which may refine types.
    pub(in crate::types) fn bind_arm_pattern(&mut self, node: &Node, ty: &Type) -> Pat {
        let refining = mem::replace(&mut self.gadt.refining, true);
        let pat = self.bind_pattern(node, ty);
        self.gadt.refining = refining;
        pat
    }

    /// Leaves an arm; whether its pattern matches no value.
    pub(in crate::types) fn leave_arm(&mut self, arm: Arm) -> bool {
        self.store.equalities.truncate(arm.equalities);
        self.classes.givens.truncate(arm.givens);
        self.store.level -= 1;
        mem::replace(&mut self.gadt.dead, arm.dead)
    }

    /// Runs `f` under the equalities among `givens`, the assumptions a
    /// deferred constraint was made with.
    pub(in crate::types) fn under<T>(
        &mut self,
        givens: &[Type],
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let mark = self.store.equalities.len();
        for g in givens {
            if let Some(args) = args_of(g, Lang::TypeEq)
                && let [Type::Skolem(s), ty] = args[..]
            {
                self.store.equalities.push(Equality {
                    skolem: *s,
                    ty: ty.clone(),
                    level: None,
                });
            }
        }
        let out = f(self);
        self.store.equalities.truncate(mark);
        out
    }

    /// Records the type `ty` at `at` in a GADT branch as the branch knows
    /// it, and what `declared`, the same type as written, is without the
    /// branch's equalities when they change it.
    pub(in crate::types) fn note_refined(&mut self, at: Range, ty: &Type, declared: &Type) {
        let refined = self.zonk(ty);
        let equalities = mem::take(&mut self.store.equalities);
        let plain = self.zonk(declared);
        self.store.equalities = equalities;
        #[cfg(feature = "ide")]
        if plain != refined {
            self.typing.pending_refined.push((at, plain));
        }
        #[cfg(not(feature = "ide"))]
        let _ = plain;
        self.typing.pending_types.push((at, refined));
    }

    /// The declared type of the local a name refers to, in a GADT branch.
    pub(in crate::types) fn declared_local(&self, i: usize) -> Option<Type> {
        (!self.store.equalities.is_empty()).then(|| self.cx.locals[i].ty.clone())
    }

    /// The equalities among the assumptions in scope, for a constraint that
    /// keeps no other assumptions.
    pub(in crate::types) fn equalities_in_scope(&self) -> Vec<Type> {
        type_eqs(&self.classes.givens)
    }

    /// The scheme of a constructor written with `: R` instantiated for a
    /// match against a value of type `scrutinee`: its fields and value over
    /// what the arm learns, which is recorded. `None` for an ordinary
    /// constructor.
    pub(in crate::types) fn indexed_pattern(
        &mut self,
        scheme: &Type,
        parent: Option<(String, String)>,
        scrutinee: &Type,
        node: &Node,
    ) -> Option<Type> {
        let (module, name) = parent?;
        if !is_indexed(scheme, &module, &name) {
            return None;
        }
        let (vars, body) = match scheme {
            Type::Forall { vars, body, .. } => (&vars[..], &**body),
            t => (&[][..], t),
        };
        let locals: Vec<Binder> = vars
            .iter()
            .filter(|v| !is_hidden_captures(v))
            .cloned()
            .collect();
        let names: Vec<String> = locals.iter().map(|v| v.name.clone()).collect();
        let refined = self.refine(&names, built(body), scrutinee);
        let constraints = match scheme {
            Type::Forall { constraints, .. } => &constraints[..],
            _ => &[],
        };
        let (mut subst, learned) = self.learn(refined, &locals, !constraints.is_empty(), node);
        if learned {
            // the dictionaries the value holds, as the pattern binds them
            let start = node.range().0;
            for (i, c) in constraints.iter().enumerate() {
                let given = field_given(&dict_local(start, i), c.substitute(&subst));
                self.classes.givens.push(given);
            }
        }
        if let Some(hidden) = vars.iter().find(|v| is_hidden_captures(v)) {
            // what the value holds, as the arm knows its type
            let captures = self.fresh();
            let held = self.captures_of(scrutinee).into_type();
            let _ = self.unify(&captures, &held);
            subst.push((hidden.name.clone(), captures));
        }
        Some(body.substitute(&subst))
    }

    /// The types of a constructor's variables in the arm, after recording
    /// what it learns, and whether it learned it; errors when it learns
    /// nothing it may. A constructor that `holds` dictionaries refines too.
    fn learn(
        &mut self,
        refined: Refined,
        locals: &[Binder],
        holds: bool,
        node: &Node,
    ) -> (Vec<(String, Type)>, bool) {
        let span = self.span(node);
        let errors = || {
            (
                locals
                    .iter()
                    .map(|v| (v.name.clone(), Type::Error))
                    .collect(),
                false,
            )
        };
        let (bound, equals) = match refined {
            Refined::Unifier { bound, equals } => (bound, equals),
            Refined::Impossible => {
                self.gadt.dead |= self.gadt.refining;
                return errors();
            }
            Refined::Unknown => {
                self.error(
                    codes::T057,
                    span,
                    "the type of the matched value is not known here, and this constructor refines it: annotate the value or the function",
                );
                return errors();
            }
            Refined::Unsupported(what) => {
                let what = match what {
                    Unsupported::Region => "two regions are the same",
                    Unsupported::Row => "two rows are the same",
                };
                self.error(
                    codes::T058,
                    span,
                    format!("matching this constructor would mean that {what}, which Flick does not learn from a pattern"),
                );
                return errors();
            }
        };
        let hidden: Vec<&Binder> = locals
            .iter()
            .filter(|v| !bound.iter().any(|(b, _)| *b == v.name))
            .collect();
        if (!hidden.is_empty() || !equals.is_empty() || holds) && !self.gadt.refining {
            self.error(
                codes::T058,
                span,
                "this constructor hides or refines a type, which only an arm of a `match` learns: match it in a `match`",
            );
            return errors();
        }
        if !equals.is_empty() && !self.gadt.signed {
            self.error(
                codes::T057,
                span,
                "this constructor refines the type of the matched value, which the checker learns only under a complete signature of the function",
            );
            return errors();
        }
        let mut subst = bound;
        for v in hidden {
            let skolem = self.store.skolem(&v.name);
            if let Type::Skolem(s) = skolem {
                self.gadt.existentials.push(s);
                self.cx.regions.rigid_kinds.push((s, v.kind.clone()));
            }
            subst.push((v.name.clone(), skolem));
        }
        // a bound variable may stand for one that the constructor hides
        let subst: Vec<(String, Type)> = subst
            .iter()
            .map(|(v, t)| (v.clone(), t.substitute(&subst)))
            .collect();
        let level = Some(self.store.level);
        for (s, ty) in equals {
            let ty = ty.substitute(&subst);
            let given = Type::apps(Type::lang(Lang::TypeEq), [Type::Skolem(s), ty.clone()]);
            self.classes.givens.push(given);
            self.store.equalities.push(Equality {
                skolem: s,
                ty,
                level,
            });
        }
        (subst, true)
    }
}

/// The module of the marker of a given held by a matched value.
const HELD: &str = "$held";

/// The given `constraint` whose dictionary the local `local` holds, bound by
/// the pattern of a constructor whose values hold dictionaries.
pub fn field_given(local: &str, constraint: Type) -> Type {
    let marker = Type::Con {
        module: HELD.to_owned(),
        name: "Field".to_owned(),
    };
    Type::apps(marker, [Type::Symbol(local.to_owned()), constraint])
}

/// The local and constraint of a given made by [`field_given`].
pub fn as_field_given(g: &Type) -> Option<(&str, &Type)> {
    match g.spine() {
        (Type::Con { module, .. }, args) if module == HELD => match args[..] {
            [Type::Symbol(local), c] => Some((local, c)),
            _ => None,
        },
        _ => None,
    }
}

/// The local the pattern at `start` binds the `i`-th dictionary of its
/// constructor to.
pub fn dict_local(start: u32, i: usize) -> String {
    format!("$dict{start}_{i}")
}

/// The equalities among `givens`; none outside a GADT branch.
pub fn type_eqs(givens: &[Type]) -> Vec<Type> {
    givens
        .iter()
        .filter(|g| args_of(g, Lang::TypeEq).is_some())
        .cloned()
        .collect()
}

#[cfg(feature = "ide")]
impl Typing {
    /// The type at `at` as an editor shows it: `Int (= a)` in a GADT branch
    /// that learned `a` is `Int`.
    pub fn shown(&self, at: Range, ty: &Type) -> String {
        let shown = show_type(ty);
        match self.refined.get(&at) {
            Some(plain) => format!("{shown} (= {})", show_type(plain)),
            None => shown,
        }
    }
}
