//! A build without effects, `[build] effect = "none"`: compiled code imports
//! `@flick-lang/runtime/core`, which has the runtime's data and pure helpers
//! without Effect. A foreign module with no such version cannot be built.

use crate::codegen::EffectBridge;
use crate::diagnostics::{Diagnostic, Severity, codes};
use crate::ir::{self, ItemKind};
use crate::types::{NO_EFFECTS, OpClass};

/// Where a library on Effect writes its build without Effect, beside its
/// interfaces: the modules that need no Effect, and the built-in ones they use.
pub const CORE_DIR: &str = "_core/";

const RUNTIME: &str = "@flick-lang/runtime";
const EFFECT_TS: &str = "@flick-lang/effect-ts";

/// The former Effect adapter specifier, accepted in foreign declarations
/// written before the packages split. Generated imports use the new package.
pub fn effect_specifier(spec: &str) -> String {
    match spec.strip_prefix(RUNTIME) {
        Some(rest)
            if rest.is_empty()
                || rest.starts_with("/effect/")
                || rest.starts_with("/ui")
                || rest.starts_with("/runner/")
                || rest.starts_with("/sql")
                || rest.starts_with("/cli") =>
        {
            format!("{EFFECT_TS}{rest}")
        }
        _ => spec.to_owned(),
    }
}

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
    match spec
        .strip_prefix(EFFECT_TS)
        .or_else(|| spec.strip_prefix(RUNTIME))
    {
        None => Some(spec.to_owned()),
        Some("") => Some(format!("{RUNTIME}/core")),
        Some(rest) => rest
            .strip_prefix("/effect/")
            .filter(|m| IN_CORE.contains(m))
            .map(|m| format!("{RUNTIME}/core/{m}")),
    }
}

/// The modules of `@flick-lang/runtime/effect/` that Flick's own runtime has,
/// under `@flick-lang/runtime/rt/effect/`, for the exports it has.
const IN_RT: &[&str] = &[
    "Arbitrary",
    "Array",
    "BigInt",
    "BigDecimal",
    "ByteSize",
    "Cause",
    "Clock",
    "Chunk",
    "Combiner",
    "Crypto",
    "DateTime",
    "Deferred",
    "Differ",
    "Duration",
    "Effect",
    "Exit",
    "Fiber",
    "FiberSet",
    "Filter",
    "Function",
    "Hash",
    "HashMap",
    "HashRing",
    "HashSet",
    "Formatter",
    "Inspectable",
    "JsonPatch",
    "JsonPointer",
    "JsonSchema",
    "Latch",
    "Layer",
    "Queue",
    "Random",
    "Number",
    "Option",
    "Optic",
    "Record",
    "Reducer",
    "Predicate",
    "RegExp",
    "Redacted",
    "Schedule",
    "Schema",
    "Scope",
    "String",
    "Struct",
    "Tuple",
    "Tracer",
    "Trie",
    "Version",
    "reactivity/AsyncResult",
    "reactivity/Atom",
];

/// The entry points of the UI layer, which Flick's own runtime has under
/// `@flick-lang/runtime/rt/`.
const UI_IN_RT: &[&str] = &["/ui", "/ui/react", "/ui/html", "/ui/server"];

/// What a foreign module `spec` is on Flick's own runtime; `None` when it has no
/// version there. A module of the program's own is itself.
pub fn rt_specifier(spec: &str) -> Option<String> {
    match spec
        .strip_prefix(EFFECT_TS)
        .or_else(|| spec.strip_prefix(RUNTIME))
    {
        None => Some(spec.to_owned()),
        Some("") => Some(format!("{RUNTIME}/rt")),
        Some(rest) if UI_IN_RT.contains(&rest) => Some(format!("{RUNTIME}/rt{rest}")),
        Some(rest) => {
            let module = rest.strip_prefix("/effect/")?;
            if IN_RT.contains(&module) {
                Some(format!("{RUNTIME}/rt/effect/{module}"))
            } else if IN_CORE.contains(&module) {
                Some(format!("{RUNTIME}/core/{module}"))
            } else {
                None
            }
        }
    }
}

/// The exports implemented by the owned runtime. Other adapter exports cross
/// into Effect through the optional bridge.
const RT_EXPORTS: &str = include_str!("rt_exports.txt");

pub fn on_kernel(spec: &str, export: &str) -> bool {
    if !spec
        .strip_prefix(RUNTIME)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    {
        return true;
    }
    RT_EXPORTS
        .lines()
        .any(|line| line.split_once(' ') == Some((spec, export)))
}

/// Throwing `core` placeholders also appear in the RT index. A build with an
/// installed Effect adapter must route them to their real implementations.
pub fn on_kernel_with_bridge(spec: &str, export: &str, bridge: bool) -> bool {
    selected_on_kernel(
        spec,
        export,
        if bridge {
            EffectBridge::Available
        } else {
            EffectBridge::Absent
        },
    )
}

pub fn ui_family(spec: &str, export: &str) -> bool {
    // Both renderers re-export these exact inert constructors from
    // runtime/ui/react/nodes. They hold no registry or runtime task, so
    // crossing the adapter here only walks the same tree repeatedly.
    if matches!(
        spec,
        "@flick-lang/runtime/rt/ui" | "@flick-lang/runtime/rt/ui/react"
    ) && include_str!("shared_ui_exports.txt")
        .lines()
        .any(|name| name == export)
    {
        return false;
    }
    matches!(
        spec,
        "@flick-lang/runtime/rt/effect/reactivity/Atom"
            | "@flick-lang/runtime/rt/ui"
            | "@flick-lang/runtime/rt/ui/html"
            | "@flick-lang/runtime/rt/ui/react"
            | "@flick-lang/runtime/rt/ui/server"
    ) || (spec == "@flick-lang/runtime/rt"
        && (export == "label"
            || ["atom", "form", "route"].iter().any(|prefix| {
                export
                    .strip_prefix(prefix)
                    .is_some_and(|rest| rest.starts_with(char::is_uppercase))
            })))
}

pub fn selected_on_kernel(spec: &str, export: &str, bridge: EffectBridge) -> bool {
    const STUBS: &str = include_str!("rt_stubs.txt");
    const BRIDGE_STUBS: &str = include_str!("rt_bridge_stubs.txt");
    if bridge == EffectBridge::AvailableWithUi && ui_family(spec, export) {
        return false;
    }
    let bridge = bridge.available();
    let core_stub = spec == "@flick-lang/runtime/rt" && STUBS.lines().any(|name| name == export);
    let module_stub = BRIDGE_STUBS
        .lines()
        .any(|line| line.split_once(' ') == Some((spec, export)));
    !(bridge && (core_stub || module_stub)) && on_kernel(spec, export)
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
    use super::{BUILTINS, effect_specifier, on_kernel, rt_specifier, specifier};
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
    fn runtime_modules_map_to_flick_runtime_core_or_stay_absent() {
        assert_eq!(
            effect_specifier("@flick-lang/runtime/effect/Stream"),
            "@flick-lang/effect-ts/effect/Stream"
        );
        assert_eq!(
            effect_specifier("@flick-lang/runtime/rt"),
            "@flick-lang/runtime/rt"
        );
        assert_eq!(
            rt_specifier("@flick-lang/effect-ts/effect/Fiber").as_deref(),
            Some("@flick-lang/runtime/rt/effect/Fiber")
        );
        assert_eq!(
            rt_specifier("@flick-lang/runtime").as_deref(),
            Some("@flick-lang/runtime/rt")
        );
        assert_eq!(
            rt_specifier("@flick-lang/runtime/effect/Fiber").as_deref(),
            Some("@flick-lang/runtime/rt/effect/Fiber")
        );
        assert_eq!(
            rt_specifier("@flick-lang/runtime/effect/DateTime").as_deref(),
            Some("@flick-lang/runtime/rt/effect/DateTime")
        );
        assert_eq!(
            rt_specifier("@flick-lang/runtime/effect/HashMap").as_deref(),
            Some("@flick-lang/runtime/rt/effect/HashMap")
        );
        assert_eq!(
            rt_specifier("@flick-lang/runtime/effect/Option").as_deref(),
            Some("@flick-lang/runtime/rt/effect/Option")
        );
        assert_eq!(
            rt_specifier("@flick-lang/runtime/effect/reactivity/Atom").as_deref(),
            Some("@flick-lang/runtime/rt/effect/reactivity/Atom")
        );
        assert_eq!(rt_specifier("@flick-lang/runtime/effect/Stream"), None);
        assert_eq!(
            rt_specifier("@flick-lang/runtime/ui/react").as_deref(),
            Some("@flick-lang/runtime/rt/ui/react")
        );
        assert_eq!(rt_specifier("@flick-lang/runtime/ui/other"), None);
        assert_eq!(rt_specifier("./clock.mjs").as_deref(), Some("./clock.mjs"));
        assert!(on_kernel("@flick-lang/runtime-extra", "anyExport"));
        assert!(on_kernel("@flick-lang/runtime/rt", "scopeRun"));
        assert!(!on_kernel("@flick-lang/runtime/rt", "missingExport"));
    }

    #[test]
    fn effect_bridge_routes_throwing_rt_stubs_to_the_effect_adapter() {
        assert!(!super::on_kernel_with_bridge(
            "@flick-lang/runtime/rt",
            "formSpec",
            true
        ));
        assert!(super::on_kernel_with_bridge(
            "@flick-lang/runtime/rt",
            "formSpec",
            false
        ));
        assert!(!super::on_kernel_with_bridge(
            "@flick-lang/runtime/rt/effect/Schedule",
            "cron",
            true
        ));
        assert!(super::on_kernel_with_bridge(
            "@flick-lang/runtime/rt/effect/Schedule",
            "cron",
            false
        ));
        assert!(super::on_kernel_with_bridge(
            "@flick-lang/runtime/rt",
            "schemaField",
            true
        ));
        assert!(super::on_kernel_with_bridge(
            "@flick-lang/runtime/rt",
            "dslFieldEffect",
            true
        ));
        for export in ["redactedMake", "redactedReveal"] {
            assert!(!super::on_kernel_with_bridge(
                "@flick-lang/runtime/rt",
                export,
                true
            ));
            assert!(super::on_kernel_with_bridge(
                "@flick-lang/runtime/rt",
                export,
                false
            ));
        }
        assert!(!super::on_kernel_with_bridge(
            "@flick-lang/runtime/rt/effect/Redacted",
            "flickEquivalent",
            true
        ));
    }

    #[test]
    fn shared_inert_ui_nodes_stay_on_kernel_with_adapter_views() {
        use crate::codegen::EffectBridge;
        for export in [
            "uiText",
            "uiParagraph",
            "uiHeading",
            "uiDiv",
            "uiFragment",
            "uiKeyed",
            "uiComponent",
        ] {
            assert!(
                super::selected_on_kernel(
                    "@flick-lang/runtime/rt/ui",
                    export,
                    EffectBridge::AvailableWithUi
                ),
                "{export}"
            );
        }
        assert!(!super::selected_on_kernel(
            "@flick-lang/runtime/rt/ui",
            "uiView",
            EffectBridge::AvailableWithUi
        ));
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
