//! Patterns: their types, the names they bind, and their shape for the
//! exhaustiveness check.

use super::ty::{Row, Type};
use super::{Checker, Local};
use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal;

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
    /// A `Number`, `BigInt` or `String` literal, by its value.
    Literal(String),
}

pub fn is_pattern(kind: K) -> bool {
    (K::WildcardPat..=K::OrPat).contains(&kind)
}

impl Checker<'_> {
    /// Checks `node` against `ty` and brings its names into scope. A
    /// constructor's fields were checked against their types when it was
    /// built, so the parts of a qualified value have their own types.
    pub fn bind_pattern(&mut self, node: &Node, ty: &Type) -> Pat {
        if let (bare, Some(_)) = self.whnf_captured(ty)
            && !matches!(
                node.kind,
                K::VarPat | K::WildcardPat | K::ParenPat | K::AnnotPat | K::OrPat
            )
        {
            return self.bind_pattern(node, &bare);
        }
        let children: Vec<&Node> = node.nodes().collect();
        let span = self.span(node);
        match node.kind {
            K::WildcardPat => Pat::Wild,
            K::VarPat => {
                if let Some(name) = node.child_tokens().find(|t| t.kind == K::Ident) {
                    let name = name.text(self.text).to_owned();
                    self.locals.push(Local {
                        name,
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
                let lit = super::infer::literal_type(Some(last.kind));
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
                self.pattern_type(&Type::prim("Unit"), ty, node);
                Pat::Ctor(Ctor::Unit, Vec::new())
            }
            K::TuplePat => {
                let items: Vec<Type> = match self.whnf(ty) {
                    Type::Tuple(items) if items.len() == children.len() => items,
                    _ => {
                        let items: Vec<Type> = children.iter().map(|_| self.fresh()).collect();
                        self.pattern_type(&Type::Tuple(items.clone()), ty, node);
                        items
                    }
                };
                let args = children
                    .iter()
                    .zip(&items)
                    .map(|(c, t)| self.bind_pattern(c, t))
                    .collect();
                Pat::Ctor(Ctor::Tuple(children.len()), args)
            }
            K::ListPat => {
                let item = self.fresh();
                self.pattern_type(&Type::app(Type::prim("List"), item.clone()), ty, node);
                let args = children
                    .iter()
                    .map(|c| self.bind_pattern(c, &item))
                    .collect();
                Pat::Ctor(Ctor::List(children.len()), args)
            }
            K::RecordPat => self.record_pattern(node, ty),
            K::AnnotPat => {
                let annotated = children.get(1).map_or(Type::Error, |t| self.annotation(t));
                self.pattern_type(&annotated, ty, node);
                self.bind_pattern(children[0], &annotated)
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
            pats.push(self.bind_pattern(alternative, ty));
            if i == 0 {
                continue;
            }
            let bound: Vec<Local> = self.locals.drain(inner..).collect();
            for local in bound {
                let first = self.locals[mark..inner]
                    .iter()
                    .find(|l| l.name == local.name)
                    .map(|l| l.ty.clone());
                if let Some(first) = first {
                    self.expect(&local.ty, &first, self.span(alternative));
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
        self.pattern_type(&shape, ty, node);
        let mut pats: Vec<(String, Pat)> = fields
            .into_iter()
            .map(|(label, field_ty, pattern)| {
                let pat = if let Some(p) = pattern {
                    self.bind_pattern(p, &field_ty)
                } else {
                    self.locals.push(Local {
                        name: label.clone(),
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
        let mut current = self.instantiate(&scheme, self.span(node));
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
        self.pattern_type(&current, ty, node);
        let mut args_pats = Vec::new();
        for (arg, param) in args
            .iter()
            .zip(params.iter().chain(std::iter::repeat(&Type::Error)))
        {
            args_pats.push(self.bind_pattern(arg, param));
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
    fn constructor(
        &mut self,
        qualifier: &str,
        name: &str,
        node: &Node,
    ) -> Option<crate::iface::Entity> {
        if !qualifier.is_empty() {
            let found = self.env.qualified_module(qualifier).map(|(_, interface)| {
                interface
                    .and_then(|i| i.find(Namespace::Constructor, name))
                    .map(|e| e.entity.clone())
            });
            if found.is_none() {
                self.error(
                    codes::T001,
                    self.span(node),
                    format!("unknown module `{qualifier}`"),
                );
            }
            return found.flatten();
        }
        match self.env.find(Namespace::Constructor, name) {
            super::env::Found::Entity(entity) => Some(entity),
            super::env::Found::Unknown => None,
            super::env::Found::Missing => {
                self.error(
                    codes::T001,
                    self.span(node),
                    format!("cannot find constructor `{name}`"),
                );
                None
            }
        }
    }
}
