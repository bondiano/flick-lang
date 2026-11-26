use flick_ir::{Adapter, Dict, Foreign, ForeignMode, Retention};

use super::{
    Env, Expr, ExprKind, HostPrimitive, Intrinsic, Lit, Local, LowerError, Param, Pat, Span, Stage,
    given, unsupported,
};

pub(super) fn foreign_intrinsic(value: &Foreign) -> Option<Intrinsic> {
    use Intrinsic::Host;
    let (from, name) = value.binding.module_export()?;
    match (from, name, value.mode) {
        ("flick:rt", "stdoutLine", ForeignMode::Sync) => Some(Intrinsic::PrintLine),
        ("flick:rt", "stderrLine", ForeignMode::Sync) => Some(Host(HostPrimitive::StderrLine)),
        ("flick:rt", "stdoutWrite", ForeignMode::Sync) => Some(Host(HostPrimitive::StdoutWrite)),
        ("flick:rt", "platformEnv", ForeignMode::Sync) => Some(Host(HostPrimitive::Env)),
        ("flick:rt", "platformVars", ForeignMode::Sync) => Some(Host(HostPrimitive::Vars)),
        ("flick:rt", "platformArgs", ForeignMode::Sync) => Some(Host(HostPrimitive::Args)),
        ("flick:Clock", "flickCurrentTimeMillis", ForeignMode::Effect) => {
            Some(Host(HostPrimitive::CurrentTimeMillis))
        }
        ("flick:Clock", "flickCurrentTimeNanos", ForeignMode::Effect) => {
            Some(Host(HostPrimitive::CurrentTimeNanos))
        }
        ("flick:Clock", "flickMonotonicTimeNanos", ForeignMode::Effect) => {
            Some(Host(HostPrimitive::MonotonicTimeNanos))
        }
        ("flick:Random", "flickNext", ForeignMode::Effect) => Some(Host(HostPrimitive::RandomNext)),
        ("flick:rt", "processSpawn", ForeignMode::Effect) => {
            Some(Host(HostPrimitive::ProcessSpawn))
        }
        ("flick:rt", "show", ForeignMode::Pure) => Some(Intrinsic::Show),
        ("flick:rt", "refMake", ForeignMode::Sync) => Some(Intrinsic::RefMake),
        ("flick:rt", "refGet", ForeignMode::Sync) => Some(Intrinsic::RefGet),
        ("flick:rt", "refSet", ForeignMode::Sync) => Some(Intrinsic::RefSet),
        ("flick:rt", "refModify", ForeignMode::Sync) => Some(Intrinsic::RefModify),
        ("flick:rt", "refSame", ForeignMode::Pure) => Some(Intrinsic::RefSame),
        ("flick:rt", "scopeRun", ForeignMode::Effect) => Some(Intrinsic::ScopeRun),
        ("flick:rt", "scopeCurrent", ForeignMode::Effect) => Some(Intrinsic::ScopeCurrent),
        ("flick:rt", "arenaMake", ForeignMode::Effect) => Some(Intrinsic::ArenaMake),
        ("flick:rt", "arenaAlloc", ForeignMode::Sync) => Some(Intrinsic::ArenaAlloc),
        ("flick:rt", "arenaGet", ForeignMode::Sync) => Some(Intrinsic::ArenaGet),
        _ => None,
    }
}

pub(super) fn lookup(env: &Env, name: &str, span: Span) -> Result<Local, LowerError> {
    env.get(name)
        .copied()
        .ok_or_else(|| LowerError::UnboundLocal {
            name: name.into(),
            span,
        })
}

pub(super) fn abstract_params(names: &[String]) -> Vec<Param> {
    names
        .iter()
        .map(|name| Param {
            pat: Pat::Bind(given(name)),
            stage: Stage::Pure,
            ty: None,
            binder_types: Vec::new(),
        })
        .collect()
}

pub(super) fn dictionary(value: &Dict, span: Span) -> Result<Expr, LowerError> {
    let kind = match value {
        Dict::Given(name) => ExprKind::Local(given(name)),
        Dict::Field(name) => ExprKind::Local(name.clone()),
        Dict::Opened { binder, field } => ExprKind::Field(
            Box::new(Expr::new(ExprKind::Local(binder.clone()), span)),
            field.clone(),
        ),
        Dict::Value { module, name } => ExprKind::Global {
            module: module.clone(),
            name: name.clone(),
            type_args: Vec::new(),
        },
        Dict::Instance { module, name, args } => {
            let instance = Expr::new(
                ExprKind::Global {
                    module: module.clone(),
                    name: format!("instance {name}"),
                    type_args: Vec::new(),
                },
                span,
            );
            ExprKind::Dicts(Box::new(instance), args.clone())
        }
        Dict::Super { dict, class } => ExprKind::Method {
            dict: (**dict).clone(),
            name: format!("${class}"),
            type_args: Vec::new(),
        },
        Dict::Site { place, definition } => ExprKind::Dictionary(vec![
            (
                "definition".into(),
                Expr::new(ExprKind::Lit(Lit::String(definition.clone())), span),
            ),
            (
                "place".into(),
                Expr::new(ExprKind::Lit(Lit::String(place.clone())), span),
            ),
        ]),
        Dict::Primitive { labels, .. } => ExprKind::List(
            labels
                .iter()
                .map(|label| Expr::new(ExprKind::Lit(Lit::String(label.clone())), span))
                .collect(),
        ),
        Dict::Structural { .. } => {
            let name = "$memory$dictionary";
            let identity = Expr::new(
                ExprKind::Lambda {
                    params: vec![Param {
                        pat: Pat::Bind(name.into()),
                        stage: Stage::Pure,
                        ty: None,
                        binder_types: Vec::new(),
                    }],
                    body: Box::new(Expr::new(ExprKind::Local(name.into()), span)),
                },
                span,
            );
            ExprKind::Dicts(Box::new(identity), vec![value.clone()])
        }
        Dict::Error => {
            return Err(unsupported("error dictionary", span));
        }
    };
    Ok(Expr::new(kind, span))
}

/// `adapter` as the contracts of known runtime bindings compare it:
/// `retention call` keeps nothing past the call, as a bare callback without
/// the line does, so a binding means the same with it or without.
pub(super) fn kept_for_call(adapter: &Adapter) -> Adapter {
    let mut adapter = adapter.clone();
    if adapter.retention == Some(Retention::Call) {
        adapter.retention = None;
    }
    adapter
}
