//! Type-level declarations of a module: kinds of its types, services and
//! classes (inferred together, so declarations may refer to each other in any
//! order), alias bodies, and the types of constructors, service operations
//! and class methods.

use super::Checker;
use super::convert::{Free, TyVar, is_type};
use super::env::{Form, Operation, TypeDef};
use super::kinds::Kind;
use super::ty::{Binder, Type};
use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// A declaration whose body is converted after every name is known.
struct Pending<'n> {
    node: &'n Node,
    name: String,
    params: Vec<Binder>,
}

impl Checker<'_> {
    pub fn declare_types<'n>(&mut self, decls: &[&'n Node]) -> Vec<(&'n Node, Type)> {
        for decl in decls.iter().filter(|d| d.kind == K::KindDecl) {
            self.kind_decl(decl);
        }
        let mut pending = Vec::new();
        for decl in decls {
            if let Some(p) = self.skeleton(decl) {
                pending.push(p);
            }
        }
        let mut refined = Vec::new();
        let mut bodies = Vec::new();
        for p in &pending {
            self.with_params(&p.params, |this| {
                bodies.push(this.body(p, &mut refined));
            });
        }
        for (p, body) in pending.iter().zip(bodies) {
            self.finish(p, body);
        }
        for p in pending.iter().filter(|p| p.node.kind == K::ErrorDecl) {
            if let Some(payload) = p.node.nodes().find(|n| is_type(n.kind)) {
                self.check_error_payload(&p.name, self.span(payload));
            }
        }
        self.break_alias_cycles(&pending);
        refined
    }

    /// A cycle of transparent aliases is reported by name resolution; its
    /// members become nominal here, so expanding them terminates.
    fn break_alias_cycles(&mut self, pending: &[Pending]) {
        let module = self.env.module.clone();
        let aliases: Vec<&str> = pending
            .iter()
            .map(|p| p.name.as_str())
            .filter(|name| {
                matches!(
                    self.env.type_def(&module, name).map(|d| &d.form),
                    Some(Form::Alias { opaque: false, .. })
                )
            })
            .collect();
        let edges: Vec<Vec<usize>> = aliases
            .iter()
            .map(|name| {
                let Some(Form::Alias { body, .. }) =
                    self.env.type_def(&module, name).map(|d| &d.form)
                else {
                    return Vec::new();
                };
                let mut out = Vec::new();
                body.visit(&mut |t| {
                    if let Type::Con { module: m, name: n } = t
                        && *m == module
                        && let Some(i) = aliases.iter().position(|a| a == n)
                    {
                        out.push(i);
                    }
                });
                out
            })
            .collect();
        let cyclic: Vec<String> = (0..aliases.len())
            .filter(|&i| crate::resolve::shortest_cycle(i, &edges).is_some())
            .map(|i| aliases[i].to_owned())
            .collect();
        for name in cyclic {
            if let Some(def) = self.env.type_def_mut(&name) {
                def.form = Form::Nominal;
            }
        }
    }

    fn kind_decl(&mut self, decl: &Node) {
        let Some(name) = self.con_token(decl) else {
            return;
        };
        let kind = Kind::Named {
            module: self.env.module.clone(),
            name: name.clone(),
        };
        self.env.define_type(
            &name,
            TypeDef {
                kind: kind.clone(),
                form: Form::Kind,
            },
        );
        self.env.declare(Namespace::Type, &name);
        for variant in decl.nodes().filter(|n| n.kind == K::Variant) {
            if let Some(member) = self.con_token(variant) {
                self.env.define_type(
                    &member,
                    TypeDef {
                        kind: kind.clone(),
                        form: Form::KindMember,
                    },
                );
                self.env.declare(Namespace::Type, &member);
            }
        }
    }

    /// Registers the name and kind of a declaration before any body is read.
    fn skeleton<'n>(&mut self, decl: &'n Node) -> Option<Pending<'n>> {
        let result = match decl.kind {
            K::TypeDecl | K::ErrorDecl => Kind::Type,
            K::ServiceDecl => Kind::Service,
            K::ClassDecl => Kind::Constraint,
            _ => return None,
        };
        let name = self.con_token(decl)?;
        let params: Vec<Binder> = decl
            .nodes()
            .filter(|n| n.kind == K::TypeParam)
            .filter_map(|param| {
                let name = param.child_tokens().find(|t| t.kind == K::Ident)?;
                let kind = match param.nodes().next() {
                    Some(k) => self.kind(k),
                    None => self.kinds.fresh(),
                };
                Some(Binder {
                    name: name.text(self.text).to_owned(),
                    kind,
                })
            })
            .collect();
        let variants: Vec<String> = decl
            .nodes()
            .filter(|n| n.kind == K::Variant)
            .filter_map(|v| self.con_token(v))
            .collect();
        let has_body = decl.nodes().any(|n| is_type(n.kind));
        let (form, result) = match decl.kind {
            K::TypeDecl if decl.has_token(K::RefinedKw) => (Form::Nominal, result),
            K::TypeDecl if !variants.is_empty() => (
                Form::Adt {
                    constructors: variants,
                },
                result,
            ),
            K::TypeDecl if has_body => (
                Form::Alias {
                    params: params.iter().map(|p| p.name.clone()).collect(),
                    body: Type::Error,
                    opaque: decl.has_token(K::OpaqueKw),
                },
                self.kinds.fresh(),
            ),
            K::TypeDecl => (Form::Nominal, result),
            K::ErrorDecl => (Form::Error, result),
            K::ServiceDecl => (
                Form::Service {
                    params: params.iter().map(|p| p.name.clone()).collect(),
                    operations: Vec::new(),
                },
                result,
            ),
            _ => (Form::Class, result),
        };
        let kind = Kind::arrows(params.iter().map(|p| p.kind.clone()), result);
        self.env.define_type(&name, TypeDef { kind, form });
        self.env.declare(Namespace::Type, &name);
        Some(Pending {
            node: decl,
            name,
            params,
        })
    }

    fn with_params<T>(&mut self, params: &[Binder], f: impl FnOnce(&mut Self) -> T) -> T {
        let mark = self.tyvars.len();
        self.tyvars.extend(params.iter().map(|p| TyVar {
            name: p.name.clone(),
            ty: Type::Var(p.name.clone()),
            kind: p.kind.clone(),
        }));
        let out = f(self);
        self.tyvars.truncate(mark);
        out
    }

    /// Converts what a declaration defines with its parameters in scope.
    fn body<'n>(&mut self, p: &Pending<'n>, refined: &mut Vec<(&'n Node, Type)>) -> Body {
        let decl = p.node;
        match decl.kind {
            K::TypeDecl => {
                let variants: Vec<&Node> = decl.nodes().filter(|n| n.kind == K::Variant).collect();
                if !variants.is_empty() {
                    return Body::Variants(
                        variants
                            .iter()
                            .filter_map(|v| Some((self.con_token(v)?, self.payload(v))))
                            .collect(),
                    );
                }
                let Some(body) = decl.nodes().find(|n| is_type(n.kind)) else {
                    return Body::None;
                };
                if decl.has_token(K::RefinedKw) {
                    let base = self.ty(body, &Kind::Type, Free::Reject);
                    if let Some(predicate) = decl.nodes().find(|n| n.kind == K::Refinement) {
                        refined.push((predicate, base));
                    }
                    return Body::None;
                }
                let Some(TypeDef { kind, .. }) = self.env.type_def(&self.env.module, &p.name)
                else {
                    return Body::None;
                };
                let mut result = kind.clone();
                for _ in &p.params {
                    result = match self.kinds.resolve(&result) {
                        Kind::Arrow(_, r) => *r,
                        other => other,
                    };
                }
                Body::Alias(self.ty(body, &result, Free::Reject))
            }
            K::ErrorDecl => {
                let payload = decl
                    .nodes()
                    .find(|n| is_type(n.kind))
                    .map(|t| self.ty(t, &Kind::Type, Free::Reject));
                Body::Variants(vec![(p.name.clone(), payload.into_iter().collect())])
            }
            K::ServiceDecl => Body::Members(self.members(decl, &[])),
            K::ClassDecl => {
                for context in decl.nodes().filter(|n| n.kind == K::Context) {
                    for c in context.nodes() {
                        self.ty(c, &Kind::Constraint, Free::Reject);
                    }
                }
                let head = Type::apps(
                    Type::con(&self.env.module, &p.name),
                    p.params.iter().map(|b| Type::Var(b.name.clone())),
                );
                Body::Members(self.members(decl, &[head]))
            }
            _ => Body::None,
        }
    }

    fn payload(&mut self, variant: &Node) -> Vec<Type> {
        variant
            .nodes()
            .filter(|n| is_type(n.kind))
            .map(|t| self.ty(t, &Kind::Type, Free::Reject))
            .collect()
    }

    /// Operations of a service or methods of a class, by name.
    fn members(&mut self, decl: &Node, constraints: &[Type]) -> Vec<(String, Type)> {
        let members = decl
            .nodes()
            .filter(|n| n.kind == K::Members)
            .flat_map(Node::nodes);
        let mut out = Vec::new();
        for member in members.filter(|m| matches!(m.kind, K::Signature | K::ValDecl)) {
            let (Some(name), Some(ty)) = (
                member.child_tokens().find(|t| t.kind == K::Ident),
                member.nodes().find(|n| is_type(n.kind)),
            ) else {
                continue;
            };
            let (scheme, _) = self.signature(ty);
            if decl.kind == K::ServiceDecl && !super::services::has_runtime_arrow(&scheme) {
                let name = name.text(self.text);
                self.error(
                    codes::E006,
                    self.span(member),
                    format!(
                        "service operation `{name}` must be a runtime function: running it looks the service up, so at least one arrow is `->{{…}}`"
                    ),
                );
            }
            let scheme = match scheme {
                Type::Forall {
                    vars,
                    constraints: own,
                    body,
                } => Type::Forall {
                    vars,
                    constraints: constraints.iter().cloned().chain(own).collect(),
                    body,
                },
                body if constraints.is_empty() => body,
                body => Type::Forall {
                    vars: Vec::new(),
                    constraints: constraints.to_vec(),
                    body: Box::new(body),
                },
            };
            out.push((name.text(self.text).to_owned(), scheme));
        }
        out
    }

    /// Fixes the kinds and stores what the declaration defines.
    fn finish(&mut self, p: &Pending, body: Body) {
        let params: Vec<Binder> = p
            .params
            .iter()
            .map(|b| Binder {
                name: b.name.clone(),
                kind: self.kinds.zonk(&b.kind, true),
            })
            .collect();
        let module = self.env.module.clone();
        let Some(def) = self.env.type_def_mut(&p.name) else {
            return;
        };
        def.kind = self.kinds.zonk(&def.kind, true);
        let quantify = |body: Type| {
            if params.is_empty() {
                return body;
            }
            match body {
                Type::Forall {
                    vars,
                    constraints,
                    body,
                } => Type::Forall {
                    vars: params.iter().cloned().chain(vars).collect(),
                    constraints,
                    body,
                },
                body => Type::Forall {
                    vars: params.clone(),
                    constraints: Vec::new(),
                    body: Box::new(body),
                },
            }
        };
        match body {
            Body::None => {}
            Body::Alias(ty) => {
                if let Form::Alias { body, .. } = &mut def.form {
                    *body = ty;
                }
            }
            Body::Variants(variants) => {
                let result = Type::apps(
                    Type::con(&module, &p.name),
                    params.iter().map(|b| Type::Var(b.name.clone())),
                );
                for (name, payload) in variants {
                    let ty = payload
                        .into_iter()
                        .rev()
                        .fold(result.clone(), |acc, t| Type::pure_fun(t, acc));
                    let entity = self.env.entity(Namespace::Constructor, &name);
                    let scheme = self.default_kinds(&quantify(ty));
                    self.env.set_value(entity.clone(), scheme);
                    self.env.set_parent(entity, &p.name);
                    self.env.declare(Namespace::Constructor, &name);
                }
            }
            Body::Members(members) => {
                let class = matches!(def.form, Form::Class);
                let mut operations = Vec::new();
                for (name, ty) in members {
                    let scheme = self.default_kinds(&quantify(ty));
                    if class {
                        let entity = self.env.entity(Namespace::Value, &name);
                        self.env.set_value(entity, scheme);
                        self.env.declare(Namespace::Value, &name);
                    } else {
                        operations.push(Operation { name, ty: scheme });
                    }
                }
                if let Some(TypeDef {
                    form:
                        Form::Service {
                            operations: ops, ..
                        },
                    ..
                }) = self.env.type_def_mut(&p.name)
                {
                    *ops = operations;
                }
                let implementation = self
                    .env
                    .type_def(&module, &p.name)
                    .and_then(super::services::impl_def);
                if let Some(implementation) = implementation {
                    self.env
                        .define_type(&format!("{}.Impl", p.name), implementation);
                }
            }
        }
    }

    pub fn con_token(&self, node: &Node) -> Option<String> {
        node.child_tokens()
            .find(|t| t.kind == K::Con)
            .map(|t| t.text(self.text).to_owned())
    }
}

enum Body {
    None,
    Alias(Type),
    /// Constructors with their payload types.
    Variants(Vec<(String, Vec<Type>)>),
    Members(Vec<(String, Type)>),
}
