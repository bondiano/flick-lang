//! What a top-level or qualified name refers to: `M.name` of a module, an
//! operation of a service, a companion of a type.

use super::Checker;
use super::ty::Type;
use super::typing::{Constructor, Ref};
use crate::diagnostics::codes;
use crate::diagnostics::suggest::closest;
use crate::iface::{Entity, Namespace};
use crate::source::Span;

impl Checker<'_> {
    /// Records what a top-level name refers to; `ty` is its type at the use.
    pub(super) fn record_entity(&mut self, entity: &Entity, ty: &Type, span: Span) {
        let parent = self.env.parent(entity);
        let r = match (entity.namespace, parent) {
            (Namespace::Constructor, Some((module, ty_name))) => {
                let arity = self.arrow_stages(ty).len();
                let rep = self.env.constructor_rep(&module, &ty_name, &entity.name);
                Ref::Constructor(Constructor {
                    module,
                    ty: ty_name,
                    name: entity.name.clone(),
                    arity,
                    rep,
                })
            }
            (Namespace::Value, Some(_)) => Ref::Method {
                module: entity.module.clone(),
                name: entity.name.clone(),
            },
            _ => Ref::Value {
                module: entity.module.clone(),
                name: entity.name.clone(),
            },
        };
        if entity.module == super::classes::RECORD && entity.name == "label" {
            self.record_label(span, ty.clone());
        }
        self.record_name(span, r);
    }

    /// `Module.name`, `Module.Con`, or `Service.operation`.
    pub(super) fn qualified(
        &mut self,
        qualifier: &str,
        namespace: Namespace,
        name: &str,
        span: Span,
    ) -> Type {
        // a service in scope with this operation wins over a module of its name
        let operation = match self.env.find_type(qualifier) {
            super::env::Found::Entity(e) => self.env.type_def(&e.module, &e.name).is_some_and(
                |d| matches!(&d.form, super::env::Form::Service { operations, .. } if operations.iter().any(|o| o.name == name)),
            ),
            _ => false,
        };
        // `M.S`: a service of the module named as a value, which has no constructor `S`
        let no_constructor = namespace == Namespace::Constructor
            && self
                .env
                .qualified_module(qualifier)
                .and_then(|(_, i)| i)
                .is_some_and(|i| i.find(namespace, name).is_none());
        if no_constructor {
            let path = format!("{qualifier}.{name}");
            if let super::env::Found::Entity(e) = self.env.find_type(&path)
                && let Some(tag) = self.service_tag(&path)
            {
                self.record_name(span, Ref::ServiceTag(format!("{}.{}", e.module, e.name)));
                return tag;
            }
            if let super::env::Found::Entity(_) = self.env.find_type(&path) {
                self.error(
                    codes::T001,
                    span,
                    format!("`{path}` is a type, not a constructor or a service"),
                );
                return Type::Error;
            }
        }
        if !operation && let Some((_, interface)) = self.env.qualified_module(qualifier) {
            // a name the module does not export is reported by name resolution
            let entity = interface
                .and_then(|i| i.find(namespace, name))
                .map(|e| e.entity.clone());
            if entity.is_none()
                && let Some(interface) = interface
                && !self.env.is_import(qualifier)
            {
                let exports = interface
                    .exports
                    .iter()
                    .filter(|e| e.entity.namespace == namespace);
                let guess =
                    qualified_guess(qualifier, name, exports.map(|e| e.entity.name.as_str()));
                self.error_guess(
                    codes::T001,
                    span,
                    format!("`{qualifier}` does not export `{name}`"),
                    guess.as_deref(),
                );
                return Type::Error;
            }
            let scheme = entity.as_ref().and_then(|e| self.env.value(e).cloned());
            let ty = scheme.map_or(Type::Error, |s| self.instantiate(&s, span));
            if let Some(entity) = entity {
                self.record_entity(&entity, &ty, span);
            }
            return ty;
        }
        // `Email.parse`: a companion of a type
        if let super::env::Found::Entity(e) = self.env.find_type(qualifier) {
            let entity = Entity {
                module: e.module.clone(),
                namespace: Namespace::Value,
                name: super::companion_name(&e.name, name),
            };
            if let Some(scheme) = self.env.value(&entity).cloned() {
                let ty = self.instantiate(&scheme, span);
                self.record_entity(&entity, &ty, span);
                return ty;
            }
        }
        self.service_member(qualifier, name, span)
    }

    /// `Service.operation`, or the error for a qualifier that names nothing.
    fn service_member(&mut self, qualifier: &str, name: &str, span: Span) -> Type {
        let service = match self.env.find_type(qualifier) {
            super::env::Found::Entity(e) => self
                .env
                .type_def(&e.module, &e.name)
                .cloned()
                .map(|d| (e.module, e.name, d)),
            super::env::Found::Unknown => return Type::Error,
            super::env::Found::Missing => None,
        };
        match service.map(|(module, service, d)| (module, service, d.form)) {
            Some((module, service, super::env::Form::Service { params, operations })) => {
                if let Some(op) = operations.iter().find(|o| o.name == name) {
                    let ty = self.operation_type(&module, &service, params.len(), &op.ty, span);
                    let stages = self.arrow_stages(&ty);
                    self.record_name(
                        span,
                        Ref::Operation {
                            service: format!("{module}.{service}"),
                            name: name.to_owned(),
                            stages,
                        },
                    );
                    ty
                } else {
                    let guess = qualified_guess(
                        qualifier,
                        name,
                        operations.iter().map(|o| o.name.as_str()),
                    );
                    self.error_guess(
                        codes::T001,
                        span,
                        format!("service `{qualifier}` has no operation `{name}`"),
                        guess.as_deref(),
                    );
                    Type::Error
                }
            }
            Some(_) => {
                self.error(
                    codes::T001,
                    span,
                    format!("`{qualifier}` is a type, not a module or a service"),
                );
                Type::Error
            }
            None => {
                let guess =
                    closest(qualifier, self.env.qualifier_names()).map(|g| format!("{g}.{name}"));
                self.error_guess(
                    codes::T001,
                    span,
                    format!("unknown module `{qualifier}`"),
                    guess.as_deref(),
                );
                Type::Error
            }
        }
    }
}

/// `qualifier.candidate` for the candidate closest to a misspelt `name`.
fn qualified_guess<'a>(
    qualifier: &str,
    name: &str,
    candidates: impl Iterator<Item = &'a str>,
) -> Option<String> {
    closest(name, candidates).map(|g| format!("{qualifier}.{g}"))
}
