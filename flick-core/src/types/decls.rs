//! Type-level declarations of a module: kinds of its types, services and
//! classes (inferred together, so declarations may refer to each other in any
//! order), alias bodies, and the types of constructors, service operations
//! and class methods.

use std::collections::HashMap;
use std::mem;

use super::Checker;
use super::class_members::method_names;
use super::convert::{Free, TyVar, is_type};
use super::env::{Abi, Form, Fundep, TypeDef};
use super::gadt::scheme::{Variant, quantify};
use super::kinds::Kind;
use super::services::{OpClass, Operation};
use super::ty::{Binder, Type};
use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::resolve;
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
        let mut bodies: Vec<Body> = pending.iter().map(|_| Body::None).collect();
        for i in self.conversion_order(&pending) {
            let p = &pending[i];
            bodies[i] = self.with_params(&p.params, |this| this.body(p, &mut refined));
        }
        for (p, body) in pending.iter().zip(bodies) {
            self.finish(p, body);
        }
        for p in pending.iter().filter(|p| p.node.kind == K::ErrorDecl) {
            if let Some(variant) = p.node.nodes().find(|n| n.kind == K::Variant) {
                self.check_error_payload(&p.name, self.span(variant));
            }
        }
        self.break_alias_cycles(&pending);
        for p in &pending {
            self.check_abi(p.node, &p.name);
        }
        refined
    }

    /// The order bodies are converted in: aliases first, each after the aliases
    /// it names, so that every later declaration sees what an alias stands for.
    /// `= {}` keeps its place: the uses before it decide whether it is a record.
    fn conversion_order(&self, pending: &[Pending]) -> Vec<usize> {
        let module = &self.env.module;
        let converted_early = |p: &Pending| {
            matches!(
                self.env.type_def(module, &p.name).map(|d| &d.form),
                Some(Form::Alias { .. })
            ) && !p
                .node
                .nodes()
                .any(|n| n.kind == K::RowType && n.nodes().next().is_none())
        };
        let aliases: HashMap<&str, usize> = pending
            .iter()
            .enumerate()
            .filter(|(_, p)| converted_early(p))
            .map(|(i, p)| (p.name.as_str(), i))
            .collect();
        let mut order = Vec::with_capacity(pending.len());
        let mut seen = vec![false; pending.len()];
        for (i, p) in pending.iter().enumerate() {
            if aliases.contains_key(p.name.as_str()) {
                self.after_named(i, pending, &aliases, &mut seen, &mut order);
            }
        }
        order.extend((0..pending.len()).filter(|&i| !seen[i]));
        order
    }

    /// Adds alias `i` after the aliases it names; a cycle, which name resolution
    /// reports, ends where it comes back.
    fn after_named(
        &self,
        i: usize,
        pending: &[Pending],
        aliases: &HashMap<&str, usize>,
        seen: &mut [bool],
        order: &mut Vec<usize>,
    ) {
        if mem::replace(&mut seen[i], true) {
            return;
        }
        for name in resolve::alias_names(pending[i].node, self.text) {
            if let Some(&named) = aliases.get(name.as_str()) {
                self.after_named(named, pending, aliases, seen, order);
            }
        }
        order.push(i);
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
            .filter(|&i| resolve::shortest_cycle(i, &edges).is_some())
            .map(|i| aliases[i].to_owned())
            .collect();
        for name in cyclic {
            if let Some(def) = self.env.type_def_mut(&name) {
                def.form = Form::NOMINAL;
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
            K::TypeDecl if decl.has_token(K::RefinedKw) => (Form::NOMINAL, result),
            K::TypeDecl if !variants.is_empty() => (
                Form::Adt {
                    constructors: variants,
                    abi: Abi::Tagged,
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
            K::TypeDecl => (Form::NOMINAL, result),
            K::ErrorDecl => (Form::Error, result),
            K::ServiceDecl => (
                Form::Service {
                    params: params.iter().map(|p| p.name.clone()).collect(),
                    operations: Vec::new(),
                    effect: decl.has_token(K::EffectKw),
                },
                result,
            ),
            _ => (
                Form::Class {
                    params: params.iter().map(|p| p.name.clone()).collect(),
                    supers: Vec::new(),
                    fundeps: Vec::new(),
                    methods: Vec::new(),
                    defaults: Vec::new(),
                    default_sigs: Vec::new(),
                    derive: false,
                },
                result,
            ),
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
                        variants.iter().filter_map(|v| self.variant(v)).collect(),
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
                let converted = self.ty(body, &result, Free::Reject);
                if decl.has_token(K::OpaqueKw) && self.kinds.resolve(&result).is_row() {
                    self.error(
                        codes::T002,
                        self.span(decl),
                        format!(
                            "`{}` cannot be opaque: a set is its row, which a signature that names it must show",
                            p.name
                        ),
                    );
                }
                if let Some(TypeDef {
                    form: Form::Alias { body, .. },
                    ..
                }) = self.env.type_def_mut(&p.name)
                {
                    *body = converted;
                }
                Body::None
            }
            K::ErrorDecl => {
                let payload = decl
                    .nodes()
                    .find(|n| n.kind == K::Variant)
                    .map(|v| self.payload(v))
                    .unwrap_or_default();
                Body::Variants(vec![Variant {
                    name: p.name.clone(),
                    payload,
                    constraints: Vec::new(),
                    indexed: None,
                }])
            }
            K::ServiceDecl => Body::Members(self.operations(decl)),
            K::ClassDecl => {
                let mut supers = Vec::new();
                for context in decl.nodes().filter(|n| n.kind == K::Context) {
                    for c in context.nodes() {
                        supers.extend(self.constraints(c, Free::Reject));
                    }
                }
                let fundeps = decl
                    .nodes()
                    .filter(|n| n.kind == K::Fundep)
                    .filter_map(|f| self.fundep(f, &p.params))
                    .collect();
                let head = Type::apps(
                    Type::con(&self.env.module, &p.name),
                    p.params.iter().map(|b| Type::Var(b.name.clone())),
                );
                let (members, defaults) = self.class_members(decl, head);
                Body::Class(supers, fundeps, members, defaults)
            }
            _ => Body::None,
        }
    }

    /// Fixes the kinds and stores what the declaration defines.
    #[allow(clippy::too_many_lines)]
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
        let quantify = |body: Type| quantify(&params, body);
        match body {
            Body::None => {}
            Body::Variants(variants) => {
                let error = p.node.kind == K::ErrorDecl;
                self.constructor_schemes(&p.name, error, &params, variants);
            }
            Body::Class(supers, fundeps, members, signed) => {
                let (methods, defaults) = method_names(p.node, self.text, &members);
                def.form = Form::Class {
                    params: params.iter().map(|b| b.name.clone()).collect(),
                    supers,
                    fundeps,
                    methods,
                    defaults,
                    default_sigs: Vec::new(),
                    derive: self.derived.iter().any(|d| d.class == p.name),
                };
                self.class_schemes(&p.name, members, signed, &quantify);
            }
            Body::Members(members) => {
                if members
                    .iter()
                    .any(|(_, _, class)| *class == OpClass::Control)
                {
                    super::ty::declare_control_effect(format!("{module}.{}", p.name));
                }
                let operations = members
                    .into_iter()
                    .map(|(name, ty, class)| Operation {
                        name,
                        ty: self.default_kinds(&quantify(ty)),
                        class,
                    })
                    .collect();
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
    Variants(Vec<Variant>),
    Members(Vec<(String, Type, OpClass)>),
    /// Superclasses, functional dependencies, methods and default
    /// signatures.
    Class(
        Vec<Type>,
        Vec<Fundep>,
        Vec<(String, Type)>,
        Vec<(String, Type)>,
    ),
}
