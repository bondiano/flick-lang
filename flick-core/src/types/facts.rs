//! What a plugin's `check` reads of its code: the type the checker found
//! for each `_` of it, as text, and the fields of a closed record; and what
//! its `check-calls` reads of the places of the module that call a name its
//! package watches.

use std::mem;

use flick_plugin::{Argument, CallSite, Constant, Fact, FactField, FactType, Refinement, Span};

use super::Checker;
use super::env::{Abi, EnumLit, Form};
use super::refined;
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
                        label: label.clone(),
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

    /// `t` with every alias in it expanded.
    fn unaliased(&self, t: &Type) -> Type {
        self.whnf(t)
            .map_children(&mut |child| self.unaliased(child))
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
        self.typing.call_sites = (calls.iter())
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
                    refinement: self.refinement(call),
                }
            })
            .collect();
    }

    /// How the refined type of a companion `T$parse` checks the constant the
    /// call gives it: the literals of an enum, a check the compiler knows by
    /// name, or its verdict on the module's own predicate.
    fn refinement(&self, call: &PendingCall) -> Option<Refinement> {
        let ty = call.name.strip_suffix("$parse")?;
        let constant = call.args.first().and_then(|(_, _, c)| c.as_ref())?;
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
        if let Some(known) = self.known_check(predicate) {
            return Some(Refinement::Known(known.to_owned()));
        }
        refined::holds(predicate, self.text, constant).map(Refinement::Holds)
    }
}

/// The constant a plugin gets; a `BigInt` is none, as no check of the
/// compiler's reads one.
fn constant_of(c: &Const) -> Option<Constant> {
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
