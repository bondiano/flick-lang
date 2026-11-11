//! The changes of a package's public API between two versions, by the
//! interfaces of its modules, and the version bump they call for, as Elm's
//! `elm diff` classifies them: a removed or changed export breaks
//! importers, an added one does not. A module's instances are its API too:
//! an importer sees each one, so removing one, or making its context
//! stronger, breaks code that used it.

use crate::types::{Atom, Elem};
use std::collections::BTreeMap;
use std::fmt;
use std::iter::once;

use crate::iface::{Export, Interface, Namespace};
use crate::types::{Binder, Form, Instance, ModuleSignature, Row, Type, TypeDef, show_type};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Added(String),
    Removed(String),
    /// What changed, as it was and as it is.
    Changed {
        name: String,
        was: String,
        now: String,
    },
}

/// The changes of one module; a whole module added or removed is one change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleChanges {
    pub module: String,
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Bump {
    Patch,
    Minor,
    Major,
}

impl fmt::Display for Bump {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Bump::Patch => "PATCH",
            Bump::Minor => "MINOR",
            Bump::Major => "MAJOR",
        })
    }
}

/// The changes from `old` to `new`, by module name.
pub fn diff(old: &[Interface], new: &[Interface]) -> Vec<ModuleChanges> {
    let old: BTreeMap<&str, &Interface> = old
        .iter()
        .filter(|i| !i.private && i.member_of.is_none())
        .map(|i| (i.module.as_str(), i))
        .collect();
    let new: BTreeMap<&str, &Interface> = new
        .iter()
        .filter(|i| !i.private && i.member_of.is_none())
        .map(|i| (i.module.as_str(), i))
        .collect();
    let mut modules: Vec<&str> = old.keys().chain(new.keys()).copied().collect();
    modules.sort_unstable();
    modules.dedup();
    modules
        .into_iter()
        .filter_map(|module| {
            let changes = match (old.get(module), new.get(module)) {
                (Some(_), None) => vec![Change::Removed(format!("module {module}"))],
                (None, Some(_)) => vec![Change::Added(format!("module {module}"))],
                (Some(o), Some(n)) => {
                    [exports(o, n), instances(o, n), static_members(o, n)].concat()
                }
                (None, None) => Vec::new(),
            };
            (!changes.is_empty()).then(|| ModuleChanges {
                module: module.to_owned(),
                changes,
            })
        })
        .collect()
}

fn static_members(old: &Interface, new: &Interface) -> Vec<Change> {
    let canonical = |signature: &ModuleSignature| {
        let mut signature = signature.clone();
        signature.canonicalize(&mut 0);
        signature
    };
    let mut out = Vec::new();
    if old.functor.as_ref().map(canonical) != new.functor.as_ref().map(canonical) {
        out.push(Change::Changed {
            name: format!("module {}", old.module),
            was: old
                .functor
                .as_ref()
                .map_or_else(|| "structure".into(), ModuleSignature::display),
            now: new
                .functor
                .as_ref()
                .map_or_else(|| "structure".into(), ModuleSignature::display),
        });
    }
    for (name, child) in &old.modules {
        match new.modules.get(name) {
            None => out.push(Change::Removed(format!("module {name}"))),
            Some(new) => {
                for change in [
                    exports(child, new),
                    instances(child, new),
                    static_members(child, new),
                ]
                .concat()
                {
                    let prefix = |label: String| format!("module {name}: {label}");
                    out.push(match change {
                        Change::Added(label) => Change::Added(prefix(label)),
                        Change::Removed(label) => Change::Removed(prefix(label)),
                        Change::Changed { name, was, now } => Change::Changed {
                            name: prefix(name),
                            was,
                            now,
                        },
                    });
                }
            }
        }
    }
    out.extend(
        new.modules
            .keys()
            .filter(|name| !old.modules.contains_key(*name))
            .map(|name| Change::Added(format!("module {name}"))),
    );
    out
}

/// The bump a set of changes calls for.
pub fn bump(changes: &[ModuleChanges]) -> Bump {
    changes
        .iter()
        .flat_map(|m| &m.changes)
        .map(|c| match c {
            Change::Added(_) => Bump::Minor,
            Change::Removed(_) | Change::Changed { .. } => Bump::Major,
        })
        .max()
        .unwrap_or(Bump::Patch)
}

/// `version` (`major.minor.patch`) after `bump`; before 1.0 a breaking
/// change bumps the minor version, as npm's caret ranges read it.
pub fn next_version(version: &str, bump: Bump) -> Option<String> {
    let mut parts = version.split('.').map(str::parse::<u64>);
    let (major, minor, patch) = (
        parts.next()?.ok()?,
        parts.next()?.ok()?,
        parts.next()?.ok()?,
    );
    Some(match (bump, major) {
        (Bump::Major, 0) | (Bump::Minor, _) => format!("{major}.{}.0", minor + 1),
        (Bump::Major, _) => format!("{}.0.0", major + 1),
        (Bump::Patch, _) => format!("{major}.{minor}.{}", patch + 1),
    })
}

fn exports(old: &Interface, new: &Interface) -> Vec<Change> {
    let key = |e: &Export| (e.entity.namespace, e.entity.name.clone());
    let public = |i: &Interface| -> BTreeMap<(Namespace, String), Export> {
        i.exports
            .iter()
            .filter(|e| !e.entity.name.contains('$'))
            .map(|e| (key(e), e.clone()))
            .collect()
    };
    let (old, new) = (public(old), public(new));
    let mut out = Vec::new();
    for (k, o) in &old {
        match new.get(k) {
            None => out.push(Change::Removed(label(o))),
            Some(n) if canonical(o) != canonical(n) => out.push(Change::Changed {
                name: label(o),
                was: shape(o),
                now: shape(n),
            }),
            Some(_) => {}
        }
    }
    out.extend(
        new.iter()
            .filter(|(k, _)| !old.contains_key(*k))
            .map(|(_, n)| Change::Added(label(n))),
    );
    out
}

/// The instances the module declares, compared by head and context: a move
/// in the source, or a renamed variable, is no change.
fn instances(old: &Interface, new: &Interface) -> Vec<Change> {
    let (old, new) = (own(old), own(new));
    let mut out = Vec::new();
    for (key, (inst, was)) in &old {
        let name = format!("instance {}", show_type(&inst.head));
        match new.get(key) {
            None => out.push(Change::Removed(name)),
            // a constraint fewer takes nothing from an importer
            Some((_, now)) if now.iter().all(|c| was.contains(c)) && now != was => {
                out.push(Change::Added(format!("{name} with fewer constraints")));
            }
            Some((then, now)) if now != was => out.push(Change::Changed {
                name,
                was: context(inst),
                now: context(then),
            }),
            Some(_) => {}
        }
    }
    out.extend(
        (new.iter())
            .filter(|(key, _)| !old.contains_key(*key))
            .map(|(_, (inst, _))| Change::Added(format!("instance {}", show_type(&inst.head)))),
    );
    out
}

/// The instances `i` declares, by head with their variables renamed, with
/// their contexts so renamed, sorted.
fn own(i: &Interface) -> BTreeMap<String, (&Instance, Vec<String>)> {
    (i.instances.iter())
        .filter(|inst| inst.module == i.module)
        .map(|inst| {
            let names = bind(inst.vars.iter().map(|v| &v.name), &mut 0);
            let renamed = |t: &Type| show_type(&rename(t, &names, &mut names.len()));
            let mut context: Vec<String> = inst.context.iter().map(renamed).collect();
            context.sort();
            (renamed(&inst.head), (inst, context))
        })
        .collect()
}

/// The context of `inst` as it reads, sorted.
fn context(inst: &Instance) -> String {
    let mut context: Vec<String> = inst.context.iter().map(show_type).collect();
    context.sort();
    context.join(", ")
}

/// `e` as it is compared: without its place in the source, its variables
/// named by the order they are bound in, so renaming one is no change.
fn canonical(e: &Export) -> Export {
    let next = &mut 0;
    let scheme = e.scheme.as_ref().map(|t| rename(t, &[], next));
    let definition = e.definition.as_ref().map(|d| {
        let mut form = d.form.clone();
        let names = match &mut form {
            Form::Alias { params, .. }
            | Form::Service { params, .. }
            | Form::Class { params, .. } => {
                let names = bind(params.iter(), next);
                *params = names.iter().map(|(_, fresh)| fresh.clone()).collect();
                names
            }
            _ => Vec::new(),
        };
        let mut types = |t: &mut Type| *t = rename(t, &names, next);
        match &mut form {
            Form::ModuleSignature { signature } => signature.canonicalize(next),
            Form::Alias { body, .. } => types(body),
            Form::Service { operations, .. } => {
                for o in operations {
                    types(&mut o.ty);
                }
            }
            Form::Class {
                supers,
                default_sigs,
                ..
            } => {
                supers
                    .iter_mut()
                    .chain(default_sigs.iter_mut().map(|(_, t)| t))
                    .for_each(types);
            }
            _ => {}
        }
        TypeDef {
            kind: d.kind.clone(),
            form,
        }
    });
    Export {
        at: None,
        scheme,
        definition,
        ..e.clone()
    }
}

/// A fresh name for each of `names`, counted from `next`: `'n`, which no
/// program writes, so it captures none of them.
fn bind<'a>(
    names: impl IntoIterator<Item = &'a String>,
    next: &mut usize,
) -> Vec<(String, String)> {
    (names.into_iter())
        .map(|name| {
            *next += 1;
            (name.clone(), format!("'{next}"))
        })
        .collect()
}

/// A scheme with its quantified variables named by binder order.
pub(crate) fn canonical_type(ty: &Type) -> Type {
    rename(ty, &[], &mut 0)
}

/// `ty` with its free variables renamed by `names` and those its `forall`s
/// bind given fresh names.
fn rename(ty: &Type, names: &[(String, String)], next: &mut usize) -> Type {
    match ty {
        Type::Con { module, name } if let Some(binder) = module.strip_prefix("$exists:") => {
            let binder = names
                .iter()
                .find(|(old, _)| old == binder)
                .map_or(binder, |(_, fresh)| fresh);
            Type::con(&format!("$exists:{binder}"), name)
        }
        Type::Var(name) => (names.iter().find(|(n, _)| n == name))
            .map_or_else(|| ty.clone(), |(_, fresh)| Type::Var(fresh.clone())),
        Type::Forall {
            vars,
            constraints,
            body,
        } => {
            let bound = bind(vars.iter().map(|v| &v.name), next);
            // first, so a name bound here shadows an outer one
            let inner: Vec<(String, String)> = bound.iter().chain(names).cloned().collect();
            Type::Forall {
                vars: (vars.iter().zip(&bound))
                    .map(|(v, (_, fresh))| Binder {
                        name: fresh.clone(),
                        kind: v.kind.clone(),
                    })
                    .collect(),
                constraints: constraints
                    .iter()
                    .map(|c| rename(c, &inner, next))
                    .collect(),
                body: Box::new(rename(body, &inner, next)),
            }
        }
        Type::Exists {
            binder,
            contract,
            body,
        } => {
            let fresh = bind([binder], next).pop().expect("one binder").1;
            let inner: Vec<(String, String)> = once((binder.clone(), fresh.clone()))
                .chain(names.iter().cloned())
                .collect();
            Type::Exists {
                binder: fresh,
                contract: Box::new(rename(contract, names, next)),
                body: Box::new(rename(body, &inner, next)),
            }
        }
        // Entries of bound capture, error and service atoms follow their names.
        Type::Row(row) => {
            let mut entries: Vec<(Elem, Type)> = (row.entries.iter())
                .map(|(label, t)| (relabel(label, names), rename(t, names, next)))
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Type::Row(Row {
                entries,
                tail: row.tail.as_ref().map(|t| Box::new(rename(t, names, next))),
                ..Row::default()
            })
        }
        _ => ty.map_children(&mut |t| rename(t, names, next)),
    }
}

/// `label` of a bound row atom (`^a`, `~a` or `@a`) after `names`.
fn relabel(label: &Elem, names: &[(String, String)]) -> Elem {
    let fresh = |atom: &Atom| match atom {
        Atom::Var(name) => {
            (names.iter().find(|(n, _)| n == name)).map(|(_, f)| Atom::Var(f.clone()))
        }
        Atom::Skolem(_) | Atom::Meta(_) => None,
    };
    let renamed = match label {
        Elem::Capture(atom) => fresh(atom).map(Elem::Capture),
        Elem::Error(atom) => fresh(atom).map(Elem::Error),
        Elem::Service(atom) => fresh(atom).map(Elem::Service),
        Elem::Named(_) | Elem::Live | Elem::Provided(_) => None,
    };
    renamed.unwrap_or_else(|| label.clone())
}

fn label(e: &Export) -> String {
    let what = match e.entity.namespace {
        Namespace::Value => "value",
        Namespace::Type => "type",
        Namespace::Constructor => "constructor",
    };
    format!("{what} {}", e.entity.name)
}

/// How an export reads: its type, or the JSON of its definition.
fn shape(e: &Export) -> String {
    match (&e.scheme, &e.definition) {
        (Some(t), _) => show_type(t),
        (None, Some(d)) => serde_json::to_string(d).unwrap_or_default(),
        (None, None) => format!("{:?}", e.decl),
    }
}

#[cfg(test)]
mod tests;
