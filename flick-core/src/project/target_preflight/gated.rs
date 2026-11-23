//! Gated service root-handler tables (`FLICK-E020`) and Effect-adapter
//! foreign checks for environment preflight.

use std::collections::HashSet;

use super::HandlesService;
use super::locals::reachable_block_stmts;
use crate::diagnostics::{Diagnostic, codes};
use crate::effects::{self, bindings};
use crate::ir::{Expr, ExprKind, Foreign, ForeignMode, Handler, Item, Stmt};
use crate::project::config::BackendEnvironment;
use crate::project::environment::ResolvedEnvironment;
use crate::source::Span;

/// Services that need an explicit environment root handler.
const GATED: &[&str] = &["Database", "Db", "Server"];

/// Bit flag for a gated service (`Database`, `Db`, `Server`), or `0`.
pub(super) fn gated_bit(effect: &str) -> u8 {
    match gated_service(effect) {
        Some("Database") => 1 << 0,
        Some("Db") => 1 << 1,
        Some("Server") => 1 << 2,
        _ => 0,
    }
}

/// Bitmask of gated services handled by `handler`.
pub(super) fn handler_mask(handler: &Handler) -> u8 {
    handler
        .effects
        .iter()
        .fold(0, |mask, handled| mask | gated_bit(&handled.effect))
}

/// Bitmask of gated services natively handled by `env` (or declared by the
/// selected backend's descriptor via `handles_service`).
pub(super) fn env_handled_mask(
    env: &ResolvedEnvironment,
    handles_service: Option<HandlesService<'_>>,
) -> u8 {
    GATED.iter().fold(0, |mask, name| {
        let handled = match handles_service.and_then(|f| f(name)) {
            Some(true) => true,
            Some(false) => {
                env.host_package.is_some()
                    && (env.tags.contains("node") || env.tags.contains("native"))
            }
            None => handles_resolved(env, name),
        };
        if handled {
            mask | gated_bit(name)
        } else {
            mask
        }
    })
}

/// Whether `effect` (`Module.Service`) is gated and, if so, its short name.
pub fn gated_service(effect: &str) -> Option<&str> {
    let name = effect.rsplit('.').next()?;
    GATED.iter().copied().find(|&gated| gated == name)
}

/// Short names of gated services that have a root handler on `environment`.
pub fn handled_services(environment: &BackendEnvironment) -> &'static [&'static str] {
    match (environment.backend.as_str(), environment.platform.as_str()) {
        ("js", "node") | ("go", "native") => GATED,
        _ => &[],
    }
}

/// Whether `environment` has a root handler for the gated service `name`.
pub fn handles(environment: &BackendEnvironment, name: &str) -> bool {
    handled_services(environment).contains(&name)
}

fn handles_resolved(env: &ResolvedEnvironment, name: &str) -> bool {
    handles(&env.pair(), name) || env.tags.contains("node") || env.tags.contains("native")
}

/// Std root-handler symbols corresponding to the handled service names.
/// `Db` is gated in source checks but has no fixed std handler symbol.
pub fn std_root_handlers_for<'a>(
    services: impl IntoIterator<Item = &'a str>,
) -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    let mut out: Vec<(&'static str, &'static str, &'static str, &'static str)> = Vec::new();
    for service in services {
        match service {
            "Database" if !out.iter().any(|(s, ..)| *s == "Database") => {
                out.push(("Database", "Database", "Database", "pg"));
            }
            "Server" if !out.iter().any(|(s, ..)| *s == "Server") => {
                out.push(("Server", "Server", "Server", "handler"));
            }
            _ => {}
        }
    }
    out
}

/// Std root-handler symbols declared for an environment that hosts them.
/// `Db` is gated in source checks but has no fixed std handler symbol.
pub fn std_root_handlers(
    environment: &BackendEnvironment,
) -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    std_root_handlers_for(handled_services(environment).iter().copied())
}

/// Whether only the Effect adapter implements `foreign`, with the UI renderer `ui`.
pub(crate) fn needs_adapter(foreign: &Foreign, ui: Option<&str>) -> bool {
    let Some(from) = foreign.binding.from.as_deref() else {
        return false;
    };
    let from = ui
        .filter(|_| bindings::UI_RENDERER.contains(&from))
        .unwrap_or(from);
    let native = effects::rt_specifier(from).unwrap_or_else(|| effects::effect_specifier(from));
    effects::requires_adapter(
        &native,
        &foreign.binding.name,
        foreign.mode == ForeignMode::Effect,
    )
}

pub(super) fn scan_item_gated_services(
    item: &Item,
    env: &ResolvedEnvironment,
    handles_service: Option<HandlesService<'_>>,
    covered: u8,
    reported: &mut HashSet<(String, Span)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let initial = covered | env_handled_mask(env, handles_service);
    item.each_body(&mut |expr| {
        scan_expr_gated_services(expr, env, initial, reported, diagnostics);
    });
}

fn scan_expr_gated_services(
    expr: &Expr,
    env: &ResolvedEnvironment,
    covered: u8,
    reported: &mut HashSet<(String, Span)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match &expr.kind {
        ExprKind::Perform { effect, .. } => {
            let bit = gated_bit(effect);
            if bit != 0
                && (covered & bit) == 0
                && let Some(name) = gated_service(effect)
            {
                let key = (name.to_owned(), expr.span);
                if reported.insert(key) {
                    diagnostics.push(Diagnostic::error(
                        codes::E020,
                        format!(
                            "unhandled effect {name} on {}/{}",
                            env.backend, env.platform
                        ),
                        expr.span,
                    ));
                }
            }
            expr.each_child(&mut |child| {
                scan_expr_gated_services(child, env, covered, reported, diagnostics);
            });
        }
        ExprKind::Handle(handler) => {
            let nested = covered | handler_mask(handler);
            for handled in &handler.effects {
                for arm in &handled.operations {
                    scan_expr_gated_services(&arm.function, env, covered, reported, diagnostics);
                }
            }
            scan_expr_gated_services(&handler.body, env, nested, reported, diagnostics);
            if let Some(ret) = &handler.ret {
                scan_expr_gated_services(ret, env, covered, reported, diagnostics);
            }
            for arm in &handler.errors {
                if let Some(guard) = &arm.guard {
                    scan_expr_gated_services(guard, env, covered, reported, diagnostics);
                }
                scan_expr_gated_services(&arm.body, env, covered, reported, diagnostics);
            }
        }
        ExprKind::Block(stmts, result) => {
            let active = reachable_block_stmts(stmts, result);
            for (stmt, is_active) in stmts.iter().zip(active) {
                if !is_active {
                    continue;
                }
                let child = match stmt {
                    Stmt::Let { value: v, .. }
                    | Stmt::Var { value: v, .. }
                    | Stmt::Assign(_, v)
                    | Stmt::Expr(v)
                    | Stmt::Fun { value: v, .. } => v,
                };
                scan_expr_gated_services(child, env, covered, reported, diagnostics);
            }
            scan_expr_gated_services(result, env, covered, reported, diagnostics);
        }
        _ => {
            expr.each_child(&mut |child| {
                scan_expr_gated_services(child, env, covered, reported, diagnostics);
            });
        }
    }
}
