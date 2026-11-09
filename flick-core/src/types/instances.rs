//! Methods of an instance: each one has the signature of its class method
//! with the instance's types in place of the class parameters, and no
//! constraint the class signature lacks; bodies are checked against it with
//! the instance's context assumed. A written signature is optional and, when
//! present, must match.

use super::Checker;
use super::bindings::let_parts;
use super::class_members::default_signature;
use super::classes::{ClassInfo, Dict, Outcome, Wanted, split};
use super::convert::{TyVar, is_type};
use super::env::Instance;
use super::passes_dictionary;
use super::ty::Type;
use crate::diagnostics::codes;
use crate::iface::{Entity, Namespace};
use crate::source::Span;
use crate::syntax::cst::Node;
use crate::syntax::kind::SyntaxKind as K;

impl Checker<'_> {
    /// Checks the methods of `instance`; the methods it takes from the
    /// class's defaults, with how many dictionaries of their own each takes
    /// and those its default signature needs.
    #[allow(clippy::too_many_lines)]
    pub(super) fn instance_methods(
        &mut self,
        decl: &Node,
        instance: &Instance,
    ) -> Vec<(String, usize, Vec<Dict>)> {
        let Some(((cm, cn), args)) = split(&instance.head) else {
            return Vec::new();
        };
        let (cm, cn) = (cm.to_owned(), cn.to_owned());
        let Some(info) = self.class_info(&cm, &cn) else {
            return Vec::new();
        };
        let mark = self.tyvars.len();
        let skolems: Vec<(String, Type)> = instance
            .vars
            .iter()
            .map(|v| (v.name.clone(), self.store.skolem(&v.name)))
            .collect();
        for (v, (name, ty)) in instance.vars.iter().zip(&skolems) {
            self.tyvars.push(TyVar {
                name: name.clone(),
                ty: ty.clone(),
                kind: v.kind.clone(),
            });
        }
        let args: Vec<Type> = args.iter().map(|a| a.substitute(&skolems)).collect();
        let assumed = self.classes.givens.len();
        let context = instance.context.iter().map(|c| c.substitute(&skolems));
        self.classes.givens.extend(context);
        let members: Vec<&Node> = decl
            .nodes()
            .filter(|n| n.kind == K::Members)
            .flat_map(Node::nodes)
            .collect();
        let mut defined = Vec::new();
        for (j, member) in members.iter().enumerate() {
            match member.kind {
                K::LetDecl => {
                    let parts = let_parts(member, self.text);
                    let name = parts.name.clone().unwrap_or_default();
                    if !info.methods.contains(&name) {
                        self.error(
                            codes::C011,
                            self.span(member),
                            format!("`{name}` is not a method of class `{cn}`"),
                        );
                        continue;
                    }
                    defined.push(name.clone());
                    let signature = j
                        .checked_sub(1)
                        .map(|k| members[k])
                        .filter(|s| s.kind == K::Signature && self.pairs(s, member));
                    let sig = signature.and_then(|s| s.nodes().find(|n| is_type(n.kind)));
                    let expected = self.method_type(&cm, &name, &info.params, &args);
                    // A class fixes this type even when the instance omits its
                    // optional signature. Keep it at the definition for IDE hover.
                    if sig.is_none()
                        && let (Some(expected), Some(binder)) = (&expected, parts.pattern)
                    {
                        self.note_type(self.span(binder), expected);
                    }
                    match (sig, expected) {
                        (Some(sig), expected) => {
                            let (scheme, _) = self.signature(sig);
                            if let Some(expected) = expected {
                                self.compare_method(&name, &scheme, &expected, self.span(sig));
                                if let Some(signature) = signature {
                                    let (scheme, expected) = (&scheme, &expected);
                                    self.redundant_method_signature(
                                        signature, member, scheme, expected,
                                    );
                                }
                            }
                            self.check_signed(member, &parts, &scheme, None, None);
                        }
                        // The class signature already fixes the method's type.
                        (None, Some(expected)) => {
                            self.check_signed(member, &parts, &expected, None, None);
                        }
                        (None, None) => {
                            self.unsigned(member, &parts, &name);
                        }
                    }
                    self.settle_operators();
                }
                K::Signature if !members.get(j + 1).is_some_and(|d| self.pairs(member, d)) => {
                    self.lone_signature(member, false);
                }
                _ => {}
            }
        }
        let missing: Vec<String> = info
            .methods
            .iter()
            .filter(|m| !defined.contains(m) && !info.defaults.contains(m))
            .map(|m| format!("`{m}`"))
            .collect();
        // a type that derives the class takes every default: at its `deriving`
        let written = decl.kind == K::InstanceDecl;
        let head = (decl.nodes())
            .find(|n| {
                if written {
                    is_type(n.kind)
                } else {
                    n.kind == K::DerivingClause
                }
            })
            .map_or_else(|| self.span(decl), |h| self.span(h));
        let defaulted: Vec<String> = (info.defaults.iter())
            .filter(|m| !defined.contains(m))
            .cloned()
            .collect();
        let defaulted = defaulted
            .into_iter()
            .map(|m| {
                let own = self.own_dictionaries(&cm, &m);
                let extra = self.default_dictionaries(instance, &info, &m, (head, written));
                (m, own, extra)
            })
            .collect();
        if !missing.is_empty() {
            self.error(
                codes::C011,
                head,
                format!("`{}` lacks method {}", instance.name, missing.join(", ")),
            );
        }
        self.solve_classes(true);
        self.classes.givens.truncate(assumed);
        self.tyvars.truncate(mark);
        defaulted
    }

    /// The dictionaries the default signature of `method` needs beyond the
    /// class's, for `instance`: its constraints with the instance's types for
    /// the class's parameters, solved under the instance's context; none
    /// without a default signature. `written` tells an `instance` from a
    /// `deriving` at `at`.
    pub(super) fn default_dictionaries(
        &mut self,
        instance: &Instance,
        info: &ClassInfo,
        method: &str,
        (at, written): (Span, bool),
    ) -> Vec<Dict> {
        let Some((
            _,
            Type::Forall {
                vars, constraints, ..
            },
        )) = info.default_sigs.iter().find(|(m, _)| m == method)
        else {
            return Vec::new();
        };
        let Some((_, args)) = split(&instance.head) else {
            return Vec::new();
        };
        let mut subst: Vec<(String, Type)> = info
            .params
            .iter()
            .cloned()
            .zip(args.into_iter().cloned())
            .collect();
        for v in vars.iter().skip(info.params.len()) {
            let fresh = self.fresh();
            subst.push((v.name.clone(), fresh));
        }
        let mut dicts = Vec::new();
        for c in constraints.iter().skip(1).filter(|c| passes_dictionary(c)) {
            let wanted = c.substitute(&subst);
            if let Outcome::Solved(dict) = self.entails(&wanted, &instance.context) {
                dicts.push(dict);
                continue;
            }
            let shown = self.show(&self.canon(&wanted));
            let name = &instance.name;
            if written {
                let message = format!(
                    "`{name}` takes `{method}` from its class's default, which needs `{shown}`"
                );
                self.error(codes::C001, at, message);
            } else {
                let message =
                    format!("cannot derive `{name}`: the default of `{method}` needs `{shown}`");
                self.error(codes::C013, at, message);
            }
            dicts.push(Dict::Error);
        }
        dicts
    }

    /// How many dictionaries a class method takes besides its class's.
    fn own_dictionaries(&self, class_module: &str, method: &str) -> usize {
        let entity = Entity {
            module: class_module.to_owned(),
            namespace: Namespace::Value,
            name: method.to_owned(),
        };
        match self.env.value(&entity) {
            Some(Type::Forall { constraints, .. }) => constraints
                .iter()
                .skip(1)
                .filter(|c| passes_dictionary(c))
                .count(),
            _ => 0,
        }
    }

    /// The default definitions in the bodies of the module's classes: each is
    /// checked against its method's signature, or its default signature, the
    /// class constraint given.
    pub(super) fn class_defaults(&mut self, decls: &[&Node]) {
        let module = self.env.module.clone();
        for decl in decls.iter().filter(|d| d.kind == K::ClassDecl) {
            let Some(class) = self.con_token(decl) else {
                continue;
            };
            let info = self.class_info(&module, &class);
            let methods = info.as_ref().map(|i| i.methods.clone());
            let signed = info.map(|i| i.default_sigs).unwrap_or_default();
            let defaults = super::class_members::class_defaults(decl, self.text);
            for (name, _) in signed
                .iter()
                .filter(|(m, _)| defaults.iter().all(|(d, _)| d != m))
            {
                let at = default_signature(decl, self.text, name);
                self.error(
                    codes::C011,
                    at.map_or_else(|| self.span(decl), |s| self.span(s)),
                    format!("`{name}` has a default signature but no default: define `let {name} = …` in the class"),
                );
            }
            for (name, member) in defaults {
                if !methods.as_ref().is_some_and(|m| m.contains(&name)) {
                    self.error(
                        codes::C011,
                        self.span(member),
                        format!("`{name}` is not a method of class `{class}`: a default needs its signature in the class"),
                    );
                    continue;
                }
                let entity = Entity {
                    module: module.clone(),
                    namespace: Namespace::Value,
                    name: name.clone(),
                };
                let own = signed
                    .iter()
                    .find(|(m, _)| *m == name)
                    .map(|(_, t)| t.clone());
                let Some(scheme) = own.or_else(|| self.env.value(&entity).cloned()) else {
                    continue;
                };
                let parts = let_parts(member, self.text);
                self.check_signed(member, &parts, &scheme, None, None);
                self.settle_operators();
            }
        }
    }

    /// The class signature of `method` with `args` for the class parameters,
    /// without the class constraint itself.
    pub(super) fn method_type(
        &self,
        class_module: &str,
        method: &str,
        params: &[String],
        args: &[Type],
    ) -> Option<Type> {
        let entity = Entity {
            module: class_module.to_owned(),
            namespace: Namespace::Value,
            name: method.to_owned(),
        };
        let Type::Forall {
            vars,
            constraints,
            body,
        } = self.env.value(&entity)?
        else {
            return None;
        };
        let subst: Vec<(String, Type)> = params.iter().cloned().zip(args.iter().cloned()).collect();
        let own = vars.get(params.len()..).unwrap_or_default().to_vec();
        let constraints = constraints.get(1..).unwrap_or_default().to_vec();
        let ty = if own.is_empty() && constraints.is_empty() {
            (**body).clone()
        } else {
            Type::Forall {
                vars: own,
                constraints,
                body: body.clone(),
            }
        };
        Some(ty.substitute(&subst))
    }

    /// The signature of an instance method must be the class signature: as
    /// polymorphic, and with no constraint the class signature does not imply.
    fn compare_method(&mut self, name: &str, scheme: &Type, expected: &Type, span: Span) {
        self.store.level += 1;
        let mark = self.classes.wanted.len();
        let (body, _, assumed) = self.skolemize(expected);
        let own = self.instantiate(scheme, span);
        let same = self.unify(&own, &body).is_ok();
        let required: Vec<Wanted> = self.classes.wanted.drain(mark..).collect();
        self.store.level -= 1;
        if !same {
            let shown = self.show(expected);
            self.error(
                codes::C010,
                span,
                format!(
                    "method `{name}` must have the class signature for this instance: `{shown}`"
                ),
            );
            return;
        }
        let mut givens = self.classes.givens.clone();
        givens.extend(assumed);
        let written = match scheme {
            Type::Forall { constraints, .. } => constraints.clone(),
            _ => Vec::new(),
        };
        for (i, w) in required.iter().enumerate() {
            if !matches!(
                self.entails(&w.constraint, &givens),
                super::classes::Outcome::Solved(_)
            ) {
                let shown = self.show(written.get(i).unwrap_or(&w.constraint));
                self.error(
                    codes::C010,
                    span,
                    format!(
                        "method `{name}` requires `{shown}`, which the class signature does not"
                    ),
                );
            }
        }
    }
}
