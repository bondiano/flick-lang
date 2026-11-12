//! Where a foreign module of the built-in packages runs: Flick's own runtime
//! or the Effect adapter, by the key that names it.

use std::borrow::Cow;

use crate::codegen::EffectBridge;
use crate::diagnostics::{Diagnostic, codes};
use crate::ir::{self, ItemKind};
use crate::types::OpClass;

/// The tables of the built-in modules' bindings, which the build script writes.
pub(crate) mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

/// The key of a module the built-in packages bind, `flick:Trie`, which names
/// no package of a target. `spec` may spell it as a module of the adapter's
/// package or of the runtime's, as earlier sources do.
pub fn key(spec: &str) -> Cow<'_, str> {
    let adapter = bindings::ADAPTER_PREFIX.len();
    bindings::PACKAGES
        .iter()
        .find_map(|package| spec.strip_prefix(package))
        .and_then(|rest| {
            bindings::ALIASES.iter().find_map(|(key, module, prefix)| {
                rest.strip_prefix(&module[adapter..])
                    .filter(|tail| tail.is_empty() || *prefix)
                    .map(|tail| format!("{key}{tail}"))
            })
        })
        .map_or(Cow::Borrowed(spec), Cow::Owned)
}

/// Whether `spec` is a module of the built-in packages' own: a key, or a
/// module of the runtime's or the adapter's package.
pub fn owned(spec: &str) -> bool {
    spec.starts_with(bindings::KEY) || bindings::PACKAGES.iter().any(|p| belongs_to(spec, p))
}

/// The module of the adapter `spec` stands for; a module of another package
/// is itself.
pub fn effect_specifier(spec: &str) -> String {
    let spec = key(spec);
    bindings::ALIASES
        .iter()
        .find_map(|(from, to, prefix)| {
            spec.strip_prefix(from)
                .filter(|rest| rest.is_empty() || *prefix)
                .map(|rest| format!("{to}{rest}"))
        })
        .unwrap_or_else(|| spec.into_owned())
}

pub fn belongs_to(spec: &str, package: &str) -> bool {
    spec.strip_prefix(package)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// The module of Flick's runtime `spec` stands for, a module of another
/// package itself, or none when only the adapter has it.
pub fn rt_specifier(spec: &str) -> Option<String> {
    let spec = key(spec);
    if let Some((_, rt)) = bindings::ROUTES.iter().find(|(from, _)| *from == spec) {
        return rt.map(str::to_owned);
    }
    (!owned(&spec)).then(|| spec.into_owned())
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

/// Whether `source` names a module of the RPC family: its procedures, groups
/// and calls are one side's values, Flick's runtime's or the adapter's.
pub fn rpc_family(source: &str) -> bool {
    let source = key(source);
    bindings::RPC_FAMILY.contains(&source.as_ref())
}

pub fn selected_on_kernel(spec: &str, export: &str, bridge: EffectBridge) -> bool {
    if bridge.ui() && ui_family(spec, export) {
        return false;
    }
    if bridge.rpc()
        && bindings::RPC_FAMILY
            .iter()
            .any(|source| rt_specifier(source).as_deref() == Some(spec))
    {
        return false;
    }
    let root = Some(bindings::NATIVE_ENTRY);
    let stub = Some(spec) == root && bindings::STUBS.lines().any(|name| name == export);
    let module_stub = bindings::BRIDGE_STUBS
        .lines()
        .any(|line| line.split_once(' ') == Some((spec, export)));
    !(bridge.available() && (stub || module_stub)) && on_kernel(spec, export)
}

/// A native export that is a placeholder, rather than an implementation.
/// Bridge policy also names working native exports, so it cannot answer this.
pub(crate) fn unavailable_on_kernel(spec: &str, export: &str) -> bool {
    (spec == bindings::NATIVE_ENTRY && bindings::STUBS.lines().any(|name| name == export))
        || bindings::UNAVAILABLE
            .lines()
            .any(|line| line.split_once(' ') == Some((spec, export)))
}

/// Whether a routed binding needs the adapter rather than a native implementation.
pub(crate) fn requires_adapter(spec: &str, export: &str, effect: bool) -> bool {
    if belongs_to(spec, bindings::OWNED_PREFIX) {
        !on_kernel(spec, export) || unavailable_on_kernel(spec, export)
    } else {
        belongs_to(spec, bindings::ADAPTER_PREFIX) || effect
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
    use super::{bindings, effect_specifier, key, on_kernel, owned, rt_specifier};

    #[test]
    fn a_key_has_two_earlier_spellings_and_names_the_module_of_each_target() {
        for (spelled, is) in [
            ("@flick-lang/effect-ts/effect/Trie", "flick:Trie"),
            ("@flick-lang/runtime/effect/http/Url", "flick:http/Url"),
            ("@flick-lang/effect-ts", "flick:rt"),
            ("@flick-lang/runtime", "flick:rt"),
            ("@flick-lang/effect-ts/ui", "flick:ui"),
            ("@flick-lang/runtime/ui/react", "flick:ui/react"),
            ("@flick-lang/effect-ts/runner/node", "flick:runner/node"),
            ("@flick-lang/effect-ts/sql", "flick:sql"),
            ("@flick-lang/effect-ts/cli", "flick:cli"),
            // the modules of a target are no keys, nor are those of other packages
            ("@flick-lang/runtime/rt", "@flick-lang/runtime/rt"),
            (
                "@flick-lang/runtime/rt/effect/Trie",
                "@flick-lang/runtime/rt/effect/Trie",
            ),
            (
                "@flick-lang/runtime/core/Option",
                "@flick-lang/runtime/core/Option",
            ),
            (
                "@flick-lang/effect-ts/bridge",
                "@flick-lang/effect-ts/bridge",
            ),
            ("@flick-lang/effect-ts-extra", "@flick-lang/effect-ts-extra"),
            ("./clock.mjs", "./clock.mjs"),
        ] {
            assert_eq!(key(spelled), is);
            assert_eq!(key(is), is);
        }
        for (key, adapter, rt) in [
            (
                "flick:Option",
                "@flick-lang/effect-ts/effect/Option",
                Some("@flick-lang/runtime/rt/effect/Option"),
            ),
            (
                "flick:rt",
                "@flick-lang/effect-ts",
                Some("@flick-lang/runtime/rt"),
            ),
            (
                "flick:ui/react",
                "@flick-lang/effect-ts/ui/react",
                Some("@flick-lang/runtime/rt/ui/react"),
            ),
            ("flick:Stream", "@flick-lang/effect-ts/effect/Stream", None),
            ("flick:cli", "@flick-lang/effect-ts/cli", None),
        ] {
            assert_eq!(effect_specifier(key), adapter);
            assert_eq!(rt_specifier(key).as_deref(), rt, "{key}");
            assert!(owned(key));
        }
        assert!(!owned("./clock.mjs") && !owned("@flick-lang/runtime-extra"));
        for (key, _) in bindings::ROUTES {
            assert!(key.starts_with(bindings::KEY), "{key} is routed by its key");
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
    fn an_adapter_owning_rpc_takes_the_native_client_too() {
        use crate::codegen::EffectBridge;
        let spec = super::rt_specifier("flick:rpc/Rpc").expect("routed to the runtime");
        assert!(super::selected_on_kernel(
            &spec,
            "rpcCallNdjson",
            EffectBridge::AvailableWithUi
        ));
        for bridge in [
            EffectBridge::AvailableWithRpc,
            EffectBridge::AvailableWithUiAndRpc,
        ] {
            assert!(!super::selected_on_kernel(&spec, "rpcCallNdjson", bridge));
        }
        assert!(super::rpc_family("flick:rpc/RpcNode"));
        assert!(!super::rpc_family("flick:ui"));
    }
}
