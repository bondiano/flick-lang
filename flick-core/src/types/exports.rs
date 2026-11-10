//! The types and kinds of a module's exports, as `.iface` stores them.

use super::Checker;
use super::env::{Abi, Form, TypeDef};
use super::regions;
use super::services::Operation;
use super::ty::Type;
use crate::iface::{Decl, Entity, Export, Interface, Namespace};

impl Checker<'_> {
    /// Stores the types and kinds of this module's exports.
    pub fn fill(&self, interface: &mut Interface) {
        // built-in instances are known to every module without an interface
        interface.instances = self.env.own_instances().cloned().collect();
        self.export_companions(interface);
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
                    export.definition = self
                        .env
                        .type_def(&entity.module, &entity.name)
                        .map(|d| self.exported_def(d, Some(export.decl)));
                }
            }
        }
    }

    /// A type definition as importers see it: an opaque alias, and an ADT
    /// exported opaque, are nominal. `export: None` is the module's own view.
    fn exported_def(&self, d: &TypeDef, export: Option<Decl>) -> TypeDef {
        let nominal = match (&d.form, export) {
            (Form::Alias { opaque, .. }, Some(_)) => *opaque,
            (Form::Adt { .. }, Some(decl)) => decl == Decl::Opaque,
            _ => false,
        };
        let mut d = d.clone();
        d.form = match d.form {
            // an opaque enum keeps what its values are: its literals
            Form::Adt {
                abi: Abi::Literal(literals),
                ..
            } if nominal => Form::Nominal { literals },
            _ if nominal => Form::NOMINAL,
            Form::Alias { params, body, .. } => Form::Alias {
                params,
                body: self.exported(&body),
                opaque: false,
            },
            Form::Service {
                params,
                operations,
                effect,
            } => Form::Service {
                params,
                operations: operations
                    .into_iter()
                    .map(|o| Operation {
                        ty: self.exported(&o.ty),
                        name: o.name,
                        class: o.class,
                    })
                    .collect(),
                effect,
            },
            form => form,
        };
        d
    }

    /// Every value, constructor and type the module declares, exported or
    /// not, typed as [`Checker::fill`] types an export but with opaque
    /// types open: what the reference model (`flick-model`) checks the
    /// module against.
    pub fn declarations(&self) -> Interface {
        let value = |(entity, t): (&Entity, &Type)| Export {
            entity: entity.clone(),
            decl: if entity.namespace == Namespace::Constructor {
                Decl::Constructor
            } else {
                Decl::Let
            },
            parent: self.env.parent(entity).map(|(_, ty)| ty),
            scheme: Some(self.exported(t)),
            definition: None,
            blocked: None,
            at: None,
        };
        let ty = |(name, d): (&str, &TypeDef)| Export {
            entity: self.env.entity(Namespace::Type, name),
            decl: match d.form {
                Form::Nominal { .. } => Decl::Abstract,
                Form::Adt { .. } => Decl::Adt,
                Form::Error => Decl::Error,
                Form::Alias { .. } => Decl::Alias,
                Form::Service { .. } => Decl::Service,
                Form::Class { .. } => Decl::Class,
                Form::Kind => Decl::Kind,
                Form::KindMember => Decl::KindMember,
            },
            parent: None,
            scheme: None,
            definition: Some(self.exported_def(d, None)),
            blocked: None,
            at: None,
        };
        let mut exports: Vec<Export> = self.env.own_values().map(value).collect();
        exports.extend(self.env.own_types().map(ty));
        exports.sort_by(|a, b| a.entity.cmp(&b.entity));
        Interface {
            module: self.env.module.clone(),
            exports,
            instances: self.env.own_instances().cloned().collect(),
            ..Interface::default()
        }
    }

    /// The companions of each exported type of this module, `T$schema`,
    /// `T$parse`, `T$value`, `T$json`, `T$table`, `T$url`, and those a plugin
    /// declares beside an exported type or value, as `f$server` of a
    /// `server fn`.
    fn export_companions(&self, interface: &mut Interface) {
        let own = |namespace: Namespace| -> Vec<String> {
            (interface.exports.iter())
                .filter(|e| e.entity.namespace == namespace && e.entity.module == self.env.module)
                .map(|e| e.entity.name.clone())
                .collect()
        };
        let types = own(Namespace::Type);
        let values = own(Namespace::Value);
        // only generated code declares a top-level `T$member`
        let mut declared: Vec<&Entity> = (self.env.own_values())
            .map(|(e, _)| e)
            .filter(|e| {
                e.namespace == Namespace::Value
                    && self.top.declares(&e.name)
                    && (e.name.split_once('$')).is_some_and(|(owner, _)| {
                        types.contains(&owner.to_owned()) || values.contains(&owner.to_owned())
                    })
                    && !interface.exports.iter().any(|x| x.entity == **e)
            })
            .collect();
        declared.sort();
        for entity in declared {
            interface.exports.push(Export {
                entity: entity.clone(),
                decl: Decl::Let,
                parent: None,
                scheme: None,
                definition: None,
                blocked: None,
                at: None,
            });
        }
        for ty in types {
            for member in ["schema", "parse", "value", "all", "json", "table", "url"] {
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
        // a use elides `Live` again from the signature (`regions/live.rs`)
        let t = if self.live() && regions::has_live(&t) {
            regions::strip_live(&t)
        } else {
            t
        };
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
