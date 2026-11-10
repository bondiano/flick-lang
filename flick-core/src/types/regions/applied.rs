//! What an application returns: pure parametric code returns only what its
//! function and argument hold, and a local `forall v. τ^v -> σ` with `v`
//! not in `σ` nothing of what its argument holds.

use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::types::Checker;
use crate::types::kinds::Kind;
use crate::types::ty::{Norm, Type};

impl Checker<'_> {
    /// Pure parametric code can return only captures from its function and
    /// argument. Refine abstract results, not independent concrete scalars.
    pub fn application_result(
        &self,
        result: Type,
        function: &Type,
        argument: &Type,
        parametric: bool,
    ) -> Type {
        let result = self.value_bound(result);
        // under `live.rs` a function or a task holds nothing it needs live
        let used =
            self.eff_parts(&result).is_some() || matches!(self.whnf(&result), Type::Fun { .. });
        if used
            || !self.refinable_result(&result)
            || matches!(
                self.whnf(function),
                Type::Fun {
                    effects: Some(_),
                    ..
                }
            )
        {
            return result;
        }
        let mut captures = self.captures_of(function);
        // a result of `forall v. τ^v -> σ` holds nothing of its argument
        let arg = if parametric {
            Norm::default()
        } else {
            self.captures_of(argument)
        };
        captures.entries.extend(arg.entries);
        captures.tails.extend(arg.tails);
        // A reader region is the lending function's own, not data a result
        // stores; its holds report every escape.
        captures
            .entries
            .retain(|(_, atom)| !matches!(atom, Type::Skolem(s) if self.regions.lent.contains(s)));
        // a type variable is no use of what it stands for
        captures.entries.retain(|(_, atom)| {
            matches!(self.store.resolve(atom), Type::Meta(_))
                || matches!(
                    self.atom_kind(atom, &[]),
                    Some(Kind::Region | Kind::CaptureRow)
                )
        });
        Type::Bounded {
            ty: Box::new(result),
            captures: Box::new(captures.into_type()),
        }
    }

    /// Whether `head` names a local whose type, before a use instantiates
    /// it, is parametric in its argument (`parametric_in_argument`).
    pub(in crate::types) fn local_parametric(&self, head: &Node) -> bool {
        if head.kind != K::NameExpr {
            return false;
        }
        let tokens: Vec<_> = head.child_tokens().filter(|t| t.kind != K::Dot).collect();
        let [token] = tokens[..] else {
            return false;
        };
        let (name, span) = (token.text(self.text), self.span(head));
        (self.locals.iter())
            .rposition(|l| l.name == name && l.generated == (span.start >= self.generated))
            .is_some_and(|i| self.parametric_in_argument(&self.locals[i].ty))
    }

    /// Whether `function`, the type of a local before a use instantiates it,
    /// is `forall v. τ^v -> σ` under its own qualifier with `v` not in `σ`:
    /// by parametricity what it returns holds nothing of what its argument
    /// holds, as `f` of `Some (f x)` in `Option.map`.
    fn parametric_in_argument(&self, function: &Type) -> bool {
        let Type::Forall { vars, body, .. } = self.whnf_captured(function).0 else {
            return false;
        };
        let Type::Fun { param, result, .. } = &*body else {
            return false;
        };
        let Type::Bounded { captures, .. } = &**param else {
            return false;
        };
        let Type::Var(v) = &**captures else {
            return false;
        };
        vars.iter()
            .any(|b| b.name == *v && b.kind == Kind::CaptureRow)
            && !result.contains(&mut |t| matches!(t, Type::Var(x) if x == v))
    }
}
