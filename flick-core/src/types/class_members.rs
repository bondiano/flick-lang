//! The members of classes and services: functional dependencies, the
//! signatures of methods and operations, and the defaults in a class body.

use super::Checker;
use super::convert::is_type;
use super::env::{Form, Fundep, TypeDef};
use super::ty::{Binder, Type};
use crate::diagnostics::codes;
use crate::iface::Namespace;
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

/// Members by name, each with its scheme.
type Signed = Vec<(String, Type)>;

impl Checker<'_> {
    /// `a b -> c` by parameter positions; `None` after a diagnostic.
    pub(super) fn fundep(&mut self, node: &Node, params: &[Binder]) -> Option<Fundep> {
        let mut sides = [Vec::new(), Vec::new()];
        let mut side = 0;
        for t in node.child_tokens() {
            match t.kind {
                K::Arrow => side = 1,
                K::Ident => {
                    let name = t.text(self.text);
                    let Some(i) = params.iter().position(|p| p.name == name) else {
                        self.error(
                            codes::T001,
                            self.token_span(t),
                            format!("`{name}` is not a parameter of the class"),
                        );
                        return None;
                    };
                    sides[side].push(i);
                }
                _ => {}
            }
        }
        let [from, to] = sides;
        Some(Fundep { from, to })
    }

    /// Operations of a service or methods of a class, by name.
    pub(super) fn members(&mut self, decl: &Node, constraints: &[Type]) -> Vec<(String, Type)> {
        let signed = self.signatures(decl, constraints);
        signed.into_iter().map(|(name, ty, _)| (name, ty)).collect()
    }

    /// The methods of a class, and the default signature of each method
    /// whose signature the body repeats: `m : t` and then `m : (C x, …) => t`,
    /// whose constraints only the class's default definition of `m` needs.
    pub(super) fn class_members(&mut self, decl: &Node, head: Type) -> (Signed, Signed) {
        let params: Vec<Type> = head.spine().1.into_iter().cloned().collect();
        let (mut methods, mut defaults): (Signed, Signed) = (Vec::new(), Vec::new());
        for (name, ty, span) in self.signatures(decl, &[head]) {
            let Some((_, method)) = methods.iter().find(|(m, _)| *m == name) else {
                methods.push((name, ty));
                continue;
            };
            let wrong = if defaults.iter().any(|(m, _)| *m == name) {
                Some(format!("`{name}` has a default signature already"))
            } else {
                self.default_signature_wrong(&name, method, &ty, &params)
            };
            match wrong {
                Some(message) => self.error(codes::C010, span, message),
                None => defaults.push((name, ty)),
            }
        }
        (methods, defaults)
    }

    /// Declares the methods of `class` and keeps its default signatures,
    /// each scheme quantified over the class's parameters.
    pub(super) fn class_schemes(
        &mut self,
        class: &str,
        members: Signed,
        signed: Signed,
        quantify: &dyn Fn(Type) -> Type,
    ) {
        let signed: Signed = (signed.into_iter())
            .map(|(n, t)| (n, self.default_kinds(&quantify(t))))
            .collect();
        if let Some(TypeDef {
            form: Form::Class { default_sigs, .. },
            ..
        }) = self.env.type_def_mut(class)
        {
            *default_sigs = signed;
        }
        let members = members.into_iter().map(|(n, t)| (n, quantify(t))).collect();
        self.declare_methods(class, members);
    }

    /// Why `default` is no default signature of the method `name` of type
    /// `method`: it has another type, the method has constraints of its own,
    /// or it constrains a variable of the method's, which an instance does
    /// not know. Each scheme's first constraint is the class's.
    fn default_signature_wrong(
        &self,
        name: &str,
        method: &Type,
        default: &Type,
        params: &[Type],
    ) -> Option<String> {
        let parts = |t: &Type| match t {
            Type::Forall {
                constraints, body, ..
            } => (
                constraints.get(1..).unwrap_or_default().to_vec(),
                (**body).clone(),
            ),
            body => (Vec::new(), body.clone()),
        };
        let ((own, ty), (extra, written)) = (parts(method), parts(default));
        if !own.is_empty() {
            return Some(format!(
                "`{name}` has constraints of its own: a default signature is for a method without them"
            ));
        }
        if written != ty {
            let shown = self.show(&ty);
            return Some(format!(
                "the default signature of `{name}` must have its type, `{shown}`, with constraints before it"
            ));
        }
        let mut vars = Vec::new();
        ty.contains(&mut |t| {
            if let Type::Var(v) = t
                && !params.contains(t)
            {
                vars.push(v.clone());
            }
            false
        });
        let constrained = (extra.iter()).find_map(|c| {
            let mut found = None;
            c.contains(&mut |t| {
                if let Type::Var(v) = t
                    && vars.contains(v)
                {
                    found = Some(v.clone());
                }
                false
            });
            found
        });
        constrained.map(|v| format!(
            "the default signature of `{name}` constrains `{v}`, a variable of the method: an instance knows only the class's parameters"
        ))
    }

    /// The signatures of a service's or a class's body, by name, each after
    /// `constraints`, and where it is.
    fn signatures(&mut self, decl: &Node, constraints: &[Type]) -> Vec<(String, Type, Span)> {
        let members = decl
            .nodes()
            .filter(|n| n.kind == K::Members)
            .flat_map(Node::nodes);
        let mut out = Vec::new();
        for member in members.filter(|m| m.kind == K::Signature) {
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
            out.push((name.text(self.text).to_owned(), scheme, self.span(member)));
        }
        out
    }
}

impl Checker<'_> {
    /// The methods of a class are values whose parent is the class.
    pub(super) fn declare_methods(&mut self, class: &str, members: Vec<(String, Type)>) {
        for (name, ty) in members {
            let scheme = self.default_kinds(&ty);
            let entity = self.env.entity(Namespace::Value, &name);
            self.env.set_value(entity.clone(), scheme);
            self.env.set_parent(entity, class);
            self.env.declare(Namespace::Value, &name);
        }
    }
}

/// The methods of a class, and those with a default in its body.
pub(super) fn method_names(
    decl: &Node,
    text: &str,
    members: &[(String, Type)],
) -> (Vec<String>, Vec<String>) {
    let methods: Vec<String> = members.iter().map(|(name, _)| name.clone()).collect();
    let defaults = class_defaults(decl, text)
        .into_iter()
        .filter_map(|(name, _)| methods.contains(&name).then_some(name))
        .collect();
    (methods, defaults)
}

/// The `let` definitions in the body of class `decl`, by name.
pub fn class_defaults<'n>(decl: &'n Node, text: &str) -> Vec<(String, &'n Node)> {
    decl.nodes()
        .filter(|n| n.kind == K::Members)
        .flat_map(Node::nodes)
        .filter(|n| n.kind == K::LetDecl)
        .filter_map(|n| Some((super::let_parts(n, text).name?, n)))
        .collect()
}

/// The default signature of `method` in the body of class `decl`: the
/// second signature of that name.
pub(super) fn default_signature<'n>(decl: &'n Node, text: &str, method: &str) -> Option<&'n Node> {
    (decl
        .nodes()
        .filter(|n| n.kind == K::Members)
        .flat_map(Node::nodes))
    .filter(|n| n.kind == K::Signature)
    .filter(|n| (n.child_tokens()).any(|t| t.kind == K::Ident && t.text(text) == method))
    .nth(1)
}
