//! Names in scope at the top level of a module: its own definitions and what
//! its imports expose. Checks imports, exports, shadowing and ambiguity; every
//! rule compares names, never import order, so reordering imports changes no
//! diagnostic.

use std::collections::{BTreeMap, BTreeSet};

use super::defs::Def;
use super::syntax::{Import, Item, ItemKind};
use super::uses::Reference;
use super::{Decl, Entity, Export, Hidden, Interface, Module, Namespace};
use crate::diagnostics::{Diagnostic, codes};
use crate::source::Span;

/// An imported name: its export, `None` for an unloaded module, the import
/// and the span of the `exposing` item.
type Binding<'a> = (Option<Export>, &'a Import, Span);

pub(super) struct Scope<'a> {
    module: &'a Module<'a>,
    defs: &'a [Def],
    imports: Vec<(&'a Import, Option<Interface>)>,
    /// Names imported with `exposing (name)`; `None` for an unloaded module.
    explicit: BTreeMap<(Namespace, String), Vec<Binding<'a>>>,
}

enum Found<'s> {
    Local(&'s Def),
    Imported(&'s Export, Option<&'s Interface>),
    /// Possibly from a module that could not be loaded.
    Unknown,
    Missing,
}

impl<'a> Scope<'a> {
    pub fn new(
        module: &'a Module<'a>,
        defs: &'a [Def],
        imported: &dyn Fn(&str) -> Option<Interface>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Self {
        let imports: Vec<_> = module
            .syntax
            .imports
            .iter()
            .map(|import| (import, imported(&import.module)))
            .collect();
        let mut explicit: BTreeMap<_, Vec<_>> = BTreeMap::new();
        for (import, interface) in &imports {
            for item in &import.exposing {
                for (namespace, export) in expose(import, interface.as_ref(), item, diagnostics) {
                    let name = export
                        .as_ref()
                        .map_or(item.name.clone(), |e| e.entity.name.clone());
                    explicit
                        .entry((namespace, name))
                        .or_default()
                        .push((export, *import, item.span));
                }
            }
        }
        let scope = Scope {
            module,
            defs,
            imports,
            explicit,
        };
        scope.check_qualifiers(diagnostics);
        scope.check_conflicts(diagnostics);
        scope.check_shadowing(diagnostics);
        scope
    }

    /// Two imports under one qualifier, e.g. `import A as M` and `import M`.
    fn check_qualifiers(&self, diagnostics: &mut Vec<Diagnostic>) {
        let mut by_qualifier: BTreeMap<&str, Vec<&Import>> = BTreeMap::new();
        for (import, _) in &self.imports {
            by_qualifier
                .entry(import.qualifier())
                .or_default()
                .push(import);
        }
        for (qualifier, mut imports) in by_qualifier {
            imports.sort_by(|a, b| (&a.module, a.span.start).cmp(&(&b.module, b.span.start)));
            imports.dedup_by(|a, b| a.module == b.module);
            if let [first, .., last] = imports[..] {
                diagnostics.push(
                    Diagnostic::error(
                        codes::R010,
                        format!(
                            "qualifier `{qualifier}` is ambiguous: it names both `{}` and `{}`",
                            first.module, last.module
                        ),
                        last.span,
                    )
                    .with_related(first.span, "also imported here")
                    .with_fix("give one of the imports another name with `as`", Vec::new()),
                );
            }
        }
    }

    /// One name exposed by two imports as different entities.
    fn check_conflicts(&self, diagnostics: &mut Vec<Diagnostic>) {
        for ((_, name), bindings) in &self.explicit {
            let mut known: Vec<_> = bindings
                .iter()
                .filter_map(|(e, i, s)| Some((e.as_ref()?, *i, *s)))
                .collect();
            if known.len() < bindings.len() {
                continue;
            }
            known.sort_by(|a, b| (&a.1.module, a.2.start).cmp(&(&b.1.module, b.2.start)));
            known.dedup_by(|a, b| a.0.entity == b.0.entity);
            if let [(_, first, first_span), .., (_, last, last_span)] = known[..] {
                diagnostics.push(
                    Diagnostic::error(
                        codes::R010,
                        format!(
                            "`{name}` is ambiguous: it is imported from both `{}` and `{}`",
                            first.module, last.module
                        ),
                        last_span,
                    )
                    .with_related(first_span, "also imported here")
                    .with_fix(
                        format!("import `{name}` from one module and qualify the other"),
                        Vec::new(),
                    ),
                );
            }
        }
    }

    /// A top-level definition with the name of an explicitly imported one.
    fn check_shadowing(&self, diagnostics: &mut Vec<Diagnostic>) {
        let mut seen = BTreeSet::new();
        for def in self.defs {
            let key = (def.namespace, def.name.clone());
            let Some(bindings) = self.explicit.get(&key) else {
                continue;
            };
            if !seen.insert(key) {
                continue;
            }
            let (_, import, span) = bindings
                .iter()
                .min_by(|a, b| (&a.1.module, a.2.start).cmp(&(&b.1.module, b.2.start)))
                .expect("a binding exists");
            diagnostics.push(
                Diagnostic::error(
                    codes::R009,
                    format!(
                        "`{}` shadows the name imported from `{}`",
                        def.name, import.module
                    ),
                    def.span,
                )
                .with_related(*span, "imported here")
                .with_fix(
                    "rename the definition or drop the name from `exposing`",
                    Vec::new(),
                ),
            );
        }
    }

    fn find(&self, namespace: Namespace, name: &str) -> Found<'_> {
        if let Some(def) = self
            .defs
            .iter()
            .find(|d| d.namespace == namespace && d.name == name)
        {
            return Found::Local(def);
        }
        if let Some(bindings) = self.explicit.get(&(namespace, name.to_owned())) {
            let (export, import, _) = &bindings[0];
            return match export {
                Some(export) => Found::Imported(export, self.interface(&import.module)),
                None => Found::Unknown,
            };
        }
        let mut open = self.open(namespace, name);
        match (open.next(), open.next()) {
            (Some((export, interface)), None) => Found::Imported(export, Some(interface)),
            (None, None) if self.has_unloaded_open_import() => Found::Unknown,
            (None, None) => Found::Missing,
            // ambiguous: reported where it is used
            _ => Found::Unknown,
        }
    }

    fn interface(&self, module: &str) -> Option<&Interface> {
        self.imports
            .iter()
            .find(|(i, _)| i.module == module)
            .and_then(|(_, i)| i.as_ref())
    }

    /// Distinct entities named `name` that `exposing (..)` imports bring in.
    fn open(
        &self,
        namespace: Namespace,
        name: &str,
    ) -> impl Iterator<Item = (&Export, &Interface)> {
        let mut seen = BTreeSet::new();
        self.imports
            .iter()
            .filter(|(import, _)| import.exposing.iter().any(|i| i.kind == ItemKind::All))
            .filter_map(|(_, interface)| interface.as_ref())
            .filter_map(move |interface| Some((interface.find(namespace, name)?, interface)))
            .filter(move |(export, _)| seen.insert(export.entity.clone()))
    }

    fn has_unloaded_open_import(&self) -> bool {
        self.imports.iter().any(|(import, interface)| {
            interface.is_none() && import.exposing.iter().any(|i| i.kind == ItemKind::All)
        })
    }

    /// The interface the module's `exposing` list describes.
    pub fn exports(&self, diagnostics: &mut Vec<Diagnostic>) -> Interface {
        let mut exports: Vec<Export> = Vec::new();
        let items = self
            .module
            .syntax
            .header
            .as_ref()
            .and_then(|h| h.exposing.as_deref())
            .unwrap_or_default();
        for item in items {
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
                        .with_fix(
                            format!("export the type alone: `{}`", item.name),
                            Vec::new(),
                        ),
                    ),
                    _ => diagnostics.push(error(format!(
                        "`{}` has no constructors to export",
                        item.name
                    ))),
                }
            }
            Found::Imported(export, interface) => {
                exports.push(export.clone());
                if members {
                    exports.extend(
                        interface
                            .into_iter()
                            .flat_map(|i| i.members(&item.name))
                            .cloned(),
                    );
                }
            }
            Found::Unknown => {}
            Found::Missing => diagnostics.push(error(format!(
                "cannot export `{}`: it is neither defined in this module nor imported",
                item.name
            ))),
        }
    }

    pub fn check_use(&self, reference: &Reference, diagnostics: &mut Vec<Diagnostic>) {
        let Reference {
            namespace,
            qualifier,
            name,
            span,
        } = reference;
        if let Some(qualifier) = qualifier {
            let interface = self
                .imports
                .iter()
                .filter(|(import, _)| import.qualifier() == qualifier)
                .find_map(|(_, interface)| interface.as_ref());
            if let Some(interface) = interface
                && interface.find(*namespace, name).is_none()
            {
                diagnostics.push(not_exported(interface, name, *span));
            }
            return;
        }
        if self
            .defs
            .iter()
            .any(|d| d.namespace == *namespace && d.name == *name)
            || self.explicit.contains_key(&(*namespace, name.clone()))
        {
            return;
        }
        let mut modules: Vec<&str> = self
            .open(*namespace, name)
            .map(|(_, i)| i.module.as_str())
            .collect();
        if modules.len() > 1 {
            modules.sort_unstable();
            let list = modules
                .iter()
                .map(|m| format!("`{m}`"))
                .collect::<Vec<_>>()
                .join(", ");
            diagnostics.push(
                Diagnostic::error(
                    codes::R010,
                    format!("`{name}` is ambiguous: it is exposed by {list}"),
                    *span,
                )
                .with_fix(
                    format!(
                        "qualify it, e.g. `{}.{name}`, or import it by name",
                        modules[0]
                    ),
                    Vec::new(),
                ),
            );
        }
    }
}

/// Names one `exposing` item brings into scope, checked against the module's interface.
fn expose(
    import: &Import,
    interface: Option<&Interface>,
    item: &Item,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<(Namespace, Option<Export>)> {
    let Some(interface) = interface else {
        return match item.kind {
            ItemKind::All => Vec::new(),
            ItemKind::Value => vec![(Namespace::Value, None)],
            ItemKind::Con | ItemKind::ConAll => vec![(Namespace::Type, None)],
        };
    };
    let found = |namespace| {
        interface
            .find(namespace, &item.name)
            .map(|e| (namespace, Some(e.clone())))
    };
    let mut out = Vec::new();
    match item.kind {
        ItemKind::All => {}
        ItemKind::Value => out.extend(found(Namespace::Value)),
        ItemKind::Con => {
            out.extend(found(Namespace::Type).or_else(|| found(Namespace::Constructor)));
        }
        ItemKind::ConAll => {
            if let Some(binding) = found(Namespace::Type) {
                let members: Vec<_> = interface
                    .members(&item.name)
                    .map(|e| (e.entity.namespace, Some(e.clone())))
                    .collect();
                if members.is_empty() {
                    diagnostics.push(no_members(import, interface, item));
                }
                out.push(binding);
                out.extend(members);
            }
        }
    }
    if out.is_empty() && item.kind != ItemKind::All {
        diagnostics.push(not_exported(interface, &item.name, item.span));
    }
    out
}

fn not_exported(interface: &Interface, name: &str, span: Span) -> Diagnostic {
    let module = &interface.module;
    match interface.hidden.iter().find(|h| h.name == name) {
        Some(hidden) => Diagnostic::error(
            codes::R005,
            format!(
                "constructor `{name}` is not exported: `{module}` exports `{}` without `(..)`",
                hidden.parent
            ),
            span,
        )
        .with_fix(
            format!(
                "export it from `{module}` as `{}(..)`, or build values through its functions",
                hidden.parent
            ),
            Vec::new(),
        ),
        None => Diagnostic::error(
            codes::R004,
            format!("`{module}` does not export `{name}`"),
            span,
        ),
    }
}

fn no_members(import: &Import, interface: &Interface, item: &Item) -> Diagnostic {
    let name = &item.name;
    let module = &import.module;
    let message = if interface
        .find(Namespace::Type, name)
        .is_some_and(|e| e.decl == Decl::Opaque)
    {
        format!("`{name}` is opaque: `{module}` does not export its constructors")
    } else if interface.hidden.iter().any(|h| &h.parent == name) {
        format!("`{module}` exports `{name}` without its constructors (no `(..)`)")
    } else {
        format!("`{name}` from `{module}` has no constructors to import")
    };
    Diagnostic::error(codes::R005, message, item.span)
        .with_fix(format!("import the type alone: `{name}`"), Vec::new())
}
