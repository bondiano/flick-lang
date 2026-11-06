//! A build without effects, `[build] effect = "none"`: compiled code imports
//! `@flick-lang/runtime/core`, which has the runtime's data and pure helpers
//! without Effect. A foreign module with no such version cannot be built.

use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::ir::{self, ItemKind};
use crate::types::{NO_EFFECTS, OpClass};

/// Where a library on Effect writes its build without Effect, beside its
/// interfaces: the modules that need no Effect, and the built-in ones they use.
pub const CORE_DIR: &str = "_core/";

const RUNTIME: &str = "@flick-lang/runtime";

/// The built-in modules whose foreign bindings `core` has, every one.
const BUILTINS: &[&str] = &[
    "Bool",
    "Float",
    "Instances",
    "Int",
    "List",
    "List.Core",
    "List.Search",
    "List.Slice",
    "List.Transform",
    "Option",
    "Prelude",
    "Result",
    "Monoids",
    "Seq",
    "String",
    "Validation",
];

/// The modules of `@flick-lang/runtime/effect/` that `core` has too, with
/// the exports the modules of [`BUILTINS`] bind.
const IN_CORE: &[&str] = &[
    "Array", "Boolean", "Iterable", "Number", "Option", "Result", "String",
];

/// What a foreign module `spec` is without effects; `None` when it has no
/// version without Effect. A module of the program's own is itself.
pub fn specifier(spec: &str) -> Option<String> {
    match spec.strip_prefix(RUNTIME) {
        None => Some(spec.to_owned()),
        Some("") => Some(format!("{RUNTIME}/core")),
        Some(rest) => rest
            .strip_prefix("/effect/")
            .filter(|m| IN_CORE.contains(m))
            .map(|m| format!("{RUNTIME}/core/{m}")),
    }
}

/// E008 when the program's `module` binds a foreign module that needs
/// Effect, or makes a schema, which `core` has no codecs for.
pub fn check(module: &ir::Module, diagnostics: &mut Vec<Diagnostic>) {
    let needed = module.items.iter().find_map(|item| match &item.kind {
        // shortcut: checks the module, not the export; one `core` lacks fails when the program loads
        ItemKind::Foreign(f) => f
            .binding
            .from
            .as_deref()
            .filter(|spec| specifier(spec).is_none())
            .map(|spec| (item.span, format!("`{spec}` runs on Effect"))),
        ItemKind::Refined { name, .. } => Some((
            item.span,
            format!("the refined type `{name}` parses with Schema"),
        )),
        _ => None,
    });
    if let Some((span, what)) = needed {
        let message = format!("{what}, but {NO_EFFECTS}");
        diagnostics.push(Diagnostic::error(codes::E008, message, span));
    }
}

/// Whether the built-in module `name` has a build without Effect.
pub fn in_core(name: &str) -> bool {
    BUILTINS.contains(&name)
}

/// E008 when the program uses the built-in module `name`, which needs Effect.
pub fn check_builtin(name: &str, diagnostics: &mut Vec<Diagnostic>) {
    if !in_core(name) {
        let message = format!("the built-in module `{name}` runs on Effect, but {NO_EFFECTS}");
        diagnostics.push(Diagnostic::new(codes::E008, Severity::Error, message));
    }
}

/// P012 at each handler of a `control` operation: on JavaScript it needs a
/// runtime of its own, which Go has already.
pub fn control_on_js(module: &ir::Module, diagnostics: &mut Vec<Diagnostic>) {
    fn walk(e: &ir::Expr, diagnostics: &mut Vec<Diagnostic>) {
        if let ir::ExprKind::Handle(h) = &e.kind {
            let control = h.effects.iter().any(|handled| {
                handled
                    .operations
                    .iter()
                    .any(|o| o.class == OpClass::Control)
            });
            if control {
                let message =
                    "handlers of `control` operations are checked, but compiled only to Go yet"
                        .to_owned();
                diagnostics.push(Diagnostic::error(codes::P012, message, e.span));
            }
        }
        e.each_child(&mut |c| walk(c, diagnostics));
    }
    for item in &module.items {
        match &item.kind {
            ItemKind::Value { body, .. } | ItemKind::Destructure { body, .. } => {
                walk(body, diagnostics);
            }
            ItemKind::Refined { check, .. } => walk(check, diagnostics),
            ItemKind::Instance { methods, .. } => {
                for (_, m) in methods {
                    walk(m, diagnostics);
                }
            }
            ItemKind::Foreign(_) | ItemKind::Constructor(_) | ItemKind::Service { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BUILTINS, specifier};
    use crate::types::builtin_sources;

    #[test]
    fn builtins_without_effects_bind_only_core_and_import_each_other() {
        for (name, source) in builtin_sources()
            .iter()
            .filter(|(n, _)| BUILTINS.contains(n))
        {
            for line in source.lines() {
                if let Some(spec) = line
                    .strip_prefix("foreign ")
                    .and_then(|l| l.split_once(" from \""))
                    .and_then(|(_, rest)| rest.split_once('"'))
                    .map(|(spec, _)| spec)
                {
                    assert!(specifier(spec).is_some(), "{name} binds `{spec}`");
                }
                if let Some(import) = line.strip_prefix("import ") {
                    let module = import.split_whitespace().next().unwrap_or_default();
                    assert!(BUILTINS.contains(&module), "{name} imports `{module}`");
                }
            }
        }
    }

    #[test]
    fn runtime_modules_map_to_core_or_need_effect() {
        assert_eq!(
            specifier("@flick-lang/runtime").as_deref(),
            Some("@flick-lang/runtime/core")
        );
        assert_eq!(
            specifier("@flick-lang/runtime/effect/Option").as_deref(),
            Some("@flick-lang/runtime/core/Option")
        );
        assert_eq!(specifier("@flick-lang/runtime/effect/HashMap"), None);
        assert_eq!(specifier("@flick-lang/runtime/ui/react"), None);
        assert_eq!(specifier("./clock.mjs").as_deref(), Some("./clock.mjs"));
    }
}
