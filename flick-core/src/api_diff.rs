//! The changes of a package's public API between two versions, by the
//! interfaces of its modules, and the version bump they call for, as Elm's
//! `elm diff` classifies them: a removed or changed export breaks
//! importers, an added one does not. A module's instances are its API too:
//! an importer sees each one, so removing one, or making its context
//! stronger, breaks code that used it.

use std::collections::BTreeMap;
use std::fmt;

use crate::iface::{Export, Interface, Namespace};
use crate::types::{Binder, Form, Instance, Row, Type, TypeDef, show_type};

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
    let old: BTreeMap<&str, &Interface> = old.iter().map(|i| (i.module.as_str(), i)).collect();
    let new: BTreeMap<&str, &Interface> = new.iter().map(|i| (i.module.as_str(), i)).collect();
    let mut modules: Vec<&str> = old.keys().chain(new.keys()).copied().collect();
    modules.sort_unstable();
    modules.dedup();
    modules
        .into_iter()
        .filter_map(|module| {
            let changes = match (old.get(module), new.get(module)) {
                (Some(_), None) => vec![Change::Removed(format!("module {module}"))],
                (None, Some(_)) => vec![Change::Added(format!("module {module}"))],
                (Some(o), Some(n)) => [exports(o, n), instances(o, n)].concat(),
                (None, None) => Vec::new(),
            };
            (!changes.is_empty()).then(|| ModuleChanges {
                module: module.to_owned(),
                changes,
            })
        })
        .collect()
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

/// `ty` with its free variables renamed by `names` and those its `forall`s
/// bind given fresh names.
fn rename(ty: &Type, names: &[(String, String)], next: &mut usize) -> Type {
    match ty {
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
        // the entry of a variable's captures or errors is labelled by its name
        Type::Row(row) => {
            let mut entries: Vec<(String, Type)> = (row.entries.iter())
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

/// `label` of a row entry, `^a` or `~a` for a variable `a`, after `names`.
fn relabel(label: &str, names: &[(String, String)]) -> String {
    let Some((sigil @ ("^" | "~"), name)) = label.split_at_checked(1) else {
        return label.to_owned();
    };
    (names.iter().find(|(n, _)| n == name))
        .map_or_else(|| label.to_owned(), |(_, fresh)| format!("{sigil}{fresh}"))
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
mod tests {
    use super::*;
    use crate::source::SourceMap;
    use std::slice;

    use crate::{resolve, syntax, types};

    fn interface(text: &str) -> Interface {
        let mut sources = SourceMap::default();
        let file = sources.add("Lib.fl", text.to_owned());
        let parse = syntax::parse(text, file);
        let syntax = resolve::module_syntax(&parse.tree, text, file);
        let module = resolve::Module {
            name: "Lib",
            file,
            text,
            tree: &parse.tree,
            syntax: &syntax,
            generated: resolve::NO_GENERATED,
        };
        let builtin = |name: &str| types::builtins().iter().find(|i| i.module == name).cloned();
        let (mut interface, _) = resolve::resolve(&module, &builtin);
        let local = |n: &str| n == "Lib";
        let options = types::Options {
            local: &local,
            limits: types::Limits::default(),
            no_let_assert: false,
            thresholds: types::Thresholds::default(),
            effects: true,
            watched: types::Watched::none(),
            stages_by_class: false,
        };
        let _ = types::check(
            &module,
            &builtin,
            types::builtins(),
            options,
            &mut interface,
        );
        interface
    }

    #[test]
    fn removed_and_changed_exports_are_major_added_ones_minor() {
        let old = interface(
            "module Lib exposing (..)\n\nf : Float -> Float\nlet f x = x\n\ng : Float\nlet g = 1\n",
        );
        let new = interface(
            "module Lib exposing (..)\n\nf : String -> String\nlet f x = x\n\nh : Float\nlet h = 2\n",
        );
        let changes = diff(slice::from_ref(&old), &[new]);
        assert_eq!(
            changes[0].changes,
            [
                Change::Changed {
                    name: "value f".into(),
                    was: "Float -> Float".into(),
                    now: "String -> String".into(),
                },
                Change::Removed("value g".into()),
                Change::Added("value h".into()),
            ]
        );
        assert_eq!(bump(&changes), Bump::Major);
        let more = interface(
            "module Lib exposing (..)\n\nf : Float -> Float\nlet f x = x\n\ng : Float\nlet g = 1\n\nk : Float\nlet k = 3\n",
        );
        assert_eq!(bump(&diff(slice::from_ref(&old), &[more])), Bump::Minor);
        assert_eq!(
            bump(&diff(slice::from_ref(&old), slice::from_ref(&old))),
            Bump::Patch
        );
        assert_eq!(next_version("1.2.3", Bump::Major).as_deref(), Some("2.0.0"));
        assert_eq!(next_version("0.2.3", Bump::Major).as_deref(), Some("0.3.0"));
        assert_eq!(next_version("1.2.3", Bump::Patch).as_deref(), Some("1.2.4"));
    }

    #[test]
    fn a_removed_instance_or_a_stronger_context_is_major() {
        let thing = "module Lib exposing (..)\n\ntype Thing = Thing\n\ntype Box a = Box a\n\n";
        let base = interface(&format!(
            "{thing}instance Show Thing =\n  let show _ = \"thing\"\n\ninstance Show (Box a) =\n  let show _ = \"box\"\n"
        ));
        let removed = interface(&format!(
            "{thing}instance Show (Box a) =\n  let show _ = \"box\"\n"
        ));
        let changes = diff(slice::from_ref(&base), &[removed]);
        assert_eq!(
            changes[0].changes,
            [Change::Removed("instance Show Thing".into())]
        );
        assert_eq!(bump(&changes), Bump::Major);
        assert_eq!(
            next_version("1.2.3", bump(&changes)).as_deref(),
            Some("2.0.0")
        );
        let stronger = interface(&format!(
            "{thing}instance Show Thing =\n  let show _ = \"thing\"\n\ninstance Show a => Show (Box a) =\n  let show (Box x) = show x\n"
        ));
        assert_eq!(
            bump(&diff(slice::from_ref(&base), slice::from_ref(&stronger))),
            Bump::Major
        );
        // the reverse takes a constraint away: importers lose nothing
        assert_eq!(
            bump(&diff(slice::from_ref(&stronger), slice::from_ref(&base))),
            Bump::Minor
        );
        // the same instances in another order are the same API
        let moved = interface(&format!(
            "{thing}instance Show (Box a) =\n  let show _ = \"box\"\n\ninstance Show Thing =\n  let show _ = \"thing\"\n"
        ));
        assert_eq!(bump(&diff(slice::from_ref(&base), &[moved])), Bump::Patch);
    }

    #[test]
    fn a_renamed_type_variable_is_no_change() {
        let api = |a: &str, b: &str| {
            interface(&format!(
                "module Lib exposing (..)\n\nf : {a} -> {b} -> {a}\nlet f x _ = x\n\n\
                 type Box {a} = Box {a}\n\ntype alias Pair {a} = ({a}, {a})\n\n\
                 class Sized {a} =\n  size : {a} -> Float\n\n\
                 instance Show (Box {a}) =\n  let show _ = \"box\"\n"
            ))
        };
        let old = api("a", "b");
        assert_eq!(diff(slice::from_ref(&old), &[api("item", "other")]), []);
        assert_eq!(
            diff(slice::from_ref(&old), &[api("b", "a")]),
            [],
            "names swapped"
        );
        let changes = diff(slice::from_ref(&old), &[api("a", "a")]);
        assert_eq!(
            changes[0].changes,
            [Change::Changed {
                name: "value f".into(),
                was: "forall a b. a -> (b -> a)^{a}".into(),
                now: "forall a. a -> (a -> a)^{a}".into(),
            }]
        );
    }
}
