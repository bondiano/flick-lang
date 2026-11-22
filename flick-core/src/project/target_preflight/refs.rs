//! IR reference, eager-initializer, and module re-export collectors for
//! environment preflight reachability.

use super::gated::handler_mask;
use super::locals::{reachable_block_stmts, stmt_expr};
use crate::iface::Namespace;
use crate::ir::{Expr, ExprKind, Handler, Item, ItemKind, Pat, Stmt, reach};
use crate::project::Compiled;
use crate::resolve::{self, ItemKind as ExportKind};
use crate::source::{SourceMap, Span};
use crate::syntax;

pub(super) enum Reference {
    Value {
        key: reach::Key,
        span: Option<Span>,
        covered: u8,
    },
    ModuleOnly {
        module: String,
        symbol: String,
        span: Option<Span>,
    },
}

pub(super) fn collect_item_references(
    item: &Item,
    is_user_module: bool,
    fallback: Option<Span>,
    covered: u8,
    out: &mut Vec<Reference>,
) {
    let span_for = |span: Span| if is_user_module { Some(span) } else { fallback };
    item.each_body(&mut |expr| collect_expr_references(expr, &span_for, covered, out));
    if let ItemKind::Destructure { pattern, .. } = &item.kind {
        collect_pat_references(pattern, span_for(item.span), out);
    }
    if let ItemKind::Instance { info, .. } = &item.kind {
        let at = span_for(item.span);
        let derived = info.derived.iter().flat_map(|(fields, _)| fields);
        let extra = info.defaults.iter().flat_map(|(_, _, extra)| extra);
        let mut dict_keys = Vec::new();
        for d in (info.supers.iter().map(|(_, d)| d))
            .chain(derived)
            .chain(extra)
        {
            reach::dict_uses(d, &mut dict_keys);
        }
        let (module, class) = &info.class;
        for (name, _, _) in &info.defaults {
            dict_keys.push((module.clone(), flick_ir::default_method(class, name)));
        }
        for key in dict_keys {
            out.push(Reference::Value {
                key,
                span: at,
                covered,
            });
        }
        if !module.is_empty() {
            out.push(Reference::ModuleOnly {
                module: module.clone(),
                symbol: format!("{module}.{class}"),
                span: at,
            });
        }
    }
}

pub(super) fn collect_expr_references(
    expr: &Expr,
    span_for: &impl Fn(Span) -> Option<Span>,
    covered: u8,
    out: &mut Vec<Reference>,
) {
    let at = span_for(expr.span);
    match &expr.kind {
        ExprKind::Global { module, name } => {
            out.push(Reference::Value {
                key: (module.clone(), name.clone()),
                span: at,
                covered,
            });
        }
        ExprKind::Method { dict, .. } => {
            let mut keys = Vec::new();
            reach::dict_uses(dict, &mut keys);
            for key in keys {
                out.push(Reference::Value {
                    key,
                    span: at,
                    covered,
                });
            }
        }
        ExprKind::Dicts(_, dicts) => {
            let mut keys = Vec::new();
            for dict in dicts {
                reach::dict_uses(dict, &mut keys);
            }
            for key in keys {
                out.push(Reference::Value {
                    key,
                    span: at,
                    covered,
                });
            }
        }
        ExprKind::Con(con) if !con.module.is_empty() => {
            out.push(Reference::ModuleOnly {
                module: con.module.clone(),
                symbol: format!("{}.{}", con.module, con.name),
                span: at,
            });
        }
        ExprKind::Perform { effect, op, .. } => {
            if let Some((module, service)) = split_service(effect) {
                out.push(Reference::ModuleOnly {
                    module: module.to_owned(),
                    symbol: format!("{module}.{service}.{op}"),
                    span: at,
                });
            }
        }
        ExprKind::ServiceTag(service) => {
            if let Some((module, name)) = split_service(service) {
                out.push(Reference::ModuleOnly {
                    module: module.to_owned(),
                    symbol: format!("{module}.{name}"),
                    span: at,
                });
            }
        }
        ExprKind::Handle(handler) => {
            collect_handle_references(handler, at, span_for, covered, out);
            return;
        }
        ExprKind::Match(_, arms) => {
            for arm in arms {
                collect_pat_references(&arm.pat, at, out);
            }
        }
        ExprKind::Lambda { params, .. } => {
            for p in params {
                collect_pat_references(&p.pat, at, out);
            }
        }
        ExprKind::Block(stmts, result) => {
            let active = reachable_block_stmts(stmts, result);
            for (stmt, is_active) in stmts.iter().zip(active) {
                if !is_active {
                    continue;
                }
                if let Stmt::Let(pat, _) = stmt {
                    collect_pat_references(pat, at, out);
                }
                collect_expr_references(stmt_expr(stmt), span_for, covered, out);
            }
            collect_expr_references(result, span_for, covered, out);
            return;
        }
        ExprKind::Each { pat, .. } => {
            collect_pat_references(pat, at, out);
        }
        _ => {}
    }
    expr.each_child(&mut |child| collect_expr_references(child, span_for, covered, out));
}

fn collect_handle_references(
    handler: &Handler,
    at: Option<Span>,
    span_for: &impl Fn(Span) -> Option<Span>,
    covered: u8,
    out: &mut Vec<Reference>,
) {
    for handled in &handler.effects {
        if let Some((module, name)) = split_service(&handled.effect) {
            out.push(Reference::ModuleOnly {
                module: module.to_owned(),
                symbol: format!("{module}.{name}"),
                span: at,
            });
        }
        for arm in &handled.operations {
            collect_expr_references(&arm.function, span_for, covered, out);
        }
    }
    let nested = covered | handler_mask(handler);
    collect_expr_references(&handler.body, span_for, nested, out);
    if let Some(ret) = &handler.ret {
        collect_expr_references(ret, span_for, covered, out);
    }
    for arm in &handler.errors {
        collect_pat_references(&arm.pat, at, out);
        if let Some(guard) = &arm.guard {
            collect_expr_references(guard, span_for, covered, out);
        }
        collect_expr_references(&arm.body, span_for, covered, out);
    }
}

fn collect_pat_references(pat: &Pat, span: Option<Span>, out: &mut Vec<Reference>) {
    match pat {
        Pat::Con(con, sub) => {
            if !con.module.is_empty() {
                out.push(Reference::ModuleOnly {
                    module: con.module.clone(),
                    symbol: format!("{}.{}", con.module, con.name),
                    span,
                });
            }
            for p in sub {
                collect_pat_references(p, span, out);
            }
        }
        Pat::Tuple(items) | Pat::Or(items) => {
            for p in items {
                collect_pat_references(p, span, out);
            }
        }
        Pat::List(items, rest) => {
            for p in items.iter().chain(rest.as_deref()) {
                collect_pat_references(p, span, out);
            }
        }
        Pat::Record(fields) => {
            for (_, p) in fields {
                collect_pat_references(p, span, out);
            }
        }
        Pat::Wild | Pat::Bind(_) | Pat::Lit(_) => {}
    }
}

pub(super) fn initialization_refs(item: &Item, out: &mut Vec<Reference>) {
    fn visit(expr: &Expr, out: &mut Vec<Reference>) {
        if matches!(
            expr.kind,
            ExprKind::Lambda { .. } | ExprKind::Task(_) | ExprKind::Abstract(..)
        ) {
            return;
        }
        let span_for = |s: Span| Some(s);
        match &expr.kind {
            ExprKind::Global { module, name } => out.push(Reference::Value {
                key: (module.clone(), name.clone()),
                span: Some(expr.span),
                covered: 0,
            }),
            ExprKind::Method { dict, .. } => {
                let mut keys = Vec::new();
                reach::dict_uses(dict, &mut keys);
                for key in keys {
                    out.push(Reference::Value {
                        key,
                        span: Some(expr.span),
                        covered: 0,
                    });
                }
            }
            ExprKind::Dicts(_, dicts) => {
                let mut keys = Vec::new();
                for dict in dicts {
                    reach::dict_uses(dict, &mut keys);
                }
                for key in keys {
                    out.push(Reference::Value {
                        key,
                        span: Some(expr.span),
                        covered: 0,
                    });
                }
            }
            ExprKind::Con(con) if !con.module.is_empty() => out.push(Reference::ModuleOnly {
                module: con.module.clone(),
                symbol: format!("{}.{}", con.module, con.name),
                span: Some(expr.span),
            }),
            ExprKind::Perform { effect, op, .. } => {
                if let Some((module, service)) = split_service(effect) {
                    out.push(Reference::ModuleOnly {
                        module: module.to_owned(),
                        symbol: format!("{module}.{service}.{op}"),
                        span: Some(expr.span),
                    });
                }
            }
            ExprKind::ServiceTag(service) => {
                if let Some((module, name)) = split_service(service) {
                    out.push(Reference::ModuleOnly {
                        module: module.to_owned(),
                        symbol: format!("{module}.{name}"),
                        span: Some(expr.span),
                    });
                }
            }
            ExprKind::Handle(handler) => {
                for handled in &handler.effects {
                    if let Some((module, name)) = split_service(&handled.effect) {
                        out.push(Reference::ModuleOnly {
                            module: module.to_owned(),
                            symbol: format!("{module}.{name}"),
                            span: Some(expr.span),
                        });
                    }
                }
            }
            _ => {}
        }
        let _ = span_for;
        expr.each_child(&mut |child| visit(child, out));
    }
    match &item.kind {
        ItemKind::Value { dicts, body, .. }
            if dicts.is_empty() && !matches!(body.kind, ExprKind::Global { .. }) =>
        {
            visit(body, out);
        }
        ItemKind::Destructure { body, .. } => visit(body, out),
        ItemKind::Instance { info, methods } if info.context.is_empty() => {
            for (_, body) in methods {
                if !matches!(body.kind, ExprKind::Global { .. }) {
                    visit(body, out);
                }
            }
        }
        _ => {}
    }
}

fn split_service(key: &str) -> Option<(&str, &str)> {
    let (module, name) = key.rsplit_once('.')?;
    (!module.is_empty() && !name.is_empty()).then_some((module, name))
}

pub(super) fn module_reexports(
    module: &Compiled,
    sources: &SourceMap,
) -> Vec<(reach::Key, Vec<String>, Span)> {
    let reexports: Vec<_> = module
        .interface
        .exports
        .iter()
        .filter(|export| {
            export.entity.namespace == Namespace::Value && export.entity.module != module.name
        })
        .collect();
    if reexports.is_empty() {
        return Vec::new();
    }
    let text = &sources.get(module.file).text;
    let parsed = syntax::parse(text, module.file);
    let syntax = resolve::module_syntax(&parsed.tree, text, module.file);
    let mut out = Vec::new();
    for export in reexports {
        let Some(header) = &syntax.header else {
            continue;
        };
        let span = header
            .exposing
            .as_ref()
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item.name == export.entity.name)
                    .or_else(|| items.iter().find(|item| item.kind == ExportKind::All))
            })
            .map_or(header.span, |item| item.span);
        out.push((
            (export.entity.module.clone(), export.entity.name.clone()),
            export.via.clone(),
            span,
        ));
    }
    out
}

pub(super) fn reexport_span(module: &Compiled, name: &str, sources: &SourceMap) -> Option<Span> {
    let text = &sources.get(module.file).text;
    let parsed = syntax::parse(text, module.file);
    let syntax = resolve::module_syntax(&parsed.tree, text, module.file);
    let header = syntax.header.as_ref()?;
    Some(
        header
            .exposing
            .as_ref()
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item.name == name)
                    .or_else(|| items.iter().find(|item| item.kind == ExportKind::All))
            })
            .map_or(header.span, |item| item.span),
    )
}
