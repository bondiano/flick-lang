//! Patterns: their types, the names they bind, and their shape for the
//! exhaustiveness check.

use std::{iter, mem};

use super::ty::{Row, Type};
use super::gadt::scheme::dicts;
use super::{Checker, Local};
use crate::diagnostics::codes;
use crate::diagnostics::suggest::closest;
use crate::iface;
use crate::iface::Namespace;
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

pub fn is_pattern(kind: K) -> bool {
    (K::WildcardPat..=K::OrPat).contains(&kind)
}

impl Checker<'_> {
    /// Checks `node` against `ty` and brings its names into scope. A
    /// data qualifier bounds its fields too, including callback fields.
    #[allow(clippy::too_many_lines)]
    pub fn bind_pattern(&mut self, node: &Node, ty: &Type) -> Pat {
        let children: Vec<&Node> = node.nodes().collect();
        let span = self.span(node);
        match node.kind {
            K::WildcardPat => Pat::Wild,
            K::VarPat => {
                if let Some(name) = node.child_tokens().find(|t| t.kind == K::Ident) {
                    self.note_type(span, ty);
                    self.locals.push(Local {
                        name: name.text(self.text).to_owned(),
                        generated: name.start >= self.generated,
                        ty: ty.clone(),
                        rec: None,
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
                    if numeric::is_integer_literal(text) {
                        let value =
                            numeric::integer_value(text).map(|v| if negative { -v } else { v });
                        self.number_literal(self.span(node), value, false, true)
                    } else {
                        Type::lang(Lang::Float)
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
                        let value = literal::number_value(text).unwrap_or_default();
                        Ctor::Literal(format!("{}", if negative { -value } else { value }))
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
        let mark = self.locals.len();
        let mut pats = Vec::new();
        for (i, alternative) in alternatives.iter().enumerate() {
            let inner = self.locals.len();
            // alternatives would learn different things: none refines
            let refining = mem::take(&mut self.gadt.refining);
            pats.push(self.bind_pattern(alternative, ty));
            self.gadt.refining = refining;
            if i == 0 {
                continue;
            }
            let bound: Vec<Local> = self.locals.drain(inner..).collect();
            for local in bound {
                let first = self.locals[mark..inner]
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
        let mut fields: Vec<(String, Type, Option<&Node>)> = Vec::new();
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
            fields.push((name, field_ty, field.nodes().next()));
        }
        let rest = self.fresh();
        let shape = Type::Record(Row {
            entries: {
                let mut entries: Vec<(String, Type)> = fields
                    .iter()
                    .map(|(l, t, _)| (l.clone(), t.clone()))
                    .collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                entries
            },
            tail: Some(Box::new(rest)),
        });
        let (bare, captures) = self.whnf_captured(ty);
        self.pattern_type(&shape, &bare, node);
        let mut pats: Vec<(String, Pat)> = fields
            .into_iter()
            .map(|(label, field_ty, pattern)| {
                let field_ty = captures.as_ref().map_or_else(
                    || field_ty.clone(),
                    |captures| self.data_field(&field_ty, captures),
                );
                let pat = if let Some(p) = pattern {
                    self.bind_pattern(p, &field_ty)
                } else {
                    self.locals.push(Local {
                        name: label.clone(),
                        generated: node.range().0 >= self.generated,
                        ty: field_ty,
                        rec: None,
                    });
                    Pat::Wild
                };
                (label, pat)
            })
            .collect();
        pats.sort_by(|a, b| a.0.cmp(&b.0));
        Pat::Record(pats)
    }

    fn constructor_pattern(&mut self, node: &Node, args: &[&Node], ty: &Type) -> Pat {
        let parts: Vec<&str> = node
            .child_tokens()
            .filter(|t| matches!(t.kind, K::Con | K::Ident))
            .map(|t| t.text(self.text))
            .collect();
        let Some((&name, qualifier)) = parts.split_last() else {
            return Pat::Wild;
        };
        let Some(entity) = self.constructor(&qualifier.join("."), name, node) else {
            for arg in args {
                self.bind_pattern(arg, &Type::Error);
            }
            return Pat::Wild;
        };
        let scheme = self.env.value(&entity).cloned().unwrap_or(Type::Error);
        self.note_type(self.name_span(node), &scheme);
        let captures = self.captures_of(ty).into_type();
        let adt_params = self.env.parent(&entity).and_then(|(module, name)| {
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
        });
        let indexed = self.indexed_pattern(&scheme, self.env.parent(&entity), ty, node);
        let mut current = if let Some(body) = &indexed {
            body.clone()
        } else if let Type::Forall { vars, body, .. } = &scheme
            && let Some(arity) = adt_params
        {
            let subst: Vec<(String, Type)> = vars
                .iter()
                .map(|v| (v.name.clone(), self.store.fresh_of(&v.kind)))
                .collect();
            if vars.len() > arity
                && let Some((_, capture)) = subst.last()
            {
                let _ = self.unify(capture, &captures);
            }
            self.instantiate(&body.substitute(&subst), self.span(node))
        } else {
            self.instantiate(&scheme, self.span(node))
        };
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
            self.pattern_type(&self.whnf(&current), &self.whnf(ty), node);
        }
        let mut args_pats = Vec::new();
        for (arg, param) in args
            .iter()
            .zip(params.iter().chain(iter::repeat(&Type::Error)))
        {
            args_pats.push(self.bind_pattern(arg, param));
        }
        if let Some((module, ty)) = self.env.parent(&entity) {
            let rep = self.env.constructor_rep(&module, &ty, name);
            let constructor = super::typing::Constructor {
                module,
                ty,
                name: name.to_owned(),
                arity: params.len(),
                rep,
                dicts: dicts(&scheme),
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

    /// The entity of a constructor; `None` after a diagnostic or when its
    /// module could not be checked.
    fn constructor(&mut self, qualifier: &str, name: &str, node: &Node) -> Option<iface::Entity> {
        if !qualifier.is_empty() {
            let found = self.env.qualified_module(qualifier).map(|(_, interface)| {
                interface.map(|i| {
                    i.find(Namespace::Constructor, name)
                        .map(|e| e.entity.clone())
                })
            });
            // a module that was checked but exports no such constructor: say so, or nothing records the pattern
            if let Some(Some(None)) = &found {
                self.error_guess(
                    codes::T001,
                    self.name_span(node),
                    format!("`{qualifier}` does not export constructor `{name}`"),
                    |this| {
                        let (_, Some(interface)) = this.env.qualified_module(qualifier)? else {
                            return None;
                        };
                        let constructors = interface
                            .exports
                            .iter()
                            .filter(|e| e.entity.namespace == Namespace::Constructor);
                        closest(name, constructors.map(|e| e.entity.name.as_str()))
                            .map(|g| format!("{qualifier}.{g}"))
                    },
                );
                return None;
            }
            let found = found.map(Option::flatten);
            if found.is_none() {
                self.error_guess(
                    codes::T001,
                    self.name_span(node),
                    format!("unknown module `{qualifier}`"),
                    |this| {
                        closest(qualifier, this.env.qualifier_names())
                            .map(|g| format!("{g}.{name}"))
                    },
                );
            }
            return found.flatten();
        }
        match self.env.find(Namespace::Constructor, name) {
            super::env::Found::Entity(entity) => Some(entity),
            super::env::Found::Unknown => None,
            super::env::Found::Missing => {
                self.error_guess(
                    codes::T001,
                    self.name_span(node),
                    format!("cannot find constructor `{name}`"),
                    |this| closest(name, this.env.names(Namespace::Constructor)).map(str::to_owned),
                );
                None
            }
        }
    }
}
