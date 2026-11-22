//! Source-level facade access detection for re-exporting modules with `@when`
//! or `[package] when` conditions.
//!
//! Typechecking and IR lowering resolve `Facade.value` (and unqualified uses
//! from `import Facade exposing (value)` / `exposing (..)`) directly to the
//! defining entity's `ExprKind::Global { module: "Shared", name: "value" }`.
//! This collector recovers the intermediate facade modules accessed at each
//! non-type use site so environment preflight and build pruning enforce
//! conditions on re-exporting facades as well as defining modules.

use std::collections::HashSet;

use crate::iface::{Export, Interface, Namespace};
use crate::ir::{Expr, ExprKind, Item, ItemKind};
use crate::project::{Compilation, Compiled};
use crate::resolve::{self, ItemKind as ExposeKind, SyntaxReference};
use crate::source::{SourceMap, Span};
use crate::syntax;
use crate::types::builtins_of;

/// One source-level reference that passes through at least one re-exporting
/// facade module (`facade != entity.module`).
#[derive(Debug, Clone)]
pub(super) struct FacadeUse {
    /// Accessed facade module names along the re-export chain (excluding the
    /// defining `entity.module`, which IR references already check).
    pub facades: Vec<String>,
    /// Display name for diagnostics (the referenced symbol name).
    pub name: String,
    /// Span in `compiled.file` (mapped back from plugin expansion if expanded).
    pub span: Span,
}

/// Collects every non-type reference in `compiled` that accesses an entity
/// through one or more re-exporting facade modules.
pub(super) fn module_facade_uses(
    compiled: &Compiled,
    compilation: &Compilation,
    sources: &SourceMap,
    by_class: bool,
) -> Vec<FacadeUse> {
    let (parsed_owned, text, generated) = if let Some(exp) = &compiled.expansion {
        (None, exp.text.as_str(), exp.len)
    } else {
        let text = sources.get(compiled.file).text.as_str();
        let parsed = syntax::parse(text, compiled.file);
        (Some(parsed), text, resolve::NO_GENERATED)
    };
    let tree = match &compiled.expansion {
        Some(exp) => &exp.tree,
        None => &parsed_owned.as_ref().expect("parsed module").tree,
    };
    let module_syntax = resolve::module_syntax(tree, text, compiled.file);
    let resolve_mod = resolve::Module {
        name: &compiled.name,
        file: compiled.file,
        text,
        tree,
        syntax: &module_syntax,
        generated,
    };
    let declared = resolve::declarations(tree, text, compiled.file);
    let local_defs: HashSet<(Namespace, &str)> = declared
        .iter()
        .map(|d| (d.namespace, d.name.as_str()))
        .collect();
    let builtins = builtins_of(by_class);
    let iface_of = |name: &str| -> Option<&Interface> {
        compilation
            .modules
            .iter()
            .find(|m| m.name == name)
            .map(|m| &m.interface)
            .or_else(|| builtins.iter().find(|i| i.module == name))
    };

    let references = resolve::syntax_references(&resolve_mod);
    let mut out = Vec::new();
    for reference in references {
        if reference.namespace == Namespace::Type {
            continue;
        }
        let Some((immediate, export)) =
            resolve_reference(&reference, &module_syntax.imports, &local_defs, &iface_of)
        else {
            continue;
        };
        let facades = collect_facades(immediate, export);
        if facades.is_empty() {
            continue;
        }
        let span = match &compiled.expansion {
            Some(exp) => exp.place(reference.span),
            None => reference.span,
        };
        out.push(FacadeUse {
            facades,
            name: reference.name,
            span,
        });
    }
    out
}

/// Returns the non-empty facade chain (`immediate` + `export.via`, excluding
/// `export.entity.module`).
fn collect_facades(immediate: &str, export: &Export) -> Vec<String> {
    let mut facades = Vec::new();
    if immediate != export.entity.module {
        facades.push(immediate.to_owned());
    }
    for via in &export.via {
        if via != &export.entity.module && !facades.iter().any(|m| m == via) {
            facades.push(via.clone());
        }
    }
    facades
}

/// Resolves a syntax reference in a module to `(immediate_module_name, export)`.
fn resolve_reference<'a>(
    reference: &SyntaxReference,
    imports: &'a [resolve::Import],
    local_defs: &HashSet<(Namespace, &str)>,
    iface_of: &impl Fn(&str) -> Option<&'a Interface>,
) -> Option<(&'a str, &'a Export)> {
    if let Some(qualifier) = reference.qualifier.as_deref() {
        // 1. Direct module qualifier (`Facade.value`, `Alias.value`, `Facade.Con`, `Facade.S`).
        if let Some((module, iface)) = resolve_qualifier(qualifier, imports, iface_of) {
            let export = find_member_export(iface, reference.namespace, &reference.name)?;
            return Some((module, export));
        }
        // 2. Qualified service op or type companion (`Facade.S.op`, `Facade.T.parse`).
        if let Some((mod_q, type_or_svc)) = qualifier.rsplit_once('.')
            && let Some((module, iface)) = resolve_qualifier(mod_q, imports, iface_of)
            && let Some(export) = iface.find(Namespace::Type, type_or_svc)
        {
            return Some((module, export));
        }
        // 3. Unqualified imported service/type qualifier (`S.op`, `T.parse`).
        if !local_defs.contains(&(Namespace::Type, qualifier)) {
            return resolve_unqualified(Namespace::Type, qualifier, imports, iface_of);
        }
        return None;
    }

    // Unqualified name (`value`, `Con`, or service tag `S`).
    if local_defs.contains(&(reference.namespace, reference.name.as_str())) {
        return None;
    }
    if let Some(found) =
        resolve_unqualified(reference.namespace, &reference.name, imports, iface_of)
    {
        return Some(found);
    }
    // `S` in `provide S ...` / `Layer.succeed S ...` is parsed as Constructor.
    if reference.namespace == Namespace::Constructor
        && !local_defs.contains(&(Namespace::Type, reference.name.as_str()))
    {
        return resolve_unqualified(Namespace::Type, &reference.name, imports, iface_of);
    }
    None
}

fn resolve_qualifier<'a>(
    qualifier: &str,
    imports: &'a [resolve::Import],
    iface_of: &impl Fn(&str) -> Option<&'a Interface>,
) -> Option<(&'a str, &'a Interface)> {
    if let Some(import) = imports.iter().find(|i| i.qualifier() == qualifier) {
        let iface = iface_of(&import.module)?;
        return Some((iface.module.as_str(), iface));
    }
    let iface = iface_of(qualifier)?;
    (!iface.private).then_some((iface.module.as_str(), iface))
}

fn find_member_export<'a>(
    iface: &'a Interface,
    namespace: Namespace,
    name: &str,
) -> Option<&'a Export> {
    if let Some(export) = iface.find(namespace, name) {
        return Some(export);
    }
    // Service tags (`Facade.S`) are tagged as `Constructor` syntactically.
    if namespace == Namespace::Constructor {
        return iface.find(Namespace::Type, name);
    }
    None
}

fn resolve_unqualified<'a>(
    namespace: Namespace,
    name: &str,
    imports: &'a [resolve::Import],
    iface_of: &impl Fn(&str) -> Option<&'a Interface>,
) -> Option<(&'a str, &'a Export)> {
    // 1. Explicit `exposing (name)` or `exposing (Parent(..))` imports win over open imports.
    for import in imports {
        let Some(iface) = iface_of(&import.module) else {
            continue;
        };
        for item in &import.exposing {
            if item.kind == ExposeKind::All {
                continue;
            }
            if item.name == name
                && let Some(export) = iface.find(namespace, name)
            {
                return Some((iface.module.as_str(), export));
            }
            if item.kind == ExposeKind::ConAll
                && let Some(member) = iface
                    .members(&item.name)
                    .find(|e| e.entity.namespace == namespace && e.entity.name == name)
            {
                return Some((iface.module.as_str(), member));
            }
        }
    }
    // 2. Open `exposing (..)` imports.
    for import in imports {
        if !import.exposing.iter().any(|i| i.kind == ExposeKind::All) {
            continue;
        }
        let Some(iface) = iface_of(&import.module) else {
            continue;
        };
        if let Some(export) = iface.find(namespace, name) {
            return Some((iface.module.as_str(), export));
        }
    }
    None
}

/// Returns every `FacadeUse` falling within `item.span`, excluding uses inside
/// unreachable local function bodies.
pub(super) fn item_facade_uses<'a>(uses: &'a [FacadeUse], item: &Item) -> Vec<&'a FacadeUse> {
    let span = item.span;
    let dead = super::locals::dead_local_fun_spans(item);
    uses.iter()
        .filter(|u| {
            u.span.file == span.file
                && u.span.start >= span.start
                && u.span.end <= span.end
                && !dead.iter().any(|d| {
                    u.span.file == d.file && u.span.start >= d.start && u.span.end <= d.end
                })
        })
        .collect()
}

/// Returns every `FacadeUse` in an eager top-level initializer of `item`
/// (excluding deferred bodies inside `Lambda`, `Task`, or `Abstract`).
pub(super) fn eager_facade_uses<'a>(uses: &'a [FacadeUse], item: &Item) -> Vec<&'a FacadeUse> {
    let mut deferred = Vec::new();
    match &item.kind {
        ItemKind::Value { dicts, body, .. }
            if dicts.is_empty() && !matches!(body.kind, ExprKind::Global { .. }) =>
        {
            collect_deferred_spans(body, &mut deferred);
        }
        ItemKind::Destructure { body, .. } => {
            collect_deferred_spans(body, &mut deferred);
        }
        ItemKind::Instance { info, methods } if info.context.is_empty() => {
            for (_, body) in methods {
                if matches!(body.kind, ExprKind::Global { .. }) {
                    deferred.push(body.span);
                } else {
                    collect_deferred_spans(body, &mut deferred);
                }
            }
        }
        _ => return Vec::new(),
    }
    item_facade_uses(uses, item)
        .into_iter()
        .filter(|u| {
            !deferred
                .iter()
                .any(|d| u.span.file == d.file && u.span.start >= d.start && u.span.end <= d.end)
        })
        .collect()
}

fn collect_deferred_spans(expr: &Expr, out: &mut Vec<Span>) {
    if matches!(
        expr.kind,
        ExprKind::Lambda { .. } | ExprKind::Task(_) | ExprKind::Abstract(..)
    ) {
        out.push(expr.span);
        return;
    }
    expr.each_child(&mut |child| collect_deferred_spans(child, out));
}
