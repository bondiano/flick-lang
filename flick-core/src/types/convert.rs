//! From the syntax of types, rows and kinds to [`Type`] and [`Kind`], checking
//! kinds on the way: every position decides the kind it expects, and the
//! kinds of type variables follow from where they are used.

use super::Checker;
use super::env::{Form, Found};
use super::kinds::Kind;
use super::ty::{Binder, Row, Type};
use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

mod kinds;
mod rows;

/// What a type variable that is not in scope means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Free {
    /// A signature: quantified over the whole signature.
    Quantify,
    /// A local annotation: a type yet to be inferred.
    Fresh,
    /// A declaration body: only its parameters are in scope.
    Reject,
}

/// A type variable in scope: a `forall` binder while its body is converted,
/// a parameter of a declaration, or a variable of an enclosing signature.
#[derive(Debug, Clone)]
pub struct TyVar {
    pub name: String,
    pub ty: Type,
    pub kind: Kind,
}

pub fn is_type(kind: K) -> bool {
    (K::NameType..=K::CaptureType).contains(&kind)
        && !matches!(
            kind,
            K::EffectAnnot | K::Row | K::RowTail | K::RecordTypeField
        )
}

impl Checker<'_> {
    /// A signature's type with its new free variables quantified, and
    /// whether the signature starts with a `forall` that binds all of them.
    pub fn signature(&mut self, node: &Node) -> (Type, bool) {
        let mark = self.tyvars.len();
        let ty = self.ty(node, &Kind::Type, Free::Quantify);
        let new: Vec<Binder> = self.tyvars[mark..]
            .iter()
            .map(|v| Binder {
                name: v.name.clone(),
                kind: v.kind.clone(),
            })
            .collect();
        self.tyvars.truncate(mark);
        let explicit = node.kind == K::ForallType && new.is_empty();
        let ty = match (new.is_empty(), ty) {
            (true, ty) => ty,
            (
                false,
                Type::Forall {
                    mut vars,
                    constraints,
                    body,
                },
            ) => {
                vars.splice(0..0, new);
                Type::Forall {
                    vars,
                    constraints,
                    body,
                }
            }
            (false, body) => Type::Forall {
                vars: new,
                constraints: Vec::new(),
                body: Box::new(body),
            },
        };
        (self.default_kinds(&ty), explicit)
    }

    /// A local type annotation; its unknown variables are inferred.
    pub fn annotation(&mut self, node: &Node) -> Type {
        let mark = self.tyvars.len();
        let ty = self.ty(node, &Kind::Type, Free::Fresh);
        self.tyvars.truncate(mark);
        self.default_kinds(&ty)
    }

    /// Kinds of `forall` binders, with what stayed unknown made `Type`.
    pub fn default_kinds(&mut self, t: &Type) -> Type {
        match t {
            Type::Forall {
                vars,
                constraints,
                body,
            } => Type::Forall {
                vars: vars
                    .iter()
                    .map(|v| Binder {
                        name: v.name.clone(),
                        kind: self.kinds.zonk(&v.kind, true),
                    })
                    .collect(),
                constraints: constraints.iter().map(|c| self.default_kinds(c)).collect(),
                body: Box::new(self.default_kinds(body)),
            },
            _ => t.map_children(&mut |c| self.default_kinds(c)),
        }
    }

    /// Converts a type of kind `expected`, keeping the variables it brings
    /// into scope for the caller to collect.
    pub fn ty(&mut self, node: &Node, expected: &Kind, free: Free) -> Type {
        let span = self.span(node);
        let children: Vec<&Node> = node.nodes().collect();
        let (ty, kind) = match node.kind {
            K::ParenType => return self.ty(children[0], expected, free),
            K::NameType if node.has_token(K::NoneKw) => {
                let kind = self.kinds.resolve(expected);
                if !kind.is_row() && !matches!(kind, Kind::Meta(_)) {
                    self.error(
                        codes::T002,
                        span,
                        format!(
                            "`none` is an empty row, but {} is expected here",
                            with_article(&kind)
                        ),
                    );
                    return Type::Error;
                }
                return Type::empty_row();
            }
            K::NameType | K::AppType => {
                let (ty, kind) = self.applied(node, &children, free);
                if ty == Type::Error {
                    return ty;
                }
                (ty, kind)
            }
            K::UnitType => (Type::prim("Unit"), Kind::Type),
            K::TupleType => (
                Type::Tuple(
                    children
                        .iter()
                        .map(|c| self.ty(c, &Kind::Type, free))
                        .collect(),
                ),
                Kind::Type,
            ),
            K::ArrowType => (self.arrow(&children, free), Kind::Type),
            K::RecordType => (Type::Record(self.record_type(node, free)), Kind::Type),
            K::RowType => {
                let expected = self.kinds.resolve(expected);
                if expected == Kind::Type && children.is_empty() {
                    (Type::Record(Row::default()), Kind::Type)
                } else {
                    let ty = self.row(node, &expected, free);
                    (ty, expected)
                }
            }
            K::UnionType => {
                let left = self.ty(children[0], expected, free);
                let right = children
                    .get(1)
                    .map_or(Type::Error, |r| self.ty(r, expected, free));
                let kind = self.kinds.resolve(expected);
                if matches!(kind, Kind::Meta(_)) || kind.is_row() {
                    (Type::Union(Box::new(left), Box::new(right)), kind)
                } else {
                    self.error(
                        codes::T002,
                        span,
                        format!(
                            "`+` joins rows, but {} is expected here",
                            with_article(&kind)
                        ),
                    );
                    return Type::Error;
                }
            }
            K::CaptureType => {
                let base = self.ty(children[0], expected, free);
                // shortcut: captures are kind-checked and dropped; regions track them
                if let Some(capture) = children.get(1) {
                    let _ = self.capture(capture, free);
                }
                return base;
            }
            K::ForallType => return self.forall(node, &children, expected, free),
            K::ConstrainedType => {
                let constraints = self.constraints(children[0], free);
                let body = children
                    .get(1)
                    .map_or(Type::Error, |b| self.ty(b, expected, free));
                return Type::Forall {
                    vars: Vec::new(),
                    constraints,
                    body: Box::new(body),
                };
            }
            _ => return Type::Error,
        };
        self.expect_kind(&kind, expected, node);
        ty
    }

    /// `a -> b`, `a ->{r; e} b`.
    fn arrow(&mut self, children: &[&Node], free: Free) -> Type {
        let param = self.ty(children[0], &Kind::Type, free);
        let effects = children
            .iter()
            .find(|c| c.kind == K::EffectAnnot)
            .map(|e| Box::new(self.effects(e, free)));
        let result = children
            .iter()
            .skip(1)
            .find(|c| is_type(c.kind))
            .map_or(Type::Error, |r| self.ty(r, &Kind::Type, free));
        Type::Fun {
            param: Box::new(param),
            effects,
            result: Box::new(result),
        }
    }

    pub fn expect_kind(&mut self, found: &Kind, expected: &Kind, node: &Node) {
        if let Err((found, expected)) = self.kinds.unify(found, expected) {
            let text = self.text_of(node);
            self.error(
                codes::T002,
                self.span(node),
                format!(
                    "`{text}` has kind {found}, but {} is expected here",
                    with_article(&expected)
                ),
            );
        }
    }

    fn forall(&mut self, node: &Node, children: &[&Node], expected: &Kind, free: Free) -> Type {
        let mark = self.tyvars.len();
        let mut vars = Vec::new();
        for t in node.child_tokens().filter(|t| t.kind == K::Ident) {
            let name = t.text(self.text).to_owned();
            let kind = self.kinds.fresh();
            vars.push(Binder {
                name: name.clone(),
                kind: kind.clone(),
            });
            self.tyvars.push(TyVar {
                ty: Type::Var(name.clone()),
                name,
                kind,
            });
        }
        let body = children
            .first()
            .map_or(Type::Error, |b| self.ty(b, expected, free));
        // implicit variables met inside stay in scope for the signature
        let implicit: Vec<TyVar> = self.tyvars.drain(mark + vars.len()..).collect();
        self.tyvars.truncate(mark);
        self.tyvars.extend(implicit);
        match body {
            Type::Forall {
                vars: inner,
                constraints,
                body,
            } if inner.is_empty() => Type::Forall {
                vars,
                constraints,
                body,
            },
            body => Type::Forall {
                vars,
                constraints: Vec::new(),
                body: Box::new(body),
            },
        }
    }

    fn constraints(&mut self, node: &Node, free: Free) -> Vec<Type> {
        match node.kind {
            K::TupleType => node
                .nodes()
                .map(|c| self.ty(c, &Kind::Constraint, free))
                .collect(),
            K::ParenType => node
                .nodes()
                .flat_map(|c| self.constraints(c, free))
                .collect(),
            _ => vec![self.ty(node, &Kind::Constraint, free)],
        }
    }

    /// A name or an application `F a b`, with its kind.
    fn applied(&mut self, node: &Node, children: &[&Node], free: Free) -> (Type, Kind) {
        let (head_node, args) = match node.kind {
            K::AppType => (children[0], &children[1..]),
            _ => (node, &[][..]),
        };
        let (head, mut kind) = if head_node.kind == K::NameType {
            match self.type_name(head_node, free) {
                Some(found) => found,
                None => return (Type::Error, Kind::Type),
            }
        } else {
            let kind = self.kinds.fresh();
            (self.ty(head_node, &kind, free), kind)
        };
        if let Type::Con { module, name } = &head
            && let Some(Form::Alias { params, .. }) =
                self.env.type_def(module, name).map(|d| &d.form)
            && params.len() > args.len()
        {
            let n = params.len();
            self.error(
                codes::T002,
                self.span(node),
                format!(
                    "type alias `{name}` needs {n} type argument(s), here it has {}",
                    args.len()
                ),
            );
            return (Type::Error, Kind::Type);
        }
        let mut ty = head;
        for arg in args {
            let (param, result) = match self.kinds.resolve(&kind) {
                Kind::Arrow(p, r) => (*p, *r),
                Kind::Meta(_) => {
                    let (p, r) = (self.kinds.fresh(), self.kinds.fresh());
                    let _ = self.kinds.unify(&kind, &Kind::arrow(p.clone(), r.clone()));
                    (p, r)
                }
                other => {
                    let text = self.text_of(head_node);
                    self.error(
                        codes::T002,
                        self.span(arg),
                        format!("`{text}` has kind {other} and takes no more type arguments"),
                    );
                    return (Type::Error, Kind::Type);
                }
            };
            ty = Type::app(ty, self.ty(arg, &param, free));
            kind = result;
        }
        (ty, kind)
    }

    /// A type variable, `none`, or a type constructor by its (qualified) name.
    fn type_name(&mut self, node: &Node, free: Free) -> Option<(Type, Kind)> {
        let tokens: Vec<_> = node.child_tokens().filter(|t| t.kind != K::Dot).collect();
        let first = tokens.first()?;
        match first.kind {
            K::NoneKw => return Some((Type::empty_row(), self.kinds.fresh())),
            K::Ident => {
                let name = first.text(self.text);
                if let Some(v) = self.tyvars.iter().rev().find(|v| v.name == name) {
                    return Some((v.ty.clone(), v.kind.clone()));
                }
                let kind = self.kinds.fresh();
                let ty = match free {
                    Free::Quantify => Type::Var(name.to_owned()),
                    Free::Fresh => self.fresh(),
                    Free::Reject => {
                        self.error(
                            codes::T001,
                            self.span(node),
                            format!("unknown type variable `{name}`: declare it as a parameter"),
                        );
                        return None;
                    }
                };
                self.tyvars.push(TyVar {
                    name: name.to_owned(),
                    ty: ty.clone(),
                    kind: kind.clone(),
                });
                return Some((ty, kind));
            }
            _ => {}
        }
        let parts: Vec<&str> = tokens.iter().map(|t| t.text(self.text)).collect();
        let (name, qualifier) = parts.split_last()?;
        let qualifier = qualifier.join(".");
        let (module, def) = self.find_type(node, &qualifier, name)?;
        let ty = if self.service_module(&qualifier, name).is_some() {
            Type::con(&module, &format!("{qualifier}.{name}"))
        } else {
            Type::con(&module, name)
        };
        match def.form {
            Form::Kind => {
                self.error(
                    codes::T002,
                    self.span(node),
                    format!("`{name}` is a kind, not a type"),
                );
                None
            }
            _ => Some((ty, def.kind)),
        }
    }

    /// The definition of a type-level name; `None` after a diagnostic or
    /// for a name from a module that could not be checked.
    pub fn find_type(
        &mut self,
        node: &Node,
        qualifier: &str,
        name: &str,
    ) -> Option<(String, super::env::TypeDef)> {
        let module = if qualifier.is_empty() {
            match self.env.find(Namespace::Type, name) {
                Found::Entity(e) => e.module,
                Found::Unknown => return None,
                Found::Missing => {
                    self.error(
                        codes::T001,
                        self.span(node),
                        format!("unknown type `{name}`"),
                    );
                    return None;
                }
            }
        } else if let Some(module) = self.service_module(qualifier, name) {
            module
        } else {
            let Some((_, interface)) = self.env.qualified_module(qualifier) else {
                self.error(
                    codes::T001,
                    self.span(node),
                    format!("unknown module `{qualifier}`"),
                );
                return None;
            };
            interface?
                .find(Namespace::Type, name)?
                .entity
                .module
                .clone()
        };
        let def = match self.env.type_def(&module, name) {
            Some(def) => def.clone(),
            None => self
                .env
                .type_def(&module, &format!("{qualifier}.{name}"))?
                .clone(),
        };
        Some((module, def))
    }

    /// The module of the service `S` in `S.Impl`, when `S` is not a module.
    fn service_module(&self, qualifier: &str, name: &str) -> Option<String> {
        if name != "Impl" || self.env.qualified_module(qualifier).is_some() {
            return None;
        }
        let Found::Entity(entity) = self.env.find(Namespace::Type, qualifier) else {
            return None;
        };
        matches!(
            self.env
                .type_def(&entity.module, qualifier)
                .map(|d| &d.form),
            Some(Form::Service { .. })
        )
        .then_some(entity.module)
    }
}

/// `a Type`, `an ErrorRow`.
pub(super) fn with_article(kind: &Kind) -> String {
    let text = kind.to_string();
    let vowel = text.starts_with(['A', 'E', 'I', 'O', 'U']);
    format!("{} {text}", if vowel { "an" } else { "a" })
}
