//! The types and kinds of a module's exports, as `.iface` stores them.

use std::collections::HashMap;

use super::Checker;
use super::env::{Abi, Form, TypeDef};
use super::regions;
use super::services::Operation;
use super::ty::Type;
use crate::iface::{self, Decl, Entity, Export, Interface, Namespace};

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
        let makers = tagged(&self.typing.makers, &exported, |_| Vec::new());
        let servers = tagged(&self.typing.servers, &exported, Vec::clone);
        let tags = [(iface::MAKER, makers), (iface::SERVER, servers)];
        interface.tags = (tags.into_iter())
            .filter(|(_, marked)| !marked.is_empty())
            .map(|(tag, marked)| (tag.to_owned(), marked))
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
                        self.exported_def((&entity.module, &entity.name), d, Some(export.decl))
                    });
                }
            }
        }
        let cells = self.exported_cells(interface);
        if !cells.is_empty() {
            interface.tags.insert(iface::CELL.to_owned(), cells);
        }
        // the labelled types an importer may name, each with its kind
        let exported = |name: &str| {
            (interface.exports.iter()).any(|e| {
                e.entity.module == self.env.module
                    && e.entity.namespace == Namespace::Type
                    && e.entity.name == name
            })
        };
        let labels: Vec<(String, Vec<String>)> = (self.own_labels())
            .filter(|(name, _)| exported(name))
            .map(|(name, kind)| (name.clone(), vec![kind.clone()]))
            .collect();
        if !labels.is_empty() {
            interface.tags.insert(iface::LABEL.to_owned(), labels);
        }
        let owners: Vec<(String, Vec<String>)> = (self.typing.owner_services.iter())
            .filter(|name| exported(name))
            .map(|name| (name.clone(), Vec::new()))
            .collect();
        if !owners.is_empty() {
            interface.tags.insert(iface::OWNER.to_owned(), owners);
        }
    }

    /// The types marked `@cell` an importer learns of: those the module
    /// exports, and those of other modules that its exports mention, each
    /// with its module, as the importer may not import that one. A built-in
    /// module is known to all.
    fn exported_cells(&self, interface: &Interface) -> Vec<(String, Vec<String>)> {
        let own = &self.env.module;
        let mut cells = Vec::new();
        let mut mentioned = |t: &Type| {
            if let Type::Con { module, name } = t
                && module != own
                && !self.env.is_builtin(module)
                && self.cell(module, name)
            {
                cells.push((name.clone(), vec![module.clone()]));
            }
        };
        for export in interface.exports.iter().filter(|e| e.entity.module == *own) {
            if let Some(scheme) = &export.scheme {
                scheme.visit(&mut mentioned);
            }
            if let Some(Form::Alias { body, .. }) = export.definition.as_ref().map(|d| &d.form) {
                body.visit(&mut mentioned);
            }
        }
        let exported = |name: &&String| {
            let entity =
                |e: &Export| e.entity.namespace == Namespace::Type && e.entity.name == **name;
            interface
                .exports
                .iter()
                .any(|e| e.entity.module == *own && entity(e))
        };
        cells.extend(
            (self.typing.cells.iter())
                .filter(exported)
                .map(|name| (name.clone(), Vec::new())),
        );
        cells.sort();
        cells.dedup();
        cells
    }

    /// A type definition as importers see it: an opaque alias, and an ADT
    /// exported opaque, are nominal. `export: None` is the module's own view.
    fn exported_def(
        &self,
        (module, name): (&str, &str),
        d: &TypeDef,
        export: Option<Decl>,
    ) -> TypeDef {
        let nominal = match (&d.form, export) {
            (Form::Alias { opaque, .. }, Some(_)) => *opaque,
            (Form::Adt { .. }, Some(decl)) => decl == Decl::Opaque,
            _ => false,
        };
        // an importer cannot see the fields, so it is told the type hides
        let hidden = nominal
            && matches!(d.form, Form::Adt { .. })
            && self.stores_hidden(&Type::Con {
                module: module.to_owned(),
                name: name.to_owned(),
            });
        let mut d = d.clone();
        d.form = match d.form {
            // an opaque enum keeps what its values are: its literals
            Form::Adt {
                abi: Abi::Literal(literals),
                ..
            } if nominal => Form::Nominal { literals, hidden },
            _ if nominal => Form::Nominal {
                literals: Vec::new(),
                hidden,
            },
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
                Form::ModuleContract { .. } | Form::ModuleSignature { .. } => Decl::ModuleType,
                Form::Service { .. } => Decl::Service,
                Form::Class { .. } => Decl::Class,
                Form::Kind => Decl::Kind,
                Form::KindMember => Decl::KindMember,
            },
            parent: None,
            scheme: None,
            definition: Some(self.exported_def((&self.env.module, name), d, None)),
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
        let t = if regions::has_live(&t) {
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

/// The values among the exported ones that `marked` holds, each with the
/// path `path` gives it, for the tags of the interface.
fn tagged<P>(
    marked: &HashMap<String, P>,
    exported: &[&str],
    path: impl Fn(&P) -> Vec<String>,
) -> Vec<(String, Vec<String>)> {
    let mut found: Vec<(String, Vec<String>)> = exported
        .iter()
        .filter_map(|n| Some(((*n).to_owned(), path(marked.get(*n)?))))
        .collect();
    found.sort();
    found
}
