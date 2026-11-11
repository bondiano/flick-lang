//! What a plugin's `check` reads of its code: the type the checker found
//! for each `_` of it, as text, and the fields of a closed record; and what
//! its `check-calls` reads of the places of the module that call a name its
//! package watches.

use std::{collections::HashMap, mem};

use flick_plugin::{
    Argument, CallSite, Constant, ExpressionFact, Fact, FactField, FactType, Refinement, Span,
};

use super::Checker;
use super::env::{Abi, EnumLit, Form};
use super::ty::Type;
use crate::consteval::Const;
use crate::source::Span as SourceSpan;
use crate::types::lang::Lang;

impl Checker<'_> {
    /// The fact of a `_` of generated code the checker found `t` for; the
    /// span is the block's, which the expansion gives it.
    pub(super) fn fact(&self, t: &Type) -> Fact {
        let t = self.zonk(t);
        let fields = match self.whnf(&t) {
            Type::Record(row) if row.tail.is_none() => Some(
                (row.entries.iter())
                    .map(|(label, ty)| FactField {
                        label: label.to_string(),
                        ty: self.fact_type(ty),
                    })
                    .collect(),
            ),
            _ => None,
        };
        Fact {
            span: Span::default(),
            ty: self.fact_type(&t),
            fields,
        }
    }

    fn fact_type(&self, t: &Type) -> FactType {
        let expanded = self.unaliased(t);
        let unknown = expanded.contains(&mut |x| matches!(x, Type::Meta(_) | Type::Error));
        FactType {
            shown: self.show_exact(t),
            exact: (!unknown).then(|| self.show_qualified(&expanded)),
            literal_of: self.literal_of(&expanded),
        }
    }

    /// The exact base type of `t` when it is an enum, opaque or not.
    fn literal_of(&self, t: &Type) -> Option<String> {
        let Type::Con { module, name } = t else {
            return None;
        };
        let first = match &self.env.type_def(module, name)?.form {
            Form::Adt {
                abi: Abi::Literal(literals),
                ..
            }
            | Form::Nominal { literals } => literals.first()?,
            _ => return None,
        };
        let base = match first {
            EnumLit::Int(_) => Lang::Int,
            EnumLit::String(_) => Lang::String,
        };
        Some(self.show_qualified(&Type::lang(base)))
    }

    /// `t` with every alias in it expanded, but one inside its own
    /// expansion.
    fn unaliased(&self, t: &Type) -> Type {
        self.expand_all(t, &|t| self.whnf(t), &mut Vec::new())
    }
}

/// An application of a watched name, as the walk found it: its types are
/// read once the module is solved.
pub(super) struct PendingCall {
    module: String,
    name: String,
    span: SourceSpan,
    /// Each argument: where it is written, its type and the constant it
    /// folds to.
    args: Vec<(SourceSpan, Type, Option<Const>)>,
}

impl Checker<'_> {
    /// Records the application of `module.name` at `span` to `args`, which a
    /// plugin watches.
    pub(super) fn watched_call(
        &mut self,
        module: &str,
        name: &str,
        span: SourceSpan,
        args: Vec<(SourceSpan, Type, Option<Const>)>,
    ) {
        self.calls.push(PendingCall {
            module: module.to_owned(),
            name: name.to_owned(),
            span,
            args,
        });
    }

    /// The call sites the plugins watching this module get, their types as
    /// the solved module has them.
    pub(super) fn record_call_sites(&mut self) {
        let calls = mem::take(&mut self.calls);
        let mut predicates = HashMap::new();
        let sites = (calls.iter())
            .map(|call| {
                let arguments = (call.args.iter())
                    .map(|(at, ty, constant)| {
                        let fact = self.fact(ty);
                        Argument {
                            span: span_of(*at),
                            ty: fact.ty,
                            fields: fact.fields,
                            constant: constant.as_ref().and_then(constant_of),
                        }
                    })
                    .collect();
                CallSite {
                    module: call.module.clone(),
                    name: call.name.clone(),
                    span: span_of(call.span),
                    arguments,
                    refinement: self.refinement(call, &mut predicates),
                    binding: None,
                }
            })
            .collect::<Vec<_>>();
        self.typing.call_sites.extend(sites);
    }

    /// How the refined type of a companion `T$parse` checks the constant the
    /// call gives it: enum literals, or read-only syntax of the module's
    /// own predicate. The plugin owns interpretation of that syntax.
    fn refinement<'n>(
        &self,
        call: &'n PendingCall,
        predicates: &mut HashMap<&'n str, ExpressionFact>,
    ) -> Option<Refinement> {
        let ty = call.name.strip_suffix("$parse")?;
        call.args.first().and_then(|(_, _, c)| c.as_ref())?;
        // an enum's literals travel in its interface, an opaque one's too
        let literals = match self.env.type_def(&call.module, ty).map(|d| &d.form) {
            Some(Form::Adt {
                abi: Abi::Literal(literals),
                ..
            }) => Some(literals.clone()),
            Some(Form::Nominal { literals }) if !literals.is_empty() => Some(literals.clone()),
            _ => None,
        };
        if let Some(literals) = literals {
            let of = |l: &EnumLit| match l {
                EnumLit::Int(i) => Constant::Number(f64::from(*i)),
                EnumLit::String(s) => Constant::Text(s.clone()),
            };
            return Some(Refinement::Literals(literals.iter().map(of).collect()));
        }
        if call.module != self.env.module {
            return None;
        }
        let predicate = self.predicates.get(ty).copied()?;
        let expression = predicates
            .entry(ty)
            .or_insert_with(|| self.expression_fact(predicate));
        Some(Refinement::Predicate(expression.clone()))
    }
}

/// The constant a plugin gets; a `BigInt` is none, as no check of the
/// compiler's reads one.
pub(super) fn constant_of(c: &Const) -> Option<Constant> {
    match c {
        Const::Number(n) => Some(Constant::Number(*n)),
        Const::String(s) => Some(Constant::Text(s.clone())),
        Const::Bool(b) => Some(Constant::Boolean(*b)),
        Const::BigInt(_) => None,
    }
}

fn span_of(span: SourceSpan) -> Span {
    Span {
        start: span.start,
        end: span.end,
    }
}
