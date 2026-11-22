//! The interface a module's `exposing` list describes: its own definitions
//! and the names it re-exports, each with the members that travel with it.

use std::collections::BTreeSet;

use super::{Found, Scope};
use crate::diagnostics::{Diagnostic, codes};
use crate::resolve::defs::Def;
use crate::resolve::syntax::{Item, ItemKind};
use crate::resolve::{Decl, Entity, Export, Hidden, Interface, Namespace};

impl Scope<'_> {
    /// The interface the module's `exposing` list describes.
    pub fn exports(&self, diagnostics: &mut Vec<Diagnostic>) -> Interface {
        let mut exports: Vec<Export> = Vec::new();
        let declared = |name: &str| self.defs.iter().any(|d| d.name == name);
        for item in self.module.syntax.exported(declared) {
            match item.kind {
                ItemKind::All => {
                    let opaque = |parent: &Option<String>| {
                        self.defs
                            .iter()
                            .any(|d| Some(&d.name) == parent.as_ref() && d.decl == Decl::Opaque)
                    };
                    let visible = self.defs.iter().filter(|d| !opaque(&d.parent));
                    exports.extend(visible.map(|d| self.local(d)));
                }
                ItemKind::Value => {
                    self.export(Namespace::Value, item, false, &mut exports, diagnostics);
                }
                ItemKind::Con | ItemKind::ConAll => {
                    let namespace = match self.find(Namespace::Type, &item.name) {
                        Found::Missing => Namespace::Constructor,
                        _ => Namespace::Type,
                    };
                    let members = item.kind == ItemKind::ConAll;
                    self.export(namespace, item, members, &mut exports, diagnostics);
                }
            }
        }
        let mut seen = BTreeSet::new();
        exports.retain(|e| seen.insert(e.entity.clone()));
        let hidden = self
            .defs
            .iter()
            .filter(|d| d.namespace == Namespace::Constructor)
            .filter_map(|d| {
                Some(Hidden {
                    name: d.name.clone(),
                    parent: d.parent.clone()?,
                })
            })
            .filter(|h| {
                let exported = |namespace, name: &str| {
                    exports
                        .iter()
                        .any(|e| e.entity.namespace == namespace && e.entity.name == name)
                };
                exported(Namespace::Type, &h.parent) && !exported(Namespace::Constructor, &h.name)
            })
            .collect();
        Interface {
            module: self.module.name.to_owned(),
            exports,
            hidden,
            ..Interface::default()
        }
    }

    fn local(&self, def: &Def) -> Export {
        Export {
            entity: Entity {
                module: self.module.name.to_owned(),
                namespace: def.namespace,
                name: def.name.clone(),
            },
            decl: def.decl,
            parent: def.parent.clone(),
            scheme: None,
            definition: None,
            blocked: def.blocked.clone(),
            at: Some([def.span.start, def.span.end]),
            #[cfg(not(target_arch = "wasm32"))]
            via: Vec::new(),
        }
    }

    fn export(
        &self,
        namespace: Namespace,
        item: &Item,
        members: bool,
        exports: &mut Vec<Export>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let error = |message: String| Diagnostic::error(codes::R011, message, item.span);
        match self.find(namespace, &item.name) {
            Found::Local(def) => {
                exports.push(self.local(def));
                if !members {
                    return;
                }
                match def.decl {
                    decl if decl.has_members() => {
                        let children = self
                            .defs
                            .iter()
                            .filter(|d| d.parent.as_ref() == Some(&def.name));
                        exports.extend(children.map(|d| self.local(d)));
                    }
                    Decl::Opaque => diagnostics.push(
                        error(format!(
                            "opaque type `{}` cannot export its constructors",
                            item.name
                        ))
                        .with_fix(|| {
                            (
                                format!("export the type alone: `{}`", item.name),
                                Vec::new(),
                            )
                        }),
                    ),
                    _ => diagnostics.push(error(format!(
                        "`{}` has no constructors to export",
                        item.name
                    ))),
                }
            }
            Found::Imported(export, interface) => {
                let via = interface.map(|i| i.module.as_str());
                exports.push(reexported(export, via));
                // a re-exported type keeps its companions, `T$parse`, `T$check`
                // and the rest, so `M.T.parse` works through the re-exporter
                if namespace == Namespace::Type {
                    let prefix = format!("{}$", export.entity.name);
                    exports.extend(
                        (interface.iter().flat_map(|i| &i.exports))
                            .filter(|e| {
                                e.entity.module == export.entity.module
                                    && e.entity.namespace == Namespace::Value
                                    && e.entity.name.starts_with(&prefix)
                            })
                            .map(|e| reexported(e, via)),
                    );
                }
                if members {
                    exports.extend(
                        interface
                            .into_iter()
                            .flat_map(|i| i.members(&item.name))
                            .map(|e| reexported(e, via)),
                    );
                }
            }
            Found::Unknown => {}
            Found::Missing
                if self
                    .module
                    .syntax
                    .applications
                    .iter()
                    .any(|application| application.name == item.name) => {}
            Found::Missing => diagnostics.push(error(format!(
                "cannot export `{}`: it is neither defined in this module nor imported",
                item.name
            ))),
        }
    }
}

/// Clones `export` for re-export, recording the immediate source module `via`
/// when it differs from `export.entity.module` (so multi-hop re-export facades
/// stay visible to environment preflight).
fn reexported(export: &Export, via: Option<&str>) -> Export {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut out = export.clone();
        if let Some(module) = via
            && module != out.entity.module
            && !out.via.iter().any(|existing| existing == module)
        {
            out.via.push(module.to_owned());
        }
        out
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = via;
        export.clone()
    }
}
