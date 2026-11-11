//! Type-level declarations of a module: kinds of its types, services and
//! classes (inferred together, so declarations may refer to each other in any
//! order), alias bodies, and the types of constructors, service operations
//! and class methods.

use std::collections::HashMap;
use std::mem;

use super::Checker;
use super::class_members::method_names;
use super::convert::{Free, TyVar, is_type};
use super::env::{Abi, Form, Fundep, ModuleNominalKind, ModuleRequirement, TypeDef};
use super::gadt::scheme::{Variant, quantify};
use super::kinds::Kind;
use super::services::{OpClass, Operation};
use super::ty::{Binder, Type};
use crate::api_diff;
use crate::diagnostics::{Diagnostic, codes};
use crate::iface::{Entity, Namespace};
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
            if let Some(def) = self.env.type_def_mut(&p.name) {
                match &bodies[i] {
                    Body::ModuleContract(requirements) => {
                        def.form = Form::ModuleContract {
                            requirements: requirements.clone(),
                        }
                    }
                    Body::ModuleSignature(signature) => {
                        def.form = Form::ModuleSignature {
                            signature: Box::new(signature.clone()),
                        }
                    }
                    _ => {}
                }
            }
        }
        // before a constructor's scheme expands an alias it names
        self.break_alias_cycles(&pending);
        for (p, body) in pending.iter().zip(bodies) {
            self.finish(p, body);
        }
        self.finish_module_contracts(&pending);
        // Aliases are converted before contract bodies so a contract may use
        // an alias declared later. Validate `Module Sig` again once the local
        // contract's members are known.
        for p in pending.iter().filter(|p| p.node.kind == K::TypeDecl) {
            let Some(TypeDef {
                form: Form::Alias { body, .. },
                ..
            }) = self.env.type_def(&self.env.module, &p.name)
            else {
                continue;
            };
            let body = body.clone();
            let mut contracts = Vec::new();
            body.visit(&mut |ty| {
                let (head, args) = ty.spine();
                if matches!(head, Type::Con { module, name } if module == "Prelude" && name == "Module")
                    && let [Type::Con { module, name }] = &args[..]
                    && module == &self.env.module
                {
                    contracts.push(name.clone());
                }
            });
            for contract in contracts {
                let abstract_names: Vec<String> = self
                    .env
                    .type_def(&self.env.module, &contract)
                    .and_then(|def| match &def.form {
                        Form::ModuleContract { requirements } => Some(requirements),
                        _ => None,
                    })
                    .into_iter()
                    .flatten()
                    .filter_map(|r| match r {
                        ModuleRequirement::Nominal {
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
                        self.span(p.node),
                        format!("`Module {contract}` cannot carry abstract class or service `{name}`; use a manifest identity in the contract"),
                    );
                }
            }
        }
        for p in pending.iter().filter(|p| p.node.kind == K::ErrorDecl) {
            if let Some(variant) = p.node.nodes().find(|n| n.kind == K::Variant) {
                self.check_error_payload(&p.name, self.span(variant));
            }
        }
        for p in &pending {
            self.check_abi(p.node, &p.name);
        }
        refined
    }

    /// Registers the name and kind of a declaration before any body is read.
    fn skeleton<'n>(&mut self, decl: &'n Node) -> Option<Pending<'n>> {
        let result = match decl.kind {
            K::TypeDecl | K::ErrorDecl => Kind::Type,
            K::ModuleTypeDecl => Kind::Module,
            K::ServiceDecl => Kind::Service,
            K::ClassDecl => Kind::Constraint,
            _ => return None,
        };
        let name = self.con_token(decl)?;
        if decl.kind == K::ServiceDecl
            && (decl.nodes().filter(|n| n.kind == K::Members))
                .flat_map(Node::nodes)
                .any(|m| m.kind == K::Signature && m.has_token(K::ControlKw))
        {
            self.control_effects.insert(name.clone());
        }
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
            K::ModuleTypeDecl => (
                Form::ModuleContract {
                    requirements: Vec::new(),
                },
                result,
            ),
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
        let mark = self.cx.tyvars.len();
        self.cx.tyvars.extend(params.iter().map(|p| TyVar {
            name: p.name.clone(),
            ty: Type::Var(p.name.clone()),
            kind: p.kind.clone(),
        }));
        let out = f(self);
        self.cx.tyvars.truncate(mark);
        out
    }

    /// Converts what a declaration defines with its parameters in scope.
    fn body<'n>(&mut self, p: &Pending<'n>, refined: &mut Vec<(&'n Node, Type)>) -> Body {
        let decl = p.node;
        match decl.kind {
            K::ModuleTypeDecl => {
                if let Some(signature) = decl.nodes().find(|n| n.kind != K::Members) {
                    match self.static_module_signature(signature) {
                        super::ModuleSignature::Structure { requirements } => {
                            Body::ModuleContract(requirements)
                        }
                        signature @ super::ModuleSignature::Parameterized { .. } => {
                            Body::ModuleSignature(signature)
                        }
                    }
                } else {
                    Body::ModuleContract(self.module_contract(decl))
                }
            }
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
            Body::ModuleContract(requirements) => {
                def.form = Form::ModuleContract { requirements };
            }
            Body::ModuleSignature(signature) => {
                def.form = Form::ModuleSignature {
                    signature: Box::new(signature),
                };
            }
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
}

include!("decls_contracts.rs");
include!("decls_order.rs");
include!("decls_signatures.rs");

enum Body {
    ModuleSignature(super::ModuleSignature),
    None,
    ModuleContract(Vec<ModuleRequirement>),
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

include!("decls_nominal_snapshots.rs");
