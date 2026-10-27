//! The types and kinds of a module's exports, as `.iface` stores them.

use super::Checker;
use super::env::{Form, Operation};
use super::ty::Type;
use crate::iface::{Decl, Export, Interface, Namespace};

impl Checker<'_> {
    /// Stores the types and kinds of this module's exports.
    pub fn fill(&self, interface: &mut Interface) {
        // built-in instances are known to every module without an interface
        interface.instances = self
            .env
            .instances
            .iter()
            .filter(|i| !self.env.builtin.contains(&i.module))
            .cloned()
            .collect();
        self.export_companions(interface);
        self.export_server_fns(interface);
        let exported: Vec<&str> = interface
            .exports
            .iter()
            .filter(|e| {
                e.entity.module == self.env.module && e.entity.namespace == Namespace::Value
            })
            .map(|e| e.entity.name.as_str())
            .collect();
        interface.makers = super::makers::exported_makers(&self.typing.makers, &exported);
        interface.servers = super::server::exported_servers(&self.typing.servers, &exported);
        for export in &mut interface.exports {
            let entity = &export.entity;
            if entity.module != self.env.module {
                continue;
            }
            match entity.namespace {
                Namespace::Value | Namespace::Constructor => {
                    export.scheme = self.env.value(entity).map(|t| self.exported(t));
                }
                Namespace::Type => {
                    export.definition = self.env.type_def(&entity.module, &entity.name).map(|d| {
                        let mut d = d.clone();
                        d.form = match d.form {
                            Form::Alias { opaque: true, .. } => Form::Nominal,
                            Form::Adt { .. } if export.decl == Decl::Opaque => Form::Nominal,
                            Form::Alias { params, body, .. } => Form::Alias {
                                params,
                                body: self.exported(&body),
                                opaque: false,
                            },
                            Form::Service { params, operations } => Form::Service {
                                params,
                                operations: operations
                                    .into_iter()
                                    .map(|o| Operation {
                                        ty: self.exported(&o.ty),
                                        name: o.name,
                                    })
                                    .collect(),
                            },
                            form => form,
                        };
                        d
                    });
                }
            }
        }
    }

    /// The implementation `f$server` of each exported server function.
    fn export_server_fns(&self, interface: &mut Interface) {
        let values: Vec<String> = interface
            .exports
            .iter()
            .filter(|e| {
                e.entity.namespace == Namespace::Value && e.entity.module == self.env.module
            })
            .map(|e| e.entity.name.clone())
            .collect();
        for value in values {
            let name = super::companion_name(&value, super::server_fn::IMPLEMENTATION);
            let entity = self.env.entity(Namespace::Value, &name);
            if self.env.value(&entity).is_some() {
                interface.exports.push(Export {
                    entity,
                    decl: Decl::Let,
                    parent: None,
                    scheme: None,
                    definition: None,
                    blocked: None,
                    at: None,
                });
            }
        }
    }

    /// The companions of each exported type of this module, `T$schema`,
    /// `T$parse`, `T$value`, `T$json`, `T$table`, as values beside it.
    fn export_companions(&self, interface: &mut Interface) {
        let types: Vec<String> = interface
            .exports
            .iter()
            .filter(|e| e.entity.namespace == Namespace::Type && e.entity.module == self.env.module)
            .map(|e| e.entity.name.clone())
            .collect();
        for ty in types {
            for member in ["schema", "parse", "value", "json", "table"] {
                let entity = self
                    .env
                    .entity(Namespace::Value, &super::companion_name(&ty, member));
                if self.env.value(&entity).is_some()
                    && !interface.exports.iter().any(|e| e.entity == entity)
                {
                    interface.exports.push(Export {
                        entity,
                        decl: Decl::Let,
                        parent: None,
                        scheme: None,
                        definition: None,
                        blocked: None,
                        at: None,
                    });
                }
            }
        }
    }

    /// A type as importers see it: transparent aliases expanded, so they
    /// need no alias of a module they do not import.
    fn exported(&self, t: &Type) -> Type {
        let t = self.zonk(t);
        let (head, args) = t.spine();
        if let Type::Con { module, name } = head
            && let Some(Form::Alias {
                params,
                body,
                opaque: false,
            }) = self.env.type_def(module, name).map(|d| &d.form)
            && args.len() >= params.len()
        {
            let subst: Vec<(String, Type)> = params
                .iter()
                .cloned()
                .zip(args.iter().map(|a| (*a).clone()))
                .collect();
            let rest = args[params.len()..].iter().map(|a| (*a).clone());
            return self.exported(&Type::apps(body.substitute(&subst), rest));
        }
        t.map_children(&mut |c| self.exported(c))
    }
}
