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

use super::bindings;

pub fn effect_specifier(spec: &str) -> String {
    bindings::ALIASES
        .iter()
        .find_map(|(from, to, prefix)| {
            spec.strip_prefix(from)
                .filter(|rest| rest.is_empty() || *prefix)
                .map(|rest| format!("{to}{rest}"))
        })
        .unwrap_or_else(|| spec.to_owned())
}

fn route(spec: &str, native: bool) -> Option<String> {
    if let Some((_, core, rt)) = bindings::ROUTES.iter().find(|(from, _, _)| *from == spec) {
        return if native { *rt } else { *core }.map(str::to_owned);
    }
    (!bindings::PACKAGES.iter().any(|p| belongs_to(spec, p))).then(|| spec.to_owned())
}

pub fn belongs_to(spec: &str, package: &str) -> bool {
    spec.strip_prefix(package)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

pub fn specifier(spec: &str) -> Option<String> {
    route(spec, false)
}
pub fn rt_specifier(spec: &str) -> Option<String> {
    route(spec, true)
}

pub fn on_kernel(spec: &str, export: &str) -> bool {
    !belongs_to(spec, bindings::OWNED_PREFIX)
        || bindings::EXPORTS
            .lines()
            .any(|line| line.split_once(' ') == Some((spec, export)))
}

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
    bindings::FAMILIES
        .iter()
        .any(|line| line.split_once(' ') == Some((spec, export)))
}

pub fn selected_on_kernel(spec: &str, export: &str, bridge: EffectBridge) -> bool {
    if bridge == EffectBridge::AvailableWithUi && ui_family(spec, export) {
        return false;
    }
    let root = Some(bindings::NATIVE_ENTRY);
    let stub = Some(spec) == root && bindings::STUBS.lines().any(|name| name == export);
    let module_stub = bindings::BRIDGE_STUBS
        .lines()
        .any(|line| line.split_once(' ') == Some((spec, export)));
    !(bridge.available() && (stub || module_stub)) && on_kernel(spec, export)
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
    bindings::CORE_BUILTINS.contains(&name)
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
    use super::{bindings, effect_specifier, on_kernel, rt_specifier, specifier};
    use crate::types::builtin_sources;

    #[test]
    fn builtins_without_effects_bind_only_core_and_import_each_other() {
        for (name, source) in builtin_sources()
            .iter()
            .filter(|(n, _)| bindings::CORE_BUILTINS.contains(n))
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
                    assert!(
                        bindings::CORE_BUILTINS.contains(&module),
                        "{name} imports `{module}`"
                    );
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
