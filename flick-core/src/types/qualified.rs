//! What a top-level or qualified name refers to: `M.name` of a module, an
//! operation of a service, a companion of a type.

use super::Checker;
use super::gadt::scheme::dicts;
use super::ty::Type;
use super::typing::{Constructor, Ref, range};
use crate::diagnostics::codes;
use crate::diagnostics::suggest::closest;
use crate::iface::{Entity, Namespace};
use crate::source::Span;
use crate::types::lang::{self, Lang};

impl Checker<'_> {
    /// Records what a top-level name refers to; `ty` is its type at the use.
    pub(super) fn record_entity(&mut self, entity: &Entity, ty: &Type, span: Span) {
        let parent = self.env.parent(entity);
        let r = match (entity.namespace, parent) {
            (Namespace::Constructor, Some((module, ty_name))) => {
                let arity = self.arrow_stages(ty).len();
                let rep = self.env.constructor_rep(&module, &ty_name, &entity.name);
                let dicts = self.env.value(entity).map_or(0, dicts);
                Ref::Constructor(Constructor {
                    module,
                    ty: ty_name,
                    name: entity.name.clone(),
                    arity,
                    rep,
                    dicts,
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
        if lang::is(Lang::RecordLabel, &entity.module, &entity.name) {
            self.record_label(span, ty.clone());
        }
        if lang::is(Lang::WireCodec, &entity.module, &entity.name) {
            self.typing.pending_wire.push((range(span), span));
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
        // generated code names a companion by its type, a module of the
        // type's name notwithstanding: `Path.schema` of a type `Path`
        if span.start >= self.generated
            && let Some(ty) = self.companion_of(qualifier, name, span)
        {
            return ty;
        }
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
            if entity.is_none() && interface.is_some() && !self.env.is_import(qualifier) {
                // `Order.schema`: a companion of a type named as a module
                if let Some(ty) = self.companion_of(qualifier, name, span) {
                    return ty;
                }
                if let Some(value) = name.strip_suffix("$server") {
                    self.not_a_server_fn(&format!("{qualifier}.{value}"), span);
                    return Type::Error;
                }
                self.error_guess(
                    codes::T001,
                    span,
                    format!("`{qualifier}` does not export `{name}`"),
                    |this| {
                        let (_, Some(interface)) = this.env.qualified_module(qualifier)? else {
                            return None;
                        };
                        let exports = interface
                            .exports
                            .iter()
                            .filter(|e| e.entity.namespace == namespace);
                        qualified_guess(qualifier, name, exports.map(|e| e.entity.name.as_str()))
                    },
                );
                return Type::Error;
            }
            let scheme = entity.as_ref().and_then(|e| self.env.value(e).cloned());
            let ty = scheme.map_or(Type::Error, |s| {
                self.instantiate(&self.live_scheme(&s), span)
            });
            if let Some(entity) = entity {
                self.record_entity(&entity, &ty, span);
            }
            return ty;
        }
        // `Email.parse`: a companion of a type
        if let Some(ty) = self.companion_of(qualifier, name, span) {
            return ty;
        }
        self.service_member(qualifier, name, span)
    }

    /// `T.member`, the companion `T$member` of the type `T` in scope.
    fn companion_of(&mut self, qualifier: &str, name: &str, span: Span) -> Option<Type> {
        let super::env::Found::Entity(e) = self.env.find_type(qualifier) else {
            return None;
        };
        let entity = Entity {
            module: e.module.clone(),
            namespace: Namespace::Value,
            name: super::companion_name(&e.name, name),
        };
        let scheme = self.env.value(&entity).cloned()?;
        let ty = self.instantiate(&self.live_scheme(&scheme), span);
        self.record_entity(&entity, &ty, span);
        Some(ty)
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
            Some((
                module,
                service,
                super::env::Form::Service {
                    params, operations, ..
                },
            )) => {
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
                    self.error_guess(
                        codes::T001,
                        span,
                        format!("service `{qualifier}` has no operation `{name}`"),
                        |_| {
                            qualified_guess(
                                qualifier,
                                name,
                                operations.iter().map(|o| o.name.as_str()),
                            )
                        },
                    );
                    Type::Error
                }
            }
            // the witness a plugin's code names for a type without one
            Some(_) if name == "schema" => {
                self.error(
                    codes::D001,
                    span,
                    format!(
                        "`{qualifier}` derives no Schema: add `deriving Schema` to its declaration"
                    ),
                );
                Type::Error
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
                self.error_guess(
                    codes::T001,
                    span,
                    format!("unknown module `{qualifier}`"),
                    |this| {
                        closest(qualifier, this.env.qualifier_names())
                            .map(|g| format!("{g}.{name}"))
                    },
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
