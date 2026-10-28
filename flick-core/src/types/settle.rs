//! Operators and number literals, settled once a declaration is inferred:
//! an operand or literal type nothing determined gets its default, and on a
//! type variable both go through the closed numeric classes.

use std::collections::HashSet;
use std::mem;

use super::env::{Form, TypeDef};
use super::ty::Type;
use super::typing::Identity;
use super::{Checker, Domain, NumLiteral};
use crate::diagnostics::codes;
use crate::iface::{Entity, Namespace};
use crate::numeric::{Num, default_num};
use crate::source::Span;

impl Checker<'_> {
    /// The type of a number literal: any numeric type, a `Float` one when it
    /// is `fractional`, settled with the operators.
    pub fn number_literal(
        &mut self,
        span: Span,
        value: Option<i128>,
        fractional: bool,
        pattern: bool,
    ) -> Type {
        let ty = self.store.fresh_numeric(fractional);
        self.literals.push(NumLiteral {
            ty: ty.clone(),
            span,
            value,
            fractional,
            pattern,
            givens: self.classes.givens.clone(),
        });
        ty
    }

    /// Checks the operand types of operators and the types of integer
    /// literals once the declaration is inferred; an operand type nothing
    /// determined becomes the first allowed type (`Int`, or `String` for
    /// `++`), a literal's becomes `Int`.
    pub fn settle_operators(&mut self) {
        for (ty, span) in mem::take(&mut self.equalities) {
            if let Some(identity) = self.identity(&ty) {
                let key = super::typing::range(span);
                self.typing.identity_eq.insert(key, identity);
            }
        }
        let chosen = self.chosen_by_classes();
        let waits = |checker: &Self, ty: &Type| matches!(checker.whnf(ty), Type::Meta(m) if chosen.contains(&m));
        // a literal of the wrong type explains an operator's error better
        for literal in mem::take(&mut self.literals) {
            if waits(self, &literal.ty) {
                self.literals.push(literal);
            } else {
                self.settle_literal(&literal);
            }
        }
        self.settle_signatures();
        for op in mem::take(&mut self.operators) {
            if waits(self, &op.ty) {
                self.operators.push(op);
                continue;
            }
            let ty = self.whnf(&op.ty);
            let (head, _) = ty.spine();
            // on a type variable, the methods of its `Numeric` constraint
            if op.domain == Domain::Numeric && matches!(head, Type::Skolem(_)) {
                self.want_under(
                    numeric_class("Numeric", ty.clone()),
                    op.span,
                    op.givens.clone(),
                );
                self.typing
                    .generic_numeric
                    .insert(super::typing::range(op.span));
                continue;
            }
            let allowed = op.domain.names();
            let fits = match head {
                Type::Error => true,
                Type::Meta(_) => {
                    let _ = self.unify(&ty, &Type::prim(allowed[0]));
                    true
                }
                head => op.domain.allows(head),
            };
            if op.domain == Domain::Numeric
                && let Some(num) = Num::ALL
                    .into_iter()
                    .find(|n| self.whnf(&ty).is_prim(n.name()))
            {
                self.typing
                    .numeric
                    .insert(super::typing::range(op.span), num);
            }
            if op.op == "++" && ty.spine().0.is_prim("List") {
                self.typing
                    .list_concat
                    .insert(super::typing::range(op.span));
            }
            // an operand already reported inside the operator explains it better
            let reported = || {
                self.diagnostics.iter().any(|d| {
                    d.span
                        .is_some_and(|s| s.start >= op.span.start && s.end <= op.span.end)
                })
            };
            if !fits && !reported() {
                let shown = self.show(&ty);
                let allowed = allowed
                    .iter()
                    .map(|a| format!("`{a}`"))
                    .collect::<Vec<_>>()
                    .join(" or ");
                let (code, detail) = match ty {
                    Type::Skolem(_) => (
                        codes::T008,
                        format!("`{shown}` stands for any type the caller chooses"),
                    ),
                    _ => (codes::T003, format!("not on `{shown}`")),
                };
                self.error(
                    code,
                    op.span,
                    format!("`{}` works on {allowed}, {detail}", op.op),
                );
            }
        }
    }

    /// The types of integer literals that a pending class constraint may
    /// still choose, as `HasField` chooses a field's type; solving the
    /// classes settles them.
    fn chosen_by_classes(&self) -> HashSet<u32> {
        let mut chosen = HashSet::new();
        if self.literals.is_empty() {
            return chosen;
        }
        for w in &self.classes.wanted {
            self.zonk(&w.constraint).visit(&mut |t| {
                if let Type::Meta(m) = t
                    && self.store.is_numeric(*m)
                {
                    chosen.insert(*m);
                }
            });
        }
        chosen
    }

    fn settle_literal(&mut self, literal: &NumLiteral) {
        let ty = self.whnf(&literal.ty);
        let num = match ty.spine().0 {
            Type::Error => return,
            Type::Meta(_) => {
                let num = default_num(literal.fractional);
                let _ = self.unify(&ty, &Type::prim(num.name()));
                num
            }
            // on a type variable, a method of its `Numeric` or `Floating`
            // constraint makes the value
            Type::Skolem(_) if !literal.pattern => {
                let class = if literal.fractional {
                    "Floating"
                } else {
                    "Numeric"
                };
                self.want_under(
                    numeric_class(class, ty.clone()),
                    literal.span,
                    literal.givens.clone(),
                );
                self.typing
                    .generic_numeric
                    .insert(super::typing::range(literal.span));
                return;
            }
            head => {
                let found = Num::ALL.into_iter().find(|n| head.is_prim(n.name()));
                if let Some(num) = found {
                    num
                } else {
                    let shown = self.show(&ty);
                    let (code, detail) = if let Type::Skolem(_) = head {
                        (
                            codes::T008,
                            format!(": `{shown}` stands for any type the caller chooses"),
                        )
                    } else {
                        (codes::T003, String::new())
                    };
                    self.error(
                        code,
                        literal.span,
                        format!("expected `{shown}`, found a number{detail}"),
                    );
                    return;
                }
            }
        };
        if literal.fractional {
            return;
        }
        if !literal.value.is_some_and(|v| num.holds(v)) {
            self.error(
                codes::T003,
                literal.span,
                format!("this number does not fit in `{}`", num.name()),
            );
        }
        self.typing
            .numeric
            .insert(super::typing::range(literal.span), num);
    }
}

/// The constraint `class ty` of the closed numeric classes.
fn numeric_class(class: &str, ty: Type) -> Type {
    Type::apps(Type::con("Numeric", class), [ty])
}

impl Checker<'_> {
    /// Whether `===` decides `==` on `ty`: `Int`, `String`, `Bool` and `BigInt`
    /// by value, a tagged type whose constructors have no fields by tag.
    fn identity(&mut self, ty: &Type) -> Option<Identity> {
        let ty = self.whnf(ty);
        let (head, _) = ty.spine();
        if ["Int", "String", "Bool", "BigInt"]
            .iter()
            .any(|p| head.is_prim(p))
        {
            return Some(Identity::Value);
        }
        let Type::Con { module, name } = head else {
            return None;
        };
        let Some(TypeDef {
            form: Form::Adt { constructors, abi },
            ..
        }) = self.env.type_def(module, name)
        else {
            return None;
        };
        let nullary = |c: &String| {
            let entity = Entity {
                module: module.clone(),
                namespace: Namespace::Constructor,
                name: c.clone(),
            };
            self.env
                .value(&entity)
                .is_some_and(|s| !matches!(strip(s), Type::Fun { .. }))
        };
        (abi.is_tagged() && constructors.iter().all(nullary)).then_some(Identity::Tag)
    }
}

/// `scheme` without its `forall`s.
fn strip(scheme: &Type) -> &Type {
    let mut t = scheme;
    while let Type::Forall { body, .. } = t {
        t = body;
    }
    t
}
