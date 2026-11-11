//! From the syntax of types, rows and kinds to [`Type`] and [`Kind`], checking
//! kinds on the way: every position decides the kind it expects, and the
//! kinds of type variables follow from where they are used.

use std::mem;

use super::Checker;
use super::env::{Form, Found, ModuleRequirement};
use super::kinds::Kind;
use super::ty::{Binder, Row, Type};
use crate::diagnostics::codes;
use crate::diagnostics::suggest::closest;
use crate::iface::Namespace;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;
use crate::syntax::literal;
use crate::types::lang::Lang;

mod kinds;
mod omit;
mod rows;
mod set_names;
mod sets;

pub use kinds::is_type;
use kinds::with_article;
use rows::{place_braces, written_none};

fn as_value_kind(kind: Kind) -> Kind {
    match kind {
        Kind::Arrow(param, result) => Kind::Arrow(param, Box::new(as_value_kind(*result))),
        Kind::Constraint => Kind::Type,
        other => other,
    }
}

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

impl Checker<'_> {
    /// A signature's type with its new free variables quantified, and
    /// whether the signature starts with a `forall` that binds all of them.
    pub fn signature(&mut self, node: &Node) -> (Type, bool) {
        let mark = self.tyvars.len();
        let hole_ok = mem::replace(&mut self.hole_ok, true);
        let ty = self.ty(node, &Kind::Type, Free::Quantify);
        self.hole_ok = hole_ok;
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
        let ty = self.default_kinds(&ty);
        let ty = self.live_scheme(&ty);
        #[cfg(feature = "ide")]
        self.typing.signature_types.insert(node.range(), ty.clone());
        (ty, explicit)
    }

    /// A local type annotation; its unknown variables are inferred.
    pub fn annotation(&mut self, node: &Node) -> Type {
        let mark = self.tyvars.len();
        let hole_ok = mem::replace(&mut self.hole_ok, true);
        let ty = self.ty(node, &Kind::Type, Free::Fresh);
        self.hole_ok = hole_ok;
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
    #[allow(clippy::too_many_lines)]
    pub fn ty(&mut self, node: &Node, expected: &Kind, free: Free) -> Type {
        let span = self.span(node);
        let children: Vec<&Node> = node.nodes().collect();
        if let Some(path) = self.symbol_route(node, expected) {
            return path;
        }
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
                return written_none(span);
            }
            K::NameType | K::AppType => {
                let (ty, kind) = self.applied(node, &children, free);
                if ty == Type::Error {
                    return ty;
                }
                (ty, kind)
            }
            K::UnitType => (Type::lang(Lang::Unit), Kind::Type),
            K::HoleType => self.hole(span, expected),
            K::TodoType => self.todo_type(span, expected),
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
                    let mut ty = self.row(node, &expected, free);
                    place_braces(&mut ty, node, span.file);
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
                let Some(capture) = children.get(1) else {
                    return base;
                };
                let captures = self.capture(capture, free);
                return if matches!(&base, Type::Var(_) | Type::Meta(_) | Type::Skolem(_)) {
                    Type::Bounded {
                        ty: Box::new(base),
                        captures: Box::new(captures),
                    }
                } else {
                    Type::Captured {
                        ty: Box::new(base),
                        captures: Box::new(captures),
                        value: false,
                    }
                };
            }
            K::ForallType => return self.forall(node, &children, expected, free),
            K::ExistsType => {
                let [contract_node, body_node] = &children[..] else {
                    return Type::Error;
                };
                let Some(binder) = node.child_tokens().find(|t| t.kind == K::Con) else {
                    return Type::Error;
                };
                let binder = binder.text(self.text).to_owned();
                let contract = self.ty(contract_node, &Kind::Module, free);
                let Type::Con { module, name } = &contract else {
                    return Type::Error;
                };
                let Some(def) = self.env.type_def(module, name).cloned() else {
                    return Type::Error;
                };
                let Form::ModuleContract { requirements } = def.form else {
                    return Type::Error;
                };
                let mut subst = Vec::new();
                let mut types = Vec::new();
                for requirement in &requirements {
                    match requirement {
                        ModuleRequirement::Type {
                            name,
                            params,
                            manifest,
                        } => {
                            let ty = manifest.as_ref().map_or_else(
                                || Type::con(&format!("$exists:{binder}"), name),
                                |ty| ty.substitute(&subst),
                            );
                            let kind =
                                Kind::arrows(params.iter().map(|p| p.kind.clone()), Kind::Type);
                            subst.push((name.clone(), ty.clone()));
                            types.push((name.clone(), ty, kind));
                        }
                        ModuleRequirement::Nominal {
                            name,
                            identity: Some(identity),
                            ..
                        } => {
                            let ty = Type::con(&identity.module, &identity.name);
                            let kind = self
                                .env
                                .type_def(&identity.module, &identity.name)
                                .map_or(Kind::Type, |def| def.kind.clone());
                            subst.push((name.clone(), ty.clone()));
                            types.push((name.clone(), ty, kind));
                        }
                        _ => {}
                    }
                }
                self.opened_modules.push(super::module_contract::Opened {
                    alias: binder.clone(),
                    types,
                    values: Vec::new(),
                });
                let body = self.ty(body_node, &Kind::Type, free);
                self.opened_modules.pop();
                return Type::Exists {
                    binder,
                    contract: Box::new(contract),
                    body: Box::new(body),
                };
            }
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
        self.rows_settled(ty, &kind, node)
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

    pub fn constraints(&mut self, node: &Node, free: Free) -> Vec<Type> {
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
    #[allow(clippy::too_many_lines)]
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
        let first_class_module =
            matches!(&head, Type::Con { module, name } if module == "Prelude" && name == "Module");
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
        if first_class_module {
            let (_, applied) = ty.spine();
            let static_functor = if let [signature] = &applied[..]
                && let Type::Con { module, name } = self.whnf(signature)
                && let Some(definition) = self.env.type_def(&module, &name)
            {
                match &definition.form {
                    Form::ModuleSignature { signature } => signature.has_functor(),
                    Form::ModuleContract { requirements } => requirements
                        .iter()
                        .any(|r| matches!(r, ModuleRequirement::Module { .. })),
                    _ => false,
                }
            } else {
                false
            };
            if static_functor {
                self.error(
                    codes::T068,
                    self.span(node),
                    "a static functor or nested module cannot be carried by `Module Sig`; apply factories and flatten the runtime contract before packing",
                );
            }
            let (_, applied) = ty.spine();
            if let [signature] = &applied[..]
                && let Type::Con { module, name } = self.whnf(signature)
                && let Some(super::env::TypeDef {
                    form: Form::ModuleContract { requirements },
                    ..
                }) = self.env.type_def(&module, &name)
            {
                let abstract_names: Vec<String> = requirements
                    .iter()
                    .filter_map(|requirement| match requirement {
                        super::ModuleRequirement::Nominal {
                            name,
                            identity: None,
                            ..
                        } => Some(name.clone()),
                        _ => None,
                    })
                    .collect();
                for name in abstract_names {
                    self.error(
                        codes::T063,
                        self.span(node),
                        format!(
                            "`Module {}` cannot carry abstract class or service `{name}`; use a manifest identity in the contract",
                            self.text_of(args[0])
                        ),
                    );
                }
            }
        }
        (ty, kind)
    }
}

include!("convert_names.rs");
