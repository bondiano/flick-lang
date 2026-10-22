//! The types and kinds of a module's exports, as `.iface` stores them.

use super::Checker;
use super::env::{Form, Operation};
use super::ty::Type;
use crate::iface::{Decl, Interface, Namespace};

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
