//! Type and IR inspection helpers for REPL phrases and session commands
//! (`:handle`, `:provide`, `:bg`).

use crate::ir::{Expr, ExprKind, ItemKind, Module};
use crate::types::lang::{self, Lang};
use crate::types::{Type, show_type};

/// Calling convention of an installed `:handle` session handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerStyle {
    /// Expects a `Unit ->{E | r; e} a` (or `Unit -> a`) thunk closure.
    Thunk { returns_task: bool },
    /// Expects an `Eff {E | r} e a` task value directly.
    Task { returns_task: bool },
}

/// Extracted metadata for an `Eff r e a` task type.
#[derive(Debug, Clone)]
pub(super) struct TaskInfo {
    pub(super) ok_type: Type,
    pub(super) err_type: Option<Type>,
    pub(super) effects: Vec<String>,
}

/// Strips outer `forall` and capture qualifiers to inspect the head type.
pub(super) fn strip_wrappers(mut ty: &Type) -> &Type {
    loop {
        match ty {
            Type::Forall { body, .. }
            | Type::Captured { ty: body, .. }
            | Type::Bounded { ty: body, .. } => ty = body,
            _ => return ty,
        }
    }
}

/// Collects concrete named row entries from a service or error row.
pub(super) fn collect_row_names(ty: &Type, out: &mut Vec<String>) {
    match strip_wrappers(ty) {
        Type::Row(row) => {
            for (elem, _) in &row.entries {
                if let Some(name) = elem.name()
                    && !out.iter().any(|existing| existing == name)
                {
                    out.push(name.to_owned());
                }
            }
            if let Some(tail) = &row.tail {
                collect_row_names(tail, out);
            }
        }
        Type::Union(a, b) => {
            collect_row_names(a, out);
            collect_row_names(b, out);
        }
        Type::Without { row, gone } => {
            let mut inner = Vec::new();
            collect_row_names(row, &mut inner);
            let gone_names: Vec<&str> = gone.iter().filter_map(|e| e.name()).collect();
            for name in inner {
                if !gone_names.iter().any(|g| *g == name)
                    && !out.iter().any(|existing| existing == &name)
                {
                    out.push(name);
                }
            }
        }
        _ => {}
    }
}

/// Collects names removed by `Without` (`{-E | r}`) in a row type.
fn collect_without_gone(ty: &Type, out: &mut Vec<String>) {
    match strip_wrappers(ty) {
        Type::Row(row) => {
            if let Some(tail) = &row.tail {
                collect_without_gone(tail, out);
            }
        }
        Type::Union(a, b) => {
            collect_without_gone(a, out);
            collect_without_gone(b, out);
        }
        Type::Without { row, gone } => {
            for elem in gone {
                if let Some(name) = elem.name()
                    && !out.iter().any(|existing| existing == name)
                {
                    out.push(name.to_owned());
                }
            }
            collect_without_gone(row, out);
        }
        _ => {}
    }
}

fn is_ambient_effect(name: &str) -> bool {
    if lang::is_host_service(name) {
        return true;
    }
    [Lang::Async, Lang::Scope, Lang::Fail]
        .into_iter()
        .filter_map(lang::label)
        .any(|label| label == name)
}

fn effect_covered(name: &str, covered: &[String]) -> bool {
    let short = name.rsplit('.').next().unwrap_or(name);
    covered.iter().any(|entry| {
        entry == name
            || (entry.rsplit('.').next().unwrap_or(entry) == short
                && (entry == short || !entry.contains('.')))
    })
}

/// Inspects an `Eff r e a` type and computes its unresolved user effects.
pub(super) fn inspect_task(ty: &Type, handled: &[String], provided: &[String]) -> Option<TaskInfo> {
    let (head, args) = strip_wrappers(ty).spine();
    if !head.is_lang(Lang::Eff) || args.len() != 3 {
        return None;
    }
    let services = args[0];
    let errors = args[1];
    let ok = args[2];
    let mut raw_effects = Vec::new();
    collect_row_names(services, &mut raw_effects);
    let effects = raw_effects
        .into_iter()
        .filter(|name| {
            !is_ambient_effect(name)
                && !effect_covered(name, handled)
                && !effect_covered(name, provided)
        })
        .collect();
    let mut err_names = Vec::new();
    collect_row_names(errors, &mut err_names);
    let err_type = (!err_names.is_empty()).then(|| errors.clone());
    Some(TaskInfo {
        ok_type: ok.clone(),
        err_type,
        effects,
    })
}

/// Inspects a `:handle` expression's type and lowered IR to determine its
/// calling convention and the effect(s) it handles.
pub(super) fn inspect_handler(
    ty: &Type,
    module_ir: Option<&Module>,
    binding: &str,
) -> Option<(HandlerStyle, Vec<String>)> {
    let Type::Fun {
        param,
        effects: out_eff,
        result,
    } = strip_wrappers(ty)
    else {
        return None;
    };
    let (is_thunk, in_services) = match strip_wrappers(param) {
        Type::Fun {
            param: inner_param,
            effects: in_eff,
            ..
        } if strip_wrappers(inner_param).is_lang(Lang::Unit) => {
            (true, in_eff.as_ref().map(|e| &e.services))
        }
        other => {
            let (head, args) = other.spine();
            if head.is_lang(Lang::Eff) && args.len() == 3 {
                (false, Some(args[0]))
            } else {
                return None;
            }
        }
    };
    let (res_head, res_args) = strip_wrappers(result).spine();
    let returns_task = res_head.is_lang(Lang::Eff) && res_args.len() == 3;
    let style = if is_thunk {
        HandlerStyle::Thunk { returns_task }
    } else {
        HandlerStyle::Task { returns_task }
    };
    let mut in_names = Vec::new();
    if let Some(services) = in_services {
        collect_row_names(services, &mut in_names);
    }
    let mut out_names = Vec::new();
    let mut gone_names = Vec::new();
    if let Some(eff) = out_eff {
        collect_row_names(&eff.services, &mut out_names);
        collect_without_gone(&eff.services, &mut gone_names);
    }
    if returns_task {
        collect_row_names(res_args[0], &mut out_names);
        collect_without_gone(res_args[0], &mut gone_names);
    }
    let mut handled = Vec::new();
    for name in in_names {
        if !out_names.iter().any(|out| out == &name) && !handled.iter().any(|h| h == &name) {
            handled.push(name);
        }
    }
    for name in gone_names {
        if !handled.iter().any(|h| h == &name) {
            handled.push(name);
        }
    }
    // Unannotated lambdas like `fun body -> try body () with | Ask.ask () -> 42`
    // infer `body : Unit -> a` (pure), while the lowered IR still holds the
    // `ExprKind::Handle` with its canonical effect names.
    if let Some(ir) = module_ir {
        for item in &ir.items {
            if let ItemKind::Value { name, body, .. } = &item.kind
                && name == binding
            {
                collect_ir_handled_effects(body, &mut handled);
            }
        }
    }
    if handled.is_empty() && in_services.is_none() {
        return None;
    }
    Some((style, handled))
}

fn collect_ir_handled_effects(expr: &Expr, out: &mut Vec<String>) {
    if let ExprKind::Handle(handler) = &expr.kind {
        for eff in &handler.effects {
            if !out.iter().any(|h| h == &eff.effect) {
                out.push(eff.effect.clone());
            }
        }
    }
    expr.each_child(&mut |child| collect_ir_handled_effects(child, out));
}

/// Inspects a `:provide` expression's `Layer ri e ro` type, returning its provided services.
pub(super) fn inspect_layer(ty: &Type) -> Option<Vec<String>> {
    let (head, args) = strip_wrappers(ty).spine();
    if !head.is_lang(Lang::Layer) || args.len() != 3 {
        return None;
    }
    let mut provided = Vec::new();
    collect_row_names(args[2], &mut provided);
    Some(provided)
}

/// Formats a type for user-facing error messages.
pub(super) fn format_type(ty: &Type) -> String {
    show_type(ty)
}
