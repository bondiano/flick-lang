//! Patterns: their types, the names they bind, and their shape for the
//! exhaustiveness check.

mod resolve;

use crate::types::Elem;
use crate::types::Elided;
use std::{iter, mem};

use super::gadt::scheme::dicts;
use super::kinds::Kind;
use super::ty::{Row, Type};
use super::typing::Range;
use super::{Checker, Local};
use crate::diagnostics::codes;
use crate::iface;
use crate::numeric;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal;
use crate::types::lang::Lang;

/// The shape of a pattern as far as matching is concerned.
#[derive(Debug, Clone, PartialEq)]
pub enum Pat {
    Wild,
    Ctor(Ctor, Vec<Pat>),
    /// Fields a record pattern mentions, sorted by label.
    Record(Vec<(String, Pat)>),
    Or(Vec<Pat>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Ctor {
    /// A constructor of the type `ty` of `module`.
    Variant {
        module: String,
        ty: String,
        name: String,
    },
    Bool(bool),
    Unit,
    Tuple(usize),
    /// A list literal pattern of this length.
    List(usize),
    /// A list pattern `[a, b, ...rest]` with this many items before the rest:
    /// lists of at least that length.
    ListRest(usize),
    /// A `Float`, `BigInt` or `String` literal, by its value.
    Literal(String),
}

pub use crate::syntax::patterns::is_pattern;

impl Checker<'_> {
    /// Checks `node` against `ty` and brings its names into scope. A
    /// data qualifier bounds its fields too, including callback fields.
    #[allow(clippy::too_many_lines)]
    pub fn bind_pattern(&mut self, node: &Node, ty: &Type) -> Pat {
        self.note_ir_value(node.range(), ty);
        let children: Vec<&Node> = node.nodes().collect();
        let span = self.span(node);
        match node.kind {
            K::WildcardPat => Pat::Wild,
            K::VarPat => {
                if let Some(name) = node.child_tokens().find(|t| t.kind == K::Ident) {
                    self.note_type(span, ty);
                    self.cx.locals.push(Local {
                        name: name.text(self.text).to_owned(),
                        at: Some(name.start),
                        generated: name.start >= self.generated,
                        ty: ty.clone(),
                        rec: None,
                        binding: super::mutation::Binding::Other,
                    });
                }
                Pat::Wild
            }
            K::ParenPat => self.bind_pattern(children[0], ty),
            K::LitPat => {
                let tokens: Vec<_> = node.child_tokens().collect();
                let Some(last) = tokens.last() else {
                    return Pat::Wild;
                };
                let lit = if last.kind == K::Number {
                    let text = last.text(self.text);
                    let negative = tokens.len() > 1;
                    if let Some(num) = literal::split_number_suffix(text).1 {
                        let fractional = num.fractional();
                        let value = (!fractional)
                            .then(|| {
                                numeric::integer_value(text).map(|v| if negative { -v } else { v })
                            })
                            .flatten();
                        self.suffixed_number_literal(self.span(node), num, value, fractional, true)
                    } else if numeric::is_integer_literal(text) {
                        let value =
                            numeric::integer_value(text).map(|v| if negative { -v } else { v });
                        self.number_literal(self.span(node), value, false, true)
                    } else {
                        self.number_literal(self.span(node), None, true, true)
                    }
                } else {
                    super::infer::literal_type(Some(last.kind))
                };
                self.pattern_type(&lit, ty, node);
                let negative = tokens.len() > 1;
                let text = last.text(self.text);
                let ctor = match last.kind {
                    K::TrueKw => Ctor::Bool(true),
                    K::FalseKw => Ctor::Bool(false),
                    K::String => Ctor::Literal(format!("{:?}", literal::string_value(text))),
                    K::BigInt => {
                        let value = literal::bigint_value(text).unwrap_or_default();
                        Ctor::Literal(format!("{}n", if negative { -value } else { value }))
                    }
                    _ => {
                        if let Some(value) = numeric::integer_value(text) {
                            Ctor::Literal(format!("{}", if negative { -value } else { value }))
                        } else if literal::split_number_suffix(text).1
                            == Some(numeric::Num::Float32)
                        {
                            let value = literal::float32_value(text).unwrap_or_default();
                            Ctor::Literal(format!("{}", if negative { -value } else { value }))
                        } else {
                            let value = literal::number_value(text).unwrap_or_default();
                            Ctor::Literal(format!("{}", if negative { -value } else { value }))
                        }
                    }
                };
                Pat::Ctor(ctor, Vec::new())
            }
            K::UnitPat => {
                self.pattern_type(&Type::lang(Lang::Unit), ty, node);
                Pat::Ctor(Ctor::Unit, Vec::new())
            }
            K::TuplePat => {
                let (bare, captures) = self.whnf_captured(ty);
                let items: Vec<Type> = match &bare {
                    Type::Tuple(items) if items.len() == children.len() => items.clone(),
                    _ => {
                        let items: Vec<Type> = children.iter().map(|_| self.fresh()).collect();
                        self.pattern_type(&Type::Tuple(items.clone()), &bare, node);
                        items
                    }
                };
                let args = children
                    .iter()
                    .zip(&items)
                    .map(|(c, t)| {
                        let t = captures
                            .as_ref()
                            .map_or_else(|| t.clone(), |captures| self.data_field(t, captures));
                        self.bind_pattern(c, &t)
                    })
                    .collect();
                Pat::Ctor(Ctor::Tuple(children.len()), args)
            }
            K::ListPat => {
                let (bare, captures) = self.whnf_captured(ty);
                let item = self.list_item(&bare).unwrap_or_else(|| self.fresh());
                self.pattern_type(
                    &Type::app(Type::lang(Lang::List), item.clone()),
                    &bare,
                    node,
                );
                let projected = captures
                    .as_ref()
                    .map_or_else(|| item.clone(), |captures| self.data_field(&item, captures));
                let (rest, items): (Vec<&Node>, Vec<&Node>) =
                    children.iter().partition(|c| c.kind == K::RestPat);
                let args = items
                    .iter()
                    .map(|c| self.bind_pattern(c, &projected))
                    .collect();
                let list = Type::app(Type::lang(Lang::List), item);
                let list = match captures {
                    Some(captures) => Type::Bounded {
                        ty: Box::new(list),
                        captures: Box::new(captures),
                    },
                    None => list,
                };
                for r in rest.iter().flat_map(|r| r.nodes()) {
                    self.bind_pattern(r, &list);
                }
                let ctor = if rest.is_empty() {
                    Ctor::List(items.len())
                } else {
                    Ctor::ListRest(items.len())
                };
                Pat::Ctor(ctor, args)
            }
            K::RecordPat => self.record_pattern(node, ty),
            K::AnnotPat => {
                let annotated = children.get(1).map_or(Type::Error, |t| self.annotation(t));
                // a written type without a qualifier names the value's type; what
                // it may capture stays the position's, as in `fun (p : T) -> …`
                // given to a callback of `forall v. a^v -> b`
                let (bare, captures) = self.whnf_captured(ty);
                match captures {
                    Some(captures) if self.whnf_captured(&annotated).1.is_none() => {
                        self.pattern_type(&annotated, &bare, node);
                        let held = Type::Captured {
                            ty: Box::new(annotated),
                            captures: Box::new(captures),
                            value: false,
                            elided: Elided::default(),
                        };
                        self.bind_pattern(children[0], &held)
                    }
                    _ => {
                        self.pattern_type(&annotated, ty, node);
                        self.bind_pattern(children[0], &annotated)
                    }
                }
            }
            K::OrPat => self.or_pattern(&children, ty),
            K::ConPat => self.constructor_pattern(node, &children, ty),
            _ => {
                let _ = span;
                Pat::Wild
            }
        }
    }

    fn pattern_type(&mut self, pattern: &Type, scrutinee: &Type, node: &Node) {
        if let Err(mismatch) = self.unify(pattern, scrutinee) {
            self.mismatch(&mismatch, pattern, scrutinee, self.span(node));
        }
    }

    /// Every alternative binds the same names (checked by the parser); their
    /// types must agree too.
    fn or_pattern(&mut self, alternatives: &[&Node], ty: &Type) -> Pat {
        let mark = self.cx.locals.len();
        let mut pats = Vec::new();
        for (i, alternative) in alternatives.iter().enumerate() {
            let inner = self.cx.locals.len();
            // alternatives would learn different things: none refines
            let refining = mem::take(&mut self.gadt.refining);
            pats.push(self.bind_pattern(alternative, ty));
            self.gadt.refining = refining;
            if i == 0 {
                continue;
            }
            let bound: Vec<Local> = self.cx.locals.drain(inner..).collect();
            for local in bound {
                let first = self.cx.locals[mark..inner]
                    .iter()
                    .find(|l| l.name == local.name)
                    .map(|l| l.ty.clone());
                // equal, not a subtype: a value bound in one alternative is not lifted
                if let Some(first) = first
                    && let Err(mismatch) = self.unify(&local.ty, &first)
                {
                    self.mismatch(&mismatch, &local.ty, &first, self.span(alternative));
                }
            }
        }
        Pat::Or(pats)
    }

    fn record_pattern(&mut self, node: &Node, ty: &Type) -> Pat {
        let mut fields: Vec<(String, Type, Option<&Node>, Range)> = Vec::new();
        for field in node.nodes().filter(|n| n.kind == K::RecordPatField) {
            let Some(label) = field.child_tokens().find(|t| t.kind == K::Ident) else {
                continue;
            };
            let name = label.text(self.text).to_owned();
            #[cfg(feature = "ide")]
            self.note_field(self.token_span(label), ty);
            if fields.iter().any(|(l, ..)| *l == name) {
                self.duplicate_label(&name, self.token_span(label));
                continue;
            }
            let field_ty = self.fresh();
            fields.push((name, field_ty, field.nodes().next(), field.range()));
        }
        let rest = self.fresh();
        let shape = Type::Record(Row {
            entries: {
                let mut entries: Vec<(Elem, Type)> = fields
                    .iter()
                    .map(|(l, t, ..)| (Elem::named(l), t.clone()))
                    .collect();
                Row::sort_entries(&mut entries);
                entries
            },
            tail: Some(Box::new(rest)),
            ..Row::default()
        });
        let (bare, captures) = self.whnf_captured(ty);
        self.pattern_type(&shape, &bare, node);
        let mut pats: Vec<(String, Pat)> = fields
            .into_iter()
            .map(|(label, field_ty, pattern, field_range)| {
                let field_ty = captures.as_ref().map_or_else(
                    || field_ty.clone(),
                    |captures| self.data_field(&field_ty, captures),
                );
                let pat = if let Some(p) = pattern {
                    self.bind_pattern(p, &field_ty)
                } else {
                    self.note_ir_value(field_range, &field_ty);
                    self.cx.locals.push(Local {
                        name: label.clone(),
                        at: None,
                        generated: node.range().0 >= self.generated,
                        ty: field_ty,
                        rec: None,
                        binding: super::mutation::Binding::Other,
                    });
                    Pat::Wild
                };
                (label, pat)
            })
            .collect();
        // a few labels, each once: an insertion sort, which spares the
        // browser compiler a sort of its own for these pairs
        for i in 1..pats.len() {
            let mut j = i;
            while j > 0 && pats[j - 1].0 > pats[j].0 {
                pats.swap(j - 1, j);
                j -= 1;
            }
        }
        Pat::Record(pats)
    }

    /// The number of parameters of the data type a constructor belongs to.
    fn adt_arity(&self, entity: &iface::Entity) -> Option<usize> {
        let (module, name) = self.env.parent(entity)?;
        let def = self.env.type_def(&module, &name)?;
        if !matches!(def.form, super::env::Form::Adt { .. }) {
            return None;
        }
        let (mut arity, mut kind) = (0, &def.kind);
        while let super::kinds::Kind::Arrow(_, result) = kind {
            arity += 1;
            kind = result;
        }
        Some(arity)
    }

    fn instantiate_con_pattern(
        &mut self,
        node: &Node,
        scheme: &Type,
        ty: &Type,
        adt_params: Option<usize>,
        indexed: Option<&(Type, Vec<(Kind, Type)>)>,
    ) -> (Type, Vec<(Kind, Type)>) {
        if let Some((body, type_args)) = indexed {
            return (body.clone(), type_args.clone());
        }
        let captures = self.captures_of(ty).into_type();
        if let Type::Forall { vars, body, .. } = scheme
            && let Some(arity) = adt_params
        {
            let mut recorded = Vec::with_capacity(vars.len());
            let subst: Vec<(String, Type)> = vars
                .iter()
                .map(|v| {
                    let fresh = self.store.fresh_of(&v.kind);
                    recorded.push((v.kind.clone(), fresh.clone()));
                    (v.name.clone(), fresh)
                })
                .collect();
            if vars.len() > arity
                && let Some((_, capture)) = subst.last()
            {
                let _ = self.unify(capture, &captures);
            }
            let (inst, more) = self.instantiate_args(&body.substitute(&subst), self.span(node));
            recorded.extend(more);
            return (inst, recorded);
        }
        self.instantiate_args(scheme, self.span(node))
    }

    fn constructor_pattern(&mut self, node: &Node, args: &[&Node], ty: &Type) -> Pat {
        let Some((name, entity)) = self.pattern_constructor(node) else {
            return Pat::Wild;
        };
        let Some(entity) = entity else {
            for arg in args {
                self.bind_pattern(arg, &Type::Error);
            }
            return Pat::Wild;
        };
        let scheme = self.env.value(&entity).cloned().unwrap_or(Type::Error);
        self.note_type(self.name_span(node), &scheme);
        let adt_params = self.adt_arity(&entity);
        let indexed = self.indexed_pattern(&scheme, self.env.parent(&entity), ty, node);
        let (mut current, con_type_args) =
            self.instantiate_con_pattern(node, &scheme, ty, adt_params, indexed.as_ref());
        let mut params = Vec::new();
        while let Type::Fun { param, result, .. } = self.whnf(&current) {
            params.push(*param);
            current = *result;
        }
        if current != Type::Error && params.len() != args.len() {
            self.error(
                codes::T018,
                self.span(node),
                format!(
                    "constructor `{name}` takes {} argument(s), but the pattern gives {}",
                    params.len(),
                    args.len()
                ),
            );
            for arg in args {
                self.bind_pattern(arg, &Type::Error);
            }
            return Pat::Wild;
        }
        if indexed.is_none() {
            let built = self.whnf(&current);
            let matched = self
                .error_element(&built, ty, node)
                .unwrap_or_else(|| self.whnf(ty));
            self.pattern_type(&built, &matched, node);
        }
        let mut args_pats = Vec::new();
        for (arg, param) in args
            .iter()
            .zip(params.iter().chain(iter::repeat(&Type::Error)))
        {
            args_pats.push(self.bind_pattern(arg, param));
        }
        if let Some((module, ty)) = self.env.parent(&entity) {
            self.record_con_use(node.range(), entity.clone(), con_type_args);
            let rep = self.env.constructor_rep(&module, &ty, name);
            let constructor = super::typing::Constructor {
                module,
                ty,
                name: name.to_owned(),
                arity: params.len(),
                rep,
                dicts: dicts(&scheme),
                fields: Vec::new(),
                type_args: Vec::new(),
            };
            self.record_name(
                self.span(node),
                super::typing::Ref::Constructor(constructor),
            );
        }
        match self.env.parent(&entity) {
            Some((module, ty)) => Pat::Ctor(
                Ctor::Variant {
                    module,
                    ty,
                    name: name.to_owned(),
                },
                args_pats,
            ),
            None => Pat::Wild,
        }
    }
}
